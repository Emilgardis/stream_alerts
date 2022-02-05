pub mod opts;
pub mod util;

use std::{error::Error, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::{get, get_service},
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
use tokio::sync::watch;
use tower_http::{services::ServeDir, trace::TraceLayer};
use twitch_api2::{
    client::ClientDefault,
    helix::{self},
    twitch_oauth2::AppAccessToken,
    types::{self, UserIdRef},
    HelixClient,
};

#[tokio::main]
async fn main() -> Result<(), color_eyre::Report> {
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

pub async fn run(opts: &Opts) -> color_eyre::Result<()> {
    let client = twitch_api2::HelixClient::with_client(
        <reqwest::Client>::default_client_with_name(Some(
            "is.sessis.live"
                .parse()
                .wrap_err_with(|| "when creating header name")?,
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

    let live = is_live("80525799".into(), &client, &token).await?;

    let (sender, recv) = watch::channel(live);
    let sender = Arc::new(sender);
    let sender2 = sender.clone();
    // spoofing changes every 10 seconds.
    let spoof = async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            sender2
                .send(LiveStatus::Live {
                    game: "Fortnite".into(),
                    game_id: "1234".into(),
                    title: "a title".into(),
                    viewers: 122,
                    started_at: types::Timestamp::now(),
                })
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            sender2.send(LiveStatus::Offline).unwrap();
        }
    };

    let app = Router::new()
        .route(
            "/ws",
            get({
                let recv = recv.clone();
                move |ws| handler(ws, recv)
            }),
        )
        .route(
            "/",
            get({ move |uri| serve_index(uri, recv.borrow().clone()) }),
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
        .layer(AddExtensionLayer::new(sender.clone()))
        .layer(TraceLayer::new_for_http());

    let server = tokio::spawn(
        axum::Server::bind(
            &"0.0.0.0:80"
                .parse()
                .wrap_err_with(|| "when parsing address")?,
        )
        .serve(app.into_make_service()),
    );

    tokio::join!(server, tokio::spawn(spoof));
    Ok(())
}

#[derive(Template)]
#[template(path = "is_live.html")]
struct IsLiveTemplate {
    is_live: bool,
}

impl IsLiveTemplate {
    fn live() -> Self { Self { is_live: true } }

    fn offline() -> Self { Self { is_live: false } }
}

#[tracing::instrument(skip(client, token))]
pub async fn is_live(
    channel: &UserIdRef,
    client: &HelixClient<'_, reqwest::Client>,
    token: &AppAccessToken,
) -> color_eyre::Result<LiveStatus> {
    if let Some(stream) = client
        .req_get(
            helix::streams::get_streams::GetStreamsRequest::builder()
                .user_id(vec![channel.to_owned()])
                .build(),
            token,
        )
        .await
        .wrap_err_with(|| "could not check live streams")?
        .data
        .get(0)
    {
        Ok(LiveStatus::Live {
            game: stream.game_name.clone(),
            game_id: stream.game_id.clone(),
            title: stream.title.clone(),
            viewers: stream.viewer_count,
            started_at: stream.started_at.clone(),
        })
    } else {
        let channel = client
            .get_channel_from_id(channel, token)
            .await?
            .ok_or_else(|| eyre::eyre!("channel not found"))?;

        Ok(LiveStatus::Offline)
    }
}

#[derive(Debug, Clone)]
pub enum LiveStatus {
    Live {
        game: String,
        game_id: types::CategoryId,
        title: String,
        viewers: usize,
        started_at: types::Timestamp,
    },
    Offline,
}

impl LiveStatus {
    /// Returns `true` if the live status is [`Live`].
    ///
    /// [`Live`]: LiveStatus::Live
    pub fn is_live(&self) -> bool { matches!(self, Self::Live { .. }) }

    /// Returns `true` if the live status is [`Offline`].
    ///
    /// [`Offline`]: LiveStatus::Offline
    pub fn is_offline(&self) -> bool { matches!(self, Self::Offline) }

    pub fn to_message(&self) -> color_eyre::Result<Message> {
        #[derive(serde::Serialize)]
        struct Msg {
            html: String,
        }
        let msg = match self {
            Self::Live {
                game,
                game_id,
                title,
                viewers,
                started_at,
            } => Msg {
                html: "yes!".to_string(),
            },
            Self::Offline => Msg {
                html: "No".to_string(),
            },
        };
        Ok(Message::Text(
            serde_json::to_string(&msg).wrap_err_with(|| "could not make into a message")?,
        ))
    }
}

async fn serve_index(uri: axum::http::Uri, live: LiveStatus) -> impl IntoResponse {
    if live.is_live() {
        IsLiveTemplate::live()
    } else {
        IsLiveTemplate::offline()
    }
}

async fn handler(ws: WebSocketUpgrade, watch: watch::Receiver<LiveStatus>) -> impl IntoResponse {
    ws.on_upgrade(|f| handle_socket(f, watch))
}

async fn handle_socket(socket: WebSocket, watch: watch::Receiver<LiveStatus>) {
    let (sender, receiver) = socket.split();

    tokio::join!(
        tokio::spawn(write(sender, watch)),
        tokio::spawn(read(receiver))
    );
}
// Reads, basically only responds to pongs. Should not be a need for refreshes, but maybe.
async fn read(mut receiver: SplitStream<WebSocket>) {
    while let Some(msg) = receiver.next().await {
        tracing::info!(message = ?msg, "got message")
    }
}

// Sends live status to clients.
async fn write(mut sender: SplitSink<WebSocket, Message>, mut watch: watch::Receiver<LiveStatus>) {
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
}
