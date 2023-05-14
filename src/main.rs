#![warn(clippy::unwrap_in_result)]
#![warn(clippy::todo)]
pub mod alerts;
mod ip;
pub mod opts;
pub mod util;

pub use alerts::AlertMessage;
use alerts::{AlertMarkdown, AlertText};
use hyper::StatusCode;
use rand::Rng;
use std::{collections::HashMap, error::Error, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Extension, Form, Path, Query, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::{get, get_service, post},
    Router,
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
use tower_http::{
    catch_panic::CatchPanicLayer,
    services::ServeDir,
    trace::{DefaultMakeSpan, MakeSpan, TraceLayer},
};

use self::alerts::{Alert, AlertId, AlertName};

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    util::install_utils()?;
    let opts = Opts::parse();

    tracing::debug!(
        "App started!\n{}",
        Opts::try_parse_from(["app", "--version"])
            .unwrap_err()
            .to_string()
    );

    run(&opts)
        .await
        .with_context(|| "when running application")?;

    Ok(())
}

pub async fn run(opts: &Opts) -> eyre::Result<()> {
    let (sender, _) = broadcast::channel(16);
    let retainer = Arc::new(retainer::Cache::<axum::http::HeaderValue, ()>::new());
    let ret = retainer.clone();
    let retainer_cleanup = async move {
        ret.monitor(10, 0.50, tokio::time::Duration::from_secs(86400 / 2))
            .await;
        Ok::<(), eyre::Report>(())
    };
    let map = Arc::new(RwLock::new(HashMap::<AlertId, Alert>::new()));
    read_alerts(&map, opts.db_path.clone()).await?;
    let app = Router::new()
        .route(
            "/ws/:id",
            get({
                let sender = sender.clone();
                move |ws, id, map| handler(ws, sender, id, map)
            }),
        )
        .route("/alert/new", get(new_alert))
        .route("/alert/new", post(new_alert_post))
        .route("/alert/:id", get(serve_alert))
        .route(
            "/alert/:id/update",
            get({
                let sender = sender.clone();
                move |id, map, opts, query| update_alert(sender, id, map, opts, query)
            }),
        )
        .nest_service(
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
                .layer(Extension(Arc::new(sender.clone())))
                .layer(Extension(retainer.clone()))
                .layer(Extension(map.clone()))
                .layer(Extension(Arc::new(opts.clone())))
                .layer(
                    TraceLayer::new_for_http()
                        .on_failure(|error, _latency, _span: &tracing::Span| {
                            tracing::error!(error=%error);
                        })
                        .make_span_with(|request: &axum::http::Request<axum::body::Body>| {
                            DefaultMakeSpan::new()
                                //.include_headers(true)
                                .make_span(request)
                                .in_scope(|| {
                                    tracing::info_span!(
                                        "http-request",
                                        status_code = tracing::field::Empty,
                                        uri = tracing::field::display(request.uri()),
                                        method = tracing::field::display(request.method()),
                                        ip = tracing::field::Empty,
                                    )
                                })
                        })
                        .on_response(
                            |response: &axum::http::Response<_>,
                             _latency: std::time::Duration,
                             span: &tracing::Span| {
                                span.record(
                                    "status_code",
                                    &tracing::field::display(response.status()),
                                );

                                tracing::info!("response generated");
                            },
                        )
                        .on_request(
                            |request: &axum::http::Request<axum::body::Body>,
                             span: &tracing::Span| {
                                if let Some(ip) =
                                    ip::real_ip(request.headers(), request.extensions())
                                {
                                    span.record("ip", tracing::field::display(ip));
                                } else {
                                    span.record("ip", "<unknown>");
                                }
                                tracing::debug!("request received");
                            },
                        ),
                )
                .layer(CatchPanicLayer::new()),
        );

    let address = (opts.interface, opts.port).into();
    let server = tokio::spawn(async move {
        axum::Server::bind(&address)
            .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .wrap_err_with(|| "when serving")?;
        Ok::<(), eyre::Report>(())
    });
    tracing::info!("spinning up server! http://{}", address);
    let r = tokio::try_join!(flatten(server), flatten(tokio::spawn(retainer_cleanup)),);
    r?;
    Ok(())
}

async fn read_alerts(
    map: &RwLock<HashMap<AlertId, Alert>>,
    db_path: std::path::PathBuf,
) -> Result<(), eyre::Report> {
    let mut i = tokio::fs::read_dir(db_path).await?;
    let mut map = map.write().await;
    while let Some(entry) = i.next_entry().await? {
        if entry.file_type().await?.is_file() {
            let path = entry.path();
            let alert = Alert::load_alert(path).await?;

            map.insert(alert.alert_id.clone(), alert);
        }
    }
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
#[template(path = "alert.html", escape = "none")]
struct AlertSite {
    alert_id: AlertId,
    alert_name: AlertName,
    last_text: AlertMarkdown,
    cache_bust: String,
}

#[derive(Template)]
#[template(path = "update_alert.html")]
struct UpdateAlert {
    alert_name: AlertName,
    alert_id: AlertId,
    last_text: AlertText,
    cache_bust: String,
    values: HashMap<String, String>,
}

#[derive(Template)]
#[template(path = "new_alert.html")]
struct NewAlert {
    cache_bust: String,
}

#[derive(Template)]
#[template(path = "404.html")]
struct NotFound {
    id: String,
}

impl NotFound {
    fn new(id: String) -> Self {
        Self { id }
    }
}

impl AlertSite {
    pub fn new(alert_id: AlertId, alert_name: AlertName, last_text: AlertMarkdown) -> Self {
        Self {
            alert_id,
            alert_name,
            last_text,
            cache_bust: rand::thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(7)
                .map(char::from)
                .collect(),
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
    AlertSite::new(alert_id, alert.name.clone(), alert.render()).into_response()
}

#[derive(serde::Deserialize)]
pub struct UpdateAlertQuery {
    alert_text: Option<AlertText>,
    api: Option<String>,
}

/// Update an existing alert and send the update to clients
async fn update_alert(
    sender: broadcast::Sender<AlertMessage>,
    Path(alert_id): Path<AlertId>,
    Extension(map): Extension<Arc<RwLock<HashMap<AlertId, Alert>>>>,
    Extension(opts): Extension<Arc<Opts>>,
    update: Query<UpdateAlertQuery>,
) -> axum::response::Response {
    if let Some(text) = &update.alert_text {
        let mut map_w = map.write().await;
        let Some(alert) = map_w.get_mut(&alert_id) else {
            return (StatusCode::BAD_REQUEST, "no alert found").into_response();
        };
        alert.last_text = text.clone();
        let _ = sender.send(AlertMessage::new_message(alert_id.clone(), alert.render()));
        tracing::info!("updated alert.");
        alert.save_alert(&opts.db_path).await.expect("oops");
    }

    if update.api.is_some() {
        return (StatusCode::OK, "ok!").into_response();
    }

    let map_r = map.read().await;
    let alert = map_r.get(&alert_id).expect("no alert found");

    UpdateAlert {
        alert_name: alert.name.clone(),
        alert_id,
        last_text: alert.last_text.clone(),
        cache_bust: rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(7)
            .map(char::from)
            .collect(),
        values: alert.values.clone(),
    }
    .into_response()
}

async fn new_alert() -> axum::response::Response {
    NewAlert {
        cache_bust: rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(7)
            .map(char::from)
            .collect(),
    }
    .into_response()
}

#[derive(serde::Deserialize, Debug)]
pub struct NewAlertPostForm {
    alert_name: AlertName,
    alert_text: AlertText,
}
async fn new_alert_post(
    Extension(map): Extension<Arc<RwLock<HashMap<AlertId, Alert>>>>,
    Extension(opts): Extension<Arc<Opts>>,
    form: Form<NewAlertPostForm>,
) -> impl IntoResponse {
    let mut map = map.write().await;
    let alert_id: AlertId = nanoid::nanoid!().into();
    let alert = Alert::new(
        alert_id.clone(),
        form.alert_text.clone(),
        form.alert_name.clone(),
    );
    alert
        .save_alert(&opts.db_path)
        .await
        .expect("could not save file");
    map.insert(alert_id.clone(), alert);

    axum::response::Redirect::to(&format!("/alert/{alert_id}/update"))
}

async fn handler(
    ws: WebSocketUpgrade,
    broadcast: broadcast::Sender<AlertMessage>,
    Path(alert_id): Path<AlertId>,
    Extension(map): Extension<Arc<RwLock<HashMap<AlertId, Alert>>>>,
) -> impl IntoResponse {
    tracing::debug!("got call into handler");
    ws.on_upgrade(|f| async {
        let alert_id = alert_id;
        if let Some(err) = handle_socket(f, broadcast, alert_id.clone(), map)
            .await
            .err()
        {
            tracing::error!(error=%err, ?alert_id, "error occured");
        }
    })
}

async fn handle_socket(
    socket: WebSocket,
    broadcast: broadcast::Sender<AlertMessage>,
    alert_id: AlertId,
    map: Arc<RwLock<HashMap<AlertId, Alert>>>,
) -> Result<(), eyre::Report> {
    let (sender, receiver) = socket.split();

    tokio::select!(
        r = tokio::spawn(write(
            sender,
            broadcast.subscribe(),
            alert_id.clone()
        )) => {
            r
        }
        r = tokio::spawn(read(receiver, broadcast, map, alert_id)) => {
            r
        }
    )
    .wrap_err_with(|| "in stream join")
    .map(|_| ())
}
// Reads, basically only responds to pongs. Should not be a need for refreshes, but maybe.
async fn read(
    mut receiver: SplitStream<WebSocket>,
    _broadcast: broadcast::Sender<AlertMessage>,
    map: Arc<RwLock<HashMap<AlertId, Alert>>>,
    alert_id: AlertId,
) -> Result<(), eyre::Report> {
    while let Some(msg) = receiver.next().await {
        let msg = msg?;
        if matches!(msg, Message::Text(..)) {
            let map = map.read().await;
            if let Some(_alert) = map.get(&alert_id) {
                // TODO: This blasts out to all clients, maybe should nerf it.
                // broadcast
                //     .send(AlertMessage::new_message(
                //         alert_id.clone(),
                //         alert.last_text.clone(),
                //     ))
                //     .wrap_err("could not send message")?;
            }
        }
    }
    Ok(())
}

/// Watch for events and send to clients.
async fn write(
    mut sender: SplitSink<WebSocket, Message>,
    mut broadcast: broadcast::Receiver<AlertMessage>,
    alert_id: AlertId,
) -> Result<(), eyre::Report> {
    loop {
        let msg = broadcast.recv().await?;
        // Check if alert id matches
        if msg.alert_id() != alert_id {
            continue;
        }
        if let Ok(msg) = msg.to_message() {
            tracing::debug!("sending message to client");
            if let Err(error) = sender.send(msg).await {
                if let Some(e) = error.source() {
                    if let Some(tokio_tungstenite::tungstenite::error::Error::ConnectionClosed) =
                        e.downcast_ref()
                    {
                        return Ok(());
                    } else {
                        Err(error).wrap_err_with(|| "sending message to ws client failed")?
                    }
                }
            };
        }
    }
}
