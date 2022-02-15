#![warn(clippy::unwrap_in_result)]
#![warn(clippy::todo)]
pub mod alerts;
pub mod opts;
pub mod util;

pub use alerts::AlertMessage;
use hyper::StatusCode;
pub use opts::SignSecret;
use serde::Deserialize;
use std::{collections::HashMap, error::Error, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Extension, Form, Path, Query, WebSocketUpgrade,
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

use tokio::{
    sync::{broadcast, RwLock},
    task::JoinHandle,
};
use tower_http::{catch_panic::CatchPanicLayer, services::ServeDir, trace::TraceLayer};

use self::alerts::{Alert, AlertId};

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
    let mut map = Arc::new(RwLock::new(HashMap::<AlertId, Alert>::new()));
    map.write().await.insert(
        AlertId::new("Cf8GfmlGGEK_-XJ_k57hO"),
        Alert::new("hello".to_string(), "HAHAH".to_string()),
    );
    let app = Router::new()
        .route(
            "/ws/:id",
            get({
                let sender = sender.clone();
                move |ws, id| handler(ws, sender, id)
            }),
        )
        .route("/alert/new", get(new_alert))
        .route("/alert/new", post(new_alert_post))
        .route("/alert/:id", get(serve_alert))
        .route(
            "/alert/:id/update",
            get({
                let sender = sender.clone();
                move |id, map, query| update_alert(sender, id, map, query)
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
                .layer(AddExtensionLayer::new(Arc::new(sender.clone())))
                .layer(AddExtensionLayer::new(retainer.clone()))
                .layer(AddExtensionLayer::new(map.clone()))
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
    alert_id: AlertId,
    alert_name: String,
    last_text: String,
}

#[derive(Template)]
#[template(path = "update_alert.html")]
struct UpdateAlert {
    alert_name: String,
    last_text: String,
}

#[derive(Template)]
#[template(path = "new_alert.html")]
struct NewAlert {}

#[derive(Template)]
#[template(path = "404.html")]
struct NotFound {
    id: String,
}

impl NotFound {
    fn new(id: String) -> Self { Self { id } }
}

impl AlertSite {
    pub fn new(alert_id: AlertId, alert_name: String, last_text: String) -> Self {
        Self {
            alert_id,
            alert_name,
            last_text,
        }
    }
}

async fn serve_alert(
    Path(alert_id): Path<AlertId>,
    Extension(map): Extension<Arc<RwLock<HashMap<AlertId, Alert>>>>,
) -> axum::response::Response {
    let alert: Alert = {
        let map_r = map.read().await;
        if let Some(alert) = map_r.get(&alert_id) {
            alert.clone()
        } else {
            return NotFound::new(alert_id.to_string()).into_response();
        }
    };
    AlertSite::new(alert_id, alert.name.clone(), alert.last_text.clone()).into_response()
}

#[derive(serde::Deserialize)]
pub struct UpdateAlertQuery {
    text: Option<String>,
}

/// Update an existing alert and send the update to clients
async fn update_alert(
    sender: broadcast::Sender<AlertMessage>,
    Path(alert_id): Path<AlertId>,
    Extension(map): Extension<Arc<RwLock<HashMap<AlertId, Alert>>>>,
    update: Query<UpdateAlertQuery>,
) -> impl IntoResponse {
    if let Some(text) = &update.text {
        let mut map_w = map.write().await;
        if let Some(alert) = map_w.get_mut(&alert_id) {
            alert.last_text = text.clone();
        }
        sender
            .send(AlertMessage::new_message(alert_id.clone(), text.clone()))
            .unwrap();
    }

    let map_r = map.read().await;
    let alert = map_r.get(&alert_id).expect("no alert found");

    UpdateAlert {
        alert_name: alert.name.clone(),
        last_text: alert.last_text.clone(),
    }
}

async fn new_alert() -> axum::response::Response { NewAlert {}.into_response() }

#[derive(serde::Deserialize, Debug)]
pub struct NewAlertPostForm {
    alert_name: String,
    alert_text: String,
}
async fn new_alert_post(
    Extension(map): Extension<Arc<RwLock<HashMap<AlertId, Alert>>>>,
    form: Form<NewAlertPostForm>,
) -> impl IntoResponse {
    let mut map = map.write().await;
    let alert_id: AlertId = nanoid::nanoid!().into();
    map.insert(
        alert_id.clone(),
        Alert::new(form.alert_name.clone(), form.alert_text.clone()),
    );

    AlertSite::new(alert_id, form.alert_name.clone(), form.alert_text.clone())
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
        if msg.alert_id() != alert_id {
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
