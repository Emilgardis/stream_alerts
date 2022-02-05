pub mod opts;
pub mod util;

use std::{error::Error, sync::Arc};

use axum::{
    body::HttpBody,
    extract::{
        ws::{Message, WebSocket},
        Extension, WebSocketUpgrade,
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
use tower_http::{services::ServeDir, trace::TraceLayer};
use twitch_api2::{
    client::ClientDefault,
    eventsub::Event,
    helix::{self},
    twitch_oauth2::{AppAccessToken, TwitchToken},
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
    let client: HelixClient<'static, _> = twitch_api2::HelixClient::with_client(
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

    let live = is_live(&opts.broadcaster_id, &client, &token).await?;

    let token = Arc::new(tokio::sync::Mutex::new(token));
    let (sender, recv) = watch::channel(live);
    let sender = Arc::new(sender);
    let sender2 = sender.clone();
    let client2 = client.clone();
    let token2 = token.clone();
    // check for new live streams every 10 minutes. If it was missed
    let broadcaster_id = opts.broadcaster_id.clone();
    let checker = async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(600));
        loop {
            let last = sender2.borrow().clone();
            interval.tick().await;
            match is_live(&broadcaster_id, &client2, &*token2.lock().await).await {
                Ok(live) => {
                    if live != last {
                        sender2.send(live).unwrap();
                    }
                }
                Err(e) => {
                    tracing::error!("{}", e);
                    if let Some(helix::HelixRequestGetError::Error {
                        status: hyper::StatusCode::FORBIDDEN,
                        ..
                    }) = e.root_cause().downcast_ref::<helix::HelixRequestGetError>()
                    {
                        tracing::warn!("Token needs to be refreshed");
                    }
                }
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), color_eyre::Report>(())
    };
    let client_id = opts.client_id.clone();
    let client_secret = opts.client_secret.clone();
    let client3 = client.clone();
    let refresher = async move {
        #[allow(clippy::never_loop)]
        loop {
            tracing::info!("hello!");
            tokio::time::sleep(
                token.lock().await.expires_in() - tokio::time::Duration::from_secs(20),
            )
            .await;
            let t = &mut *token.lock().await;
            *t = twitch_api2::twitch_oauth2::AppAccessToken::get_app_access_token(
                client3.get_client(),
                client_id.clone(),
                client_secret.clone(),
                vec![],
            )
            .await
            .unwrap();
        }
        #[allow(unreachable_code)]
        Ok::<(), color_eyre::Report>(())
    };

    let app = Router::new()
        .route(
            "/ws",
            get({
                let recv = recv.clone();
                move |ws| handler(ws, recv)
            }),
        )
        .route("/", get(move |uri| serve_index(uri, recv.borrow().clone())))
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
        .route("/twitch/eventsub", post(twitch_eventsub))
        .layer(AddExtensionLayer::new(client.clone()))
        .layer(AddExtensionLayer::new(sender.clone()))
        .layer(TraceLayer::new_for_http());

    let server = tokio::spawn(async move {
        axum::Server::bind(
            &"0.0.0.0:80"
                .parse()
                .wrap_err_with(|| "when parsing address")?,
        )
        .serve(app.into_make_service())
        .await
        .wrap_err_with(|| "when serving")?;
        Ok::<(), color_eyre::Report>(())
    });

    let r = tokio::try_join!(
        flatten(server),
        flatten(tokio::spawn(checker)),
        flatten(tokio::spawn(refresher))
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

#[derive(Template)]
#[template(path = "is_live.html")]
struct IsLiveTemplate {
    is_live: bool,
}

impl IsLiveTemplate {
    fn live() -> Self { Self { is_live: true } }

    fn offline() -> Self { Self { is_live: false } }
}

pub struct SignSecret {
    secret: Vec<u8>,
}

impl SignSecret {
    /// Get a reference to the sign secret.
    pub fn secret(&self) -> &[u8] { self.secret.as_ref() }
}

impl std::fmt::Debug for SignSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignSecret")
            .field("secret", &"[redacted]")
            .finish()
    }
}

impl std::str::FromStr for SignSecret {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SignSecret {
            secret: s.as_bytes().to_vec(),
        })
    }
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

#[derive(Debug, Clone, PartialEq)]
pub enum LiveStatus {
    Live { started_at: types::Timestamp },
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
            Self::Live { started_at } => Msg {
                html: "Yes".to_string(),
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

async fn twitch_eventsub(
    request: axum::http::Request<axum::body::Body>,
    Extension(sender): Extension<Arc<watch::Sender<LiveStatus>>>,
    Extension(opts): Extension<Arc<Opts>>,
) -> impl IntoResponse {
    const MAX_ALLOWED_RESPONSE_SIZE: u64 = 64 * 1024;

    let (parts, body) = request.into_parts();
    let response_content_length = match body.size_hint().upper() {
        Some(v) => v,
        None => MAX_ALLOWED_RESPONSE_SIZE + 1, /* Just to protect ourselves from a malicious response */
    };
    let body = if response_content_length < MAX_ALLOWED_RESPONSE_SIZE {
        hyper::body::to_bytes(body).await.unwrap()
    } else {
        panic!("too big data given")
    };

    let request = axum::http::Request::from_parts(parts, &*body);

    if !Event::verify_payload(&request, opts.sign_secret.secret()) {
        return (
            StatusCode::BAD_REQUEST,
            "Invalid payload signature".to_string(),
        );
    }
    // Event is verified, now do stuff.
    let event = Event::parse_http(&request).unwrap();

    if let Some(ver) = event.get_verification_request() {
        return (StatusCode::OK, ver.challenge.clone());
    }

    if event.is_revocation() {
        tracing::info!(event=?event, "subscription was revoked");
        return (StatusCode::OK, "".to_string());
    }
    use twitch_api2::eventsub::{Message as M, Payload as P};

    match event {
        Event::ChannelUpdateV1(P {
            message: M::Notification(notification),
            ..
        }) => {
            todo!()
        }
        Event::StreamOnlineV1(P {
            message: M::Notification(notification),
            ..
        }) => {
            sender
                .send(LiveStatus::Live {
                    started_at: notification.started_at.clone(),
                })
                .unwrap();
        }
        Event::StreamOfflineV1(P {
            message: M::Notification(notification),
            ..
        }) => sender.send(LiveStatus::Offline).unwrap(),
        _ => todo!(),
    }
    todo!()
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
