use std::{collections::HashMap, error::Error, path::Path, sync::Arc};

use askama::Template;
use axum::{
    extract::{
        self,
        ws::{self, WebSocket},
    },
    response::IntoResponse,
    routing::{get, post},
    Extension,
};
use eyre::Context;
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use hyper::StatusCode;
use rand::Rng;
use tokio::{
    fs::OpenOptions,
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{broadcast, RwLock},
};

use crate::opts::Opts;

pub async fn setup(opts: &Opts) -> Result<axum::Router, eyre::Report> {
    let (sender, _) = broadcast::channel(16);
    let map = Arc::new(RwLock::new(HashMap::<AlertId, Alert>::new()));
    read_alerts(&map, opts.db_path.clone()).await?;
    Ok(axum::Router::new()
        .route("/new", get(new_alert))
        .route("/new", post(new_alert_post))
        .route(
            "/ws/:id",
            get({
                let sender = sender.clone();
                move |ws, id, map| handler(ws, sender, id, map)
            }),
        )
        .route("/:id", get(serve_alert))
        .route(
            "/:id/update",
            get({
                let sender = sender.clone();
                move |id, map, opts, query| update_alert(sender, id, map, opts, query)
            }),
        )
        .route(
            "/:id/update/:field",
            get({
                let sender = sender.clone();
                move |id, map, opts, query| update_alert_field(sender, id, map, opts, query)
            }),
        )
        .layer(
            tower::ServiceBuilder::new()
                .layer(Extension(Arc::new(sender.clone())))
                .layer(Extension(map.clone())),
        ))
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
    values: HashMap<String, AlertField>,
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
    extract::Path(alert_id): extract::Path<AlertId>,
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

pub(crate) async fn read_alerts(
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

#[derive(serde::Deserialize)]
pub struct UpdateAlertQuery {
    alert_text: Option<AlertText>,
    api: Option<String>,
}

/// Update an existing alert and send the update to clients
async fn update_alert(
    sender: broadcast::Sender<AlertMessage>,
    extract::Path(alert_id): extract::Path<AlertId>,
    Extension(map): Extension<Arc<RwLock<HashMap<AlertId, Alert>>>>,
    Extension(opts): Extension<Arc<Opts>>,
    update: extract::Query<UpdateAlertQuery>,
) -> axum::response::Response {
    if let Some(text) = &update.alert_text {
        let mut map_w = map.write().await;
        let Some(alert) = map_w.get_mut(&alert_id) else {
            return (StatusCode::BAD_REQUEST, "no alert found").into_response();
        };
        alert.last_text = text.clone();
        let _ = sender.send(AlertMessage::new_message(alert_id.clone(), alert.render()));
        tracing::info!(count=sender.receiver_count(), "updated alert.");

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

#[derive(serde::Deserialize)]
pub struct UpdateAlertFieldQuery {
    incr: Option<i32>,
    decr: Option<i32>,
    set: Option<String>,
    new: Option<String>,
    kind: Option<String>,
}

async fn update_alert_field(
    sender: broadcast::Sender<AlertMessage>,
    extract::Path((alert_id, field)): extract::Path<(AlertId, String)>,
    Extension(map): Extension<Arc<RwLock<HashMap<AlertId, Alert>>>>,
    Extension(opts): Extension<Arc<Opts>>,
    extract::Query(update): extract::Query<UpdateAlertFieldQuery>,
) -> axum::response::Response {
    let mut map = map.write().await;
    let Some(alert) = map.get_mut(&alert_id) else {
        return (StatusCode::BAD_REQUEST, "no alert found").into_response();
    };

    let field = alert.values.entry(field);

    match (update, field) {
        (
            UpdateAlertFieldQuery {
                incr: Some(incr),
                decr: None,
                set: None,
                new: None,
                kind: _,
            },
            std::collections::hash_map::Entry::Occupied(mut entry),
        ) if entry.get().can_incr() => entry.get_mut().incr(incr),
        (
            UpdateAlertFieldQuery {
                incr: None,
                decr: Some(decr),
                set: None,
                new: None,
                kind: _,
            },
            std::collections::hash_map::Entry::Occupied(mut entry),
        ) if entry.get().can_incr() => entry.get_mut().incr(-decr),
        (
            UpdateAlertFieldQuery {
                incr: None,
                decr: None,
                set: Some(set),
                new: None,
                kind: _,
            },
            std::collections::hash_map::Entry::Occupied(mut entry),
        ) => entry.get_mut().set(set).unwrap(),
        (
            UpdateAlertFieldQuery {
                incr: None,
                decr: None,
                set: None,
                new: Some(value),
                kind: Some(kind),
            },
            entry,
        ) => match kind.as_str() {
            "counter" => {
                entry
                    .and_modify(|f| *f = AlertField::Counter(value.parse().unwrap()))
                    .or_insert(AlertField::Counter(value.parse().unwrap()));
            }
            "text" => {
                entry
                    .and_modify(|f| *f = AlertField::Text(value.clone()))
                    .or_insert(AlertField::Text(value.clone()));
            }
            _ => return (StatusCode::BAD_REQUEST, "invalid kind").into_response(),
        },
        _ => return (StatusCode::BAD_REQUEST, "invalid update requested").into_response(),
    };
    let _ = sender.send(AlertMessage::new_message(alert_id.clone(), alert.render()));

    alert.save_alert(&opts.db_path).await.expect("oops");

    (StatusCode::OK, "done!").into_response()
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
    form: extract::Form<NewAlertPostForm>,
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

pub(crate) async fn handler(
    ws: ws::WebSocketUpgrade,
    broadcast: broadcast::Sender<AlertMessage>,
    extract::Path(alert_id): extract::Path<AlertId>,
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
        if matches!(msg, ws::Message::Text(..)) {
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
    mut sender: SplitSink<WebSocket, ws::Message>,
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

#[derive(Clone, serde::Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertMessage {
    MessageMarkdown {
        alert_id: AlertId,
        #[serde(serialize_with = "alert_ser")]
        text: AlertMarkdown,
    },
    Update {
        alert_id: AlertId,
    },
}

fn alert_ser<S: serde::Serializer>(alert: &AlertMarkdown, ser: S) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    alert.to_markdown().serialize(ser)
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
#[serde(tag = "type")]
pub enum AlertMessageRecv {
    Init { alert_id: AlertId },
}

impl AlertMessageRecv {
    pub fn from_ws_message(message: &ws::Message) -> Result<Self, eyre::Report> {
        match message {
            ws::Message::Text(text) => {
                let message: AlertMessageRecv = serde_json::from_str(text)
                    .wrap_err("could not parse input as received message")?;
                Ok(message)
            }
            _ => Err(eyre::eyre!("invalid message type")),
        }
    }
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Alert {
    pub alert_id: AlertId,
    pub last_text: AlertText,
    pub name: AlertName,
    #[serde(default)]
    pub values: HashMap<String, AlertField>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize, Debug, PartialEq)]
pub enum AlertField {
    Text(String),
    Counter(i32),
}

impl AlertField {
    pub(crate) fn set(&mut self, set: String) -> Result<(), eyre::Report> {
        match self {
            AlertField::Text(text) => *text = set,
            AlertField::Counter(counter) => {
                let set: i32 = set.parse()?;
                *counter = set;
            }
        }
        Ok(())
    }
    pub fn can_incr(&self) -> bool {
        match self {
            AlertField::Text(_) => false,
            AlertField::Counter(_) => true,
        }
    }

    /// increment value, noop if not supported
    pub fn incr(&mut self, incr: i32) {
        match self {
            AlertField::Text(_) => {}
            AlertField::Counter(counter) => {
                *counter += incr;
            }
        }
    }
}

impl std::fmt::Display for AlertField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertField::Text(s) => write!(f, "{s}"),
            AlertField::Counter(i) => write!(f, "{i}"),
        }
    }
}

impl Alert {
    pub async fn save_alert(&self, db_path: impl AsRef<Path>) -> Result<(), eyre::Report> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(db_path.as_ref().to_owned().join(self.alert_id.as_str()))
            .await?;
        let json = serde_json::to_string(&self)?;
        file.write_all(json.as_bytes()).await?;
        Ok(())
    }

    pub async fn load_alert(path: impl AsRef<Path>) -> Result<Self, eyre::Report> {
        let mut file = OpenOptions::new().read(true).open(path).await?;
        let mut buf = vec![];
        file.read_to_end(&mut buf).await?;
        let alert: Self = serde_json::from_slice(&buf)?;
        Ok(alert)
    }

    pub fn new(alert_id: AlertId, last_text: AlertText, name: AlertName) -> Self {
        Self {
            alert_id,
            last_text,
            name,
            values: HashMap::new(),
        }
    }

    pub fn render(&self) -> AlertMarkdown {
        tracing::info!("and i op");
        let mut text = self.last_text.to_string();
        for (key, value) in &self.values {
            text = text.replace(&format!("${key}"), &value.to_string());
        }
        text = text.replace("$$", "$");

        AlertMarkdown::from(text)
    }
}

#[aliri_braid::braid(serde)]
pub struct AlertId;
#[aliri_braid::braid(serde)]
pub struct AlertName;
#[aliri_braid::braid(serde)]
pub struct AlertMarkdown;
#[aliri_braid::braid(serde)]
pub struct AlertText;

impl AlertMarkdownRef {
    pub fn to_markdown(&self) -> String {
        let mut options = comrak::ComrakOptions::default();
        options.extension.table = true;
        options.render.unsafe_ = true;
        comrak::markdown_to_html(self.as_str(), &options)
    }
}

impl AlertMessage {
    pub fn alert_id(&self) -> &AlertIdRef {
        match self {
            AlertMessage::Update { alert_id } => alert_id,
            AlertMessage::MessageMarkdown { alert_id, .. } => alert_id,
        }
    }

    pub fn new_message(alert_id: AlertId, text: AlertMarkdown) -> Self {
        Self::MessageMarkdown { alert_id, text }
    }

    pub(crate) fn to_message(&self) -> Result<ws::Message, eyre::Report> {
        Ok(ws::Message::Text(serde_json::to_string(self)?))
    }
}
