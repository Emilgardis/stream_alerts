#![warn(clippy::unwrap_in_result)]
pub mod opts;
pub mod twitch;
pub mod util;

pub use opts::SignSecret;
use twitch::LiveStatus;

use std::{error::Error, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::{get, get_service, post},
    AddExtensionLayer, Router,
};

use askama::Template;
use futures::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use opts::Opts;

use clap::Parser;
use eyre::Context;

use reqwest::StatusCode;
use tokio::{sync::watch, task::JoinHandle};
use tower_http::{catch_panic::CatchPanicLayer, services::ServeDir, trace::TraceLayer};
use twitch_api2::{client::ClientDefault, HelixClient};

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    let _ = util::install_utils()?;
    let opts = Opts::parse();

    tracing::debug!(
        "App started!\n{}",
        Opts::try_parse_from(&["app", "--version"])
            .unwrap_err()
            .to_string()
    );

    run(&opts)
        .await
        .with_context(|| "when running application")?;

    Ok(())
}

pub async fn run(opts: &Opts) -> eyre::Result<()> {
    let client: HelixClient<'static, _> = twitch_api2::HelixClient::with_client(
        <reqwest::Client>::default_client_with_name(Some(
            "is.sessis.live"
                .parse()
                .wrap_err_with(|| "when creating header name")
                .unwrap(),
        ))
        .wrap_err_with(|| "when creating client")?,
    );

    let token = twitch_api2::twitch_oauth2::AppAccessToken::get_app_access_token(
        &client,
        opts.client_id.clone(),
        opts.client_secret.clone(),
        vec![],
    )
    .await?;

    let broadcaster_id = client
        .get_channel_from_login(&*opts.broadcaster_login, &token)
        .await?
        .ok_or_else(|| eyre::eyre!("broadcaster not found"))?
        .broadcaster_id;

    let live = twitch::is_live(&broadcaster_id, &client, &token).await?;

    let token = Arc::new(tokio::sync::RwLock::new(token));
    let (sender, recv) = watch::channel(live);
    let sender = Arc::new(sender);
    let retainer = Arc::new(retainer::Cache::<axum::http::HeaderValue, ()>::new());
    let ret = retainer.clone();
    let retainer_cleanup = async move {
        ret.monitor(10, 0.50, tokio::time::Duration::from_secs(86400 / 2))
            .await;
        Ok::<(), eyre::Report>(())
    };

    let app = Router::new()
        .route(
            "/ws",
            get({
                let recv = recv.clone();
                move |ws| handler(ws, recv)
            }),
        )
        .route("/", get(move || serve_index(recv.borrow().clone())))
        .route(
            "/twitch/eventsub",
            post({
                let broadcaster_id = broadcaster_id.clone();
                move |sender, opts, cache, request| {
                    twitch::twitch_eventsub(sender, opts, cache, request, broadcaster_id)
                }
            }),
        )
        .nest(
            "/static",
            get_service(ServeDir::new("./static/")).handle_error(|error| async move {
                tracing::error!("{}", error);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Unhandled internal error".to_string(),
                )
            }),
        )
        .layer(
            tower::ServiceBuilder::new()
                //.layer(axum::error_handling::HandleErrorLayer::new(handle_error))
                .layer(AddExtensionLayer::new(client.clone()))
                .layer(AddExtensionLayer::new(sender.clone()))
                .layer(AddExtensionLayer::new(retainer.clone()))
                .layer(AddExtensionLayer::new(Arc::new(opts.clone())))
                .layer(TraceLayer::new_for_http().on_failure(
                    |error, _latency, _span: &tracing::Span| {
                        tracing::error!(error=%error);
                    },
                ))
                .layer(CatchPanicLayer::new()),
        );

    let server = tokio::spawn(async move {
        axum::Server::bind(
            &"0.0.0.0:80"
                .parse()
                .wrap_err_with(|| "when parsing address")?,
        )
        .serve(app.into_make_service())
        .await
        .wrap_err_with(|| "when serving")?;
        Ok::<(), eyre::Report>(())
    });
    tracing::info!("spinning up server!");
    let r = tokio::try_join!(
        flatten(server),
        flatten(tokio::spawn(twitch::checker(
            sender.clone(),
            client.clone(),
            broadcaster_id.clone(),
            token.clone()
        ))),
        flatten(tokio::spawn(twitch::refresher(
            client.clone(),
            token.clone(),
            opts.client_id.clone(),
            opts.client_secret.clone()
        ))),
        flatten(tokio::spawn(twitch::eventsub_register(
            token.clone(),
            client.clone(),
            opts.website_callback.clone(),
            broadcaster_id.clone(),
            opts.sign_secret.clone()
        ))),
        flatten(tokio::spawn(retainer_cleanup)),
    );
    r?;
    Ok(())
}

async fn flatten<T>(handle: JoinHandle<Result<T, eyre::Report>>) -> Result<T, eyre::Report> {
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(e) => Err(e).wrap_err_with(|| "handling failed"),
    }
}

async fn handle_error(err: axum::BoxError) -> impl IntoResponse {
    tracing::error!(error=%err, "error occured");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Unhandled internal error".to_string(),
    )
}

#[derive(Template)]
#[template(path = "is_live.html")]
struct IsLiveTemplate {
    is_live: bool,
    broadcaster_url: String,
}

impl IsLiveTemplate {
    fn live(broadcaster_url: String) -> Self {
        Self {
            is_live: true,
            broadcaster_url,
        }
    }

    fn offline(broadcaster_url: String) -> Self {
        Self {
            is_live: false,
            broadcaster_url,
        }
    }
}

async fn serve_index(live: LiveStatus) -> impl IntoResponse {
    if live.is_live() {
        IsLiveTemplate::live(live.url().clone())
    } else {
        IsLiveTemplate::offline(live.url().clone())
    }
}

async fn handler(ws: WebSocketUpgrade, watch: watch::Receiver<LiveStatus>) -> impl IntoResponse {
    ws.on_upgrade(|f| handle_socket(f, watch))
}

async fn handle_socket(
    socket: WebSocket,
    watch: watch::Receiver<LiveStatus>,
) -> Result<(), eyre::Report> {
    let (sender, receiver) = socket.split();

    tokio::try_join!(
        flatten(tokio::spawn(write(sender, watch))),
        flatten(tokio::spawn(read(receiver)))
    )
    .wrap_err_with(|| "in stream join")
    .map(|_| ())
}
// Reads, basically only responds to pongs. Should not be a need for refreshes, but maybe.
async fn read(mut receiver: SplitStream<WebSocket>) -> Result<(), eyre::Report> {
    while let Some(msg) = receiver.next().await {
        tracing::debug!(message = ?msg, "got message")
    }
    Ok(())
}

// Sends live status to clients.
async fn write(
    mut sender: SplitSink<WebSocket, Message>,
    mut watch: watch::Receiver<LiveStatus>,
) -> Result<(), eyre::Report> {
    while watch.changed().await.is_ok() {
        let val = watch.borrow().clone();
        if let Ok(msg) = val.to_message() {
            if let Err(error) = sender.send(msg).await {
                if let Some(e) = error.source() {
                    if let Some(tokio_tungstenite::tungstenite::error::Error::ConnectionClosed) =
                        e.downcast_ref()
                    {
                        // NOOP
                    } else {
                        Err(e).unwrap()
                    }
                }
            };
        }
    }
    Ok(())
}
