#![warn(clippy::unwrap_in_result)]
pub mod alerts;
pub mod opts;
pub mod util;

pub use alerts::AlertMessage;
use hyper::StatusCode;
pub use opts::SignSecret;
use std::{error::Error, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, WebSocketUpgrade,
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

use tokio::{sync::broadcast, task::JoinHandle};
use tower_http::{catch_panic::CatchPanicLayer, services::ServeDir, trace::TraceLayer};

use self::alerts::AlertId;

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
    let (sender, recv) = broadcast::channel(16);
    let retainer = Arc::new(retainer::Cache::<axum::http::HeaderValue, ()>::new());
    let ret = retainer.clone();
    let retainer_cleanup = async move {
        ret.monitor(10, 0.50, tokio::time::Duration::from_secs(86400 / 2))
            .await;
        Ok::<(), eyre::Report>(())
    };

    let app = Router::new()
        .route(
            "/ws/:id",
            get({
                let sender = sender.clone();
                move |ws, id| handler(ws, sender, id)
            }),
        )
        .route("/alert/:id", get(serve_alert))
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
                .layer(AddExtensionLayer::new(Arc::new(sender.clone())))
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
    let r = tokio::try_join!(flatten(server), flatten(tokio::spawn(retainer_cleanup)),);
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
#[template(path = "alert.html")]
struct AlertSite {
    alert_name: String,
}

impl AlertSite {
    pub fn new(alert_id: AlertId) -> Self { todo!() }
}

async fn serve_alert(Path(alert_id): Path<AlertId>) -> impl IntoResponse {
    AlertSite::new(alert_id)
}

async fn handler(
    ws: WebSocketUpgrade,
    watch: broadcast::Sender<AlertMessage>,
    Path(alert_id): Path<AlertId>,
) -> impl IntoResponse {
    let recv = watch.subscribe();
    ws.on_upgrade(|f| handle_socket(f, recv, alert_id))
}

async fn handle_socket(
    socket: WebSocket,
    watch: broadcast::Receiver<AlertMessage>,
    alert_id: AlertId,
) -> Result<(), eyre::Report> {
    let (sender, receiver) = socket.split();

    tokio::try_join!(
        flatten(tokio::spawn(write(sender, watch, alert_id))),
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

/// Watch for events and send to clients.
async fn write(
    mut sender: SplitSink<WebSocket, Message>,
    mut watch: broadcast::Receiver<AlertMessage>,
    alert_id: AlertId,
) -> Result<(), eyre::Report> {
    loop {
        let msg = watch.recv().await?;
        // Check if alert id matches
        if msg.alert_id != alert_id {
            continue;
        }
        if let Ok(msg) = msg.to_message() {
            if let Err(error) = sender.send(msg).await {
                if let Some(e) = error.source() {
                    if let Some(tokio_tungstenite::tungstenite::error::Error::ConnectionClosed) =
                        e.downcast_ref()
                    {
                        // NOOP
                    } else {
                        Err(error).wrap_err_with(|| "sending message to ws client failed")?
                    }
                }
            };
        }
    }
    Ok(())
}
