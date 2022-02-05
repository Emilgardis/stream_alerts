pub mod opts;
pub mod util;

use std::net;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use opts::Opts;

use clap::Parser;
use eyre::Context;

use tokio::sync::watch;
use twitch_api2::{
    client::ClientDefault,
    helix::{self, Response as HelixResponse},
    twitch_oauth2::AppAccessToken,
    types::{self, UserId, UserIdRef},
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
        .with_context(|| "When running application")?;

    Ok(())
}

pub async fn run(opts: &Opts) -> color_eyre::Result<()> {
    let client = twitch_api2::HelixClient::with_client(
        <reqwest::Client>::default_client_with_name(Some(
            "is.sessis.live"
                .parse()
                .wrap_err_with(|| "when creating header name")?,
        ))
        .wrap_err_with(|| "When creating client")?,
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

    let app = Router::new().route(
        "/ws",
        get({
            let recv = recv.clone();
            move |ws| handler(ws, recv)
        }),
    );

    axum::Server::bind(
        &"0.0.0.0:80"
            .parse()
            .wrap_err_with(|| "When parsing address")?,
    )
    .serve(app.into_make_service())
    .await
    .with_context(|| "When serving")?;
    todo!()
}

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

async fn handler(ws: WebSocketUpgrade, watch: watch::Receiver<LiveStatus>) -> impl IntoResponse {
    ws.on_upgrade(|f| handle_socket(f, watch))
}

async fn handle_socket(socket: WebSocket, watch: watch::Receiver<LiveStatus>) {
    let (sender, receiver) = socket.split();

    tokio::join!(
        tokio::spawn(write(sender, todo!())),
        tokio::spawn(read(receiver))
    );
}

async fn read(mut receiver: SplitStream<WebSocket>) {
    while let Some(msg) = receiver.next().await {}
}

async fn write(mut sender: SplitSink<WebSocket, Message>, mut watch: watch::Receiver<bool>) {
    while let Ok(watch) = watch.changed().await {}
}
