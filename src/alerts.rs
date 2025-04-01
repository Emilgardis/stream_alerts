use askama::Template;

#[cfg(feature = "ssr")]
use axum::{extract, http::StatusCode};
#[cfg(feature = "ssr")]
use axum::{
    extract::ws::{self, WebSocket},
    response::IntoResponse,
    routing::get,
    Extension,
};
use eyre::Context;
use futures::{
    stream::{SplitSink, SplitStream},
    StreamExt,
};
use leptos::{prelude::*, server};
use rand::Rng;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::Arc,
};
#[cfg(feature = "ssr")]
use tokio::{
    fs::OpenOptions,
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{broadcast, RwLock},
};

use crate::opts::Opts;

#[derive(Clone)]
#[cfg(feature = "ssr")]

pub struct AlertManager {
    alerts: Arc<RwLock<HashMap<AlertId, Alert>>>,
    pub sender: broadcast::Sender<AlertMessage>,
    pub db_path: std::path::PathBuf,
}

#[cfg(feature = "ssr")]
impl AlertManager {
    pub async fn read_alerts(&self) -> tokio::sync::RwLockReadGuard<HashMap<AlertId, Alert>> {
        self.alerts.read().await
    }

    pub async fn edit_alert(
        &self,
        alert_id: &AlertId,
        f: impl FnOnce(&mut Alert) + 'static,
    ) -> Result<(), leptos::server_fn::ServerFnError> {
        self.try_edit_alert::<std::convert::Infallible>(alert_id, |a| {
            f(a);
            Ok(())
        })
        .await
        .unwrap()?;
        Ok(())
    }

    pub async fn try_edit_alert<E>(
        &self,
        alert_id: &AlertId,
        f: impl (FnOnce(&mut Alert) -> Result<(), E>) + 'static,
    ) -> Result<Result<(), leptos::server_fn::ServerFnError>, E> {
        let mut map_w = self.alerts.write().await;
        let Some(alert) = map_w.get_mut(alert_id) else {
            return Ok(Err(ServerFnError::ServerError("no such alert".to_owned())));
        };
        f(alert)?;
        let _ = self
            .sender
            .send(AlertMessage::new_message(alert_id.clone(), alert.render()));
        let _ = self.sender.send(AlertMessage::new_style(
            alert_id.clone(),
            alert.render_style(),
        ));
        tracing::info!(count = self.sender.receiver_count(), "updated alert.");

        alert.save_alert(&self.db_path).await.expect("oops");
        Ok(Ok(()))
    }

    pub async fn new_alert(&self, alert: Alert) -> Result<(), leptos::server_fn::ServerFnError> {
        {
            let mut map_w = self.alerts.write().await;
            alert.save_alert(&self.db_path).await.expect("oops");
            map_w.insert(alert.alert_id.clone(), alert.clone());
        }

        let _ = self.sender.send(AlertMessage::new_message(
            alert.alert_id.clone(),
            alert.render(),
        ));
        tracing::info!(count = self.sender.receiver_count(), "updated alert.");

        Ok(())
    }
}

#[server(ReadAlert, "/backend")]
#[tracing::instrument(err)]
pub async fn read_alert(alert: AlertId) -> Result<Alert, ServerFnError> {
    let Some(alerts): Option<AlertManager> = use_context() else {
        tracing::info!("manager not found!");
        return Err(
            ServerFnError::<leptos::server_fn::error::NoCustomError>::ServerError(
                "Missing manager".to_owned(),
            ),
        );
    };

    // do some server-only work here to access the database
    let alerts = alerts.alerts.read().await;
    tracing::info!("alert_id = {alert}, alerts: {alerts:?}");
    Ok(alerts
        .get(&alert)
        .ok_or_else(|| {
            ServerFnError::<leptos::server_fn::error::NoCustomError>::ServerError(
                "alert not found".to_owned(),
            )
        })?
        .clone())
}

#[server(ReadAllAlerts, "/backend")]
#[tracing::instrument(err)]
pub async fn read_all_alerts() -> Result<Vec<(AlertId, Alert)>, ServerFnError> {
    // do some server-only work here to access the database
    let Some(alerts): Option<AlertManager> = use_context() else {
        tracing::info!("manager not found!");
        return Err(
            ServerFnError::<leptos::server_fn::error::NoCustomError>::ServerError(
                "Missing manager".to_owned(),
            ),
        );
    };
    let alerts = alerts.alerts.read().await;
    Ok(alerts.clone().into_iter().collect())
}

#[cfg(feature = "ssr")]
pub async fn setup<S>(opts: &Opts) -> Result<(axum::Router<S>, AlertManager), eyre::Report> {
    use axum::{
        extract::{Path, State, WebSocketUpgrade},
        response::IntoResponse,
        routing::get,
        Router,
    };

    let (sender, _) = broadcast::channel(16);
    let map = Arc::new(RwLock::new(HashMap::<AlertId, Alert>::new()));
    read_alerts(&map, opts.db_path.clone()).await?;

    let manager = AlertManager {
        alerts: map.clone(),
        sender: sender.clone(),
        db_path: opts.db_path.clone(),
    };

    let app = Router::new()
        .route("/ws/:id", get(handler))
        .route("/:id", get(serve_alert))
        .route("/:id/update/:field", get(update_alert_field))
        .with_state(sender.clone())
        .with_state(map.clone());

    Ok((app, manager))
}

#[derive(Template)]
#[template(path = "alert.html", escape = "none")]
struct AlertSite {
    alert_id: AlertId,
    alert_name: AlertName,
    last_text: AlertMarkdown,
    cache_bust: String,
    style: String,
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
    pub fn new(
        alert_id: AlertId,
        alert_name: AlertName,
        last_text: AlertMarkdown,
        style: String,
    ) -> Self {
        Self {
            alert_id,
            alert_name,
            last_text,
            cache_bust: rand::thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(7)
                .map(char::from)
                .collect(),
            style,
        }
    }
}

#[cfg(feature = "ssr")]
async fn serve_alert(
    extract::Path(alert_id): extract::Path<AlertId>,
    Extension(manager): Extension<AlertManager>,
) -> impl axum::response::IntoResponse {
    let alert: Alert = {
        if let Some(alert) = manager.read_alerts().await.get(&alert_id) {
            alert.clone()
        } else {
            // TODO: rdirect to leptos 404
            return axum::response::Html(
                NotFound::new(alert_id.to_string())
                    .render()
                    .unwrap_or_default(),
            );
        }
    };
    axum::response::Html(
        AlertSite::new(
            alert_id,
            alert.name.clone(),
            alert.render(),
            alert.render_style(),
        )
        .render()
        .unwrap_or_default(),
    )
}

#[cfg(feature = "ssr")]
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

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct UpdateAlertQuery {
    alert_text: Option<AlertText>,
    api: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct UpdateAlertField {
    incr: Option<i32>,
    decr: Option<i32>,
    set: Option<String>,
    new: Option<String>,
    kind: Option<String>,
}

#[cfg(feature = "ssr")]
async fn update_alert_field(
    extract::Path((alert_id, field)): extract::Path<(AlertId, AlertFieldName)>,
    Extension(manager): Extension<AlertManager>,
    extract::Query(update): extract::Query<UpdateAlertField>,
) -> axum::response::Response {
    match manager
        .try_edit_alert(&alert_id, |a| {
            a.update_field(Some(field), None, update)
                .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()).into_response())
        })
        .await
    {
        Ok(Ok(_)) => (StatusCode::OK, "done!").into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        Err(e) => e,
    }
}

#[cfg(feature = "ssr")]
#[axum::debug_handler]
pub(crate) async fn handler(
    ws: ws::WebSocketUpgrade,
    extract::State(broadcast): extract::State<broadcast::Sender<AlertMessage>>,
    extract::Path(alert_id): extract::Path<AlertId>,
    Extension(manager): Extension<AlertManager>,
) -> impl IntoResponse {
    tracing::debug!("handling ws connection");
    ws.on_upgrade(|f| async {
        let alert_id = alert_id;
        if let Some(err) = handle_socket(f, broadcast, alert_id.clone(), manager)
            .await
            .err()
        {
            tracing::error!(error=%err, ?alert_id, "error occured");
        }
    })
}

#[cfg(feature = "ssr")]
async fn handle_socket(
    socket: WebSocket,
    broadcast: broadcast::Sender<AlertMessage>,
    alert_id: AlertId,
    manager: AlertManager,
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
        r = tokio::spawn(read(receiver, broadcast, manager, alert_id)) => {
            r
        }
    )
    .wrap_err_with(|| "in stream join")
    .map(|_| ())
}
// Reads, basically only responds to pongs. Should not be a need for refreshes, but maybe.
#[cfg(feature = "ssr")]
async fn read(
    mut receiver: SplitStream<WebSocket>,
    _broadcast: broadcast::Sender<AlertMessage>,
    manager: AlertManager,
    alert_id: AlertId,
) -> Result<(), eyre::Report> {
    while let Some(msg) = receiver.next().await {
        let msg = msg?;
        if matches!(msg, ws::Message::Text(..)) {
            let map = manager.read_alerts().await;
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
#[cfg(feature = "ssr")]
async fn write(
    mut sender: SplitSink<WebSocket, ws::Message>,
    mut broadcast: broadcast::Receiver<AlertMessage>,
    alert_id: AlertId,
) -> Result<(), eyre::Report> {
    use std::error::Error as _;

    use futures::SinkExt as _;

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
    Style {
        alert_id: AlertId,
        style: String,
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
    #[cfg(feature = "ssr")]
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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Alert {
    pub alert_id: AlertId,
    pub last_text: AlertText,
    #[serde(default)]
    pub last_style: String,
    pub name: AlertName,
    #[serde(default)]
    pub fields: BTreeMap<AlertFieldId, (AlertFieldName, AlertField)>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize, Debug, PartialEq)]
pub enum AlertField {
    Text(String),
    Counter(i32),
}

impl Default for AlertField {
    fn default() -> Self {
        Self::Text(String::new())
    }
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
    pub fn entry_field_name(
        &mut self,
        field_name: AlertFieldName,
    ) -> Option<std::collections::btree_map::Entry<'_, AlertFieldId, (AlertFieldName, AlertField)>>
    {
        let id = self
            .fields
            .iter()
            .find_map(|(id, (name, _))| (name == &field_name).then(|| id.clone()));
        id.map(|id| self.fields.entry(id))
    }
}

impl Alert {
    #[cfg(feature = "ssr")]
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

    #[cfg(feature = "ssr")]
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
            last_style: String::new(),
            name,
            fields: BTreeMap::new(),
        }
    }

    pub fn render(&self) -> AlertMarkdown {
        tracing::info!("and i op");
        let mut text = self.last_text.to_string();
        for (name, value) in self.fields.values() {
            text = text.replace(&format!("${name}"), &value.to_string());
        }
        text = text.replace("$$", "$");

        AlertMarkdown::from(text)
    }
    pub fn render_style(&self) -> String {
        tracing::info!("and i op style");
        let mut text = self.last_style.to_string();
        for (name, value) in self.fields.values() {
            text = text.replace(&format!("${name}"), &value.to_string());
        }
        text = text.replace("$$", "$");

        text
    }

    fn update_field(
        &mut self,
        name: Option<AlertFieldName>,
        id: Option<AlertFieldId>,
        update: UpdateAlertField,
    ) -> Result<(), eyre::Report> {
        let field = match (name, id) {
            (_, Some(id)) => self.fields.entry(id),
            (Some(name), None) => self
                .entry_field_name(name)
                .ok_or_else(|| eyre::eyre!("no such field"))?,
            (None, None) => eyre::bail!("no field name or id provided"),
        };

        match (update, field) {
            (
                UpdateAlertField {
                    incr: Some(incr),
                    decr: None,
                    set: None,
                    new: None,
                    kind: _,
                },
                std::collections::btree_map::Entry::Occupied(mut entry),
            ) if entry.get().1.can_incr() => entry.get_mut().1.incr(incr),
            (
                UpdateAlertField {
                    incr: None,
                    decr: Some(decr),
                    set: None,
                    new: None,
                    kind: _,
                },
                std::collections::btree_map::Entry::Occupied(mut entry),
            ) if entry.get().1.can_incr() => entry.get_mut().1.incr(-decr),
            (
                UpdateAlertField {
                    incr: None,
                    decr: None,
                    set: Some(set),
                    new: None,
                    kind: _,
                },
                std::collections::btree_map::Entry::Occupied(mut entry),
            ) => entry.get_mut().1.set(set)?,
            (
                UpdateAlertField {
                    incr: None,
                    decr: None,
                    set: None,
                    new: Some(value),
                    kind: Some(kind),
                },
                std::collections::btree_map::Entry::Occupied(mut entry),
            ) => match kind.as_str() {
                "counter" => {
                    let value = value.parse()?;
                    entry.get_mut().1 = AlertField::Counter(value);
                }
                "text" => {
                    entry.get_mut().1 = AlertField::Text(value);
                }
                _ => return Err(eyre::eyre!("invalid kind")),
            },
            _ => return Err(eyre::eyre!("invalid update requested")),
        };

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn add_alert_field(
        &mut self,
        name: AlertFieldName,
        kind: &str,
        value: String,
    ) -> Result<(), eyre::Report> {
        tracing::debug!("adding new field");
        match kind {
            "counter" => self.fields.insert(
                AlertFieldId::new_id(),
                (name, AlertField::Counter(value.parse()?)),
            ),
            "text" => self
                .fields
                .insert(AlertFieldId::new_id(), (name, AlertField::Text(value))),
            _ => return Err(eyre::eyre!("invalid kind")),
        };
        Ok(())
    }
}

macro_rules! attr_type {
    ($attr_type:ty) => {
        impl leptos::attr::IntoAttributeValue for $attr_type {
            type Output = String;
            fn into_attribute_value(self) -> Self::Output {
                self.to_string().into_attribute_value()
            }
        }
        impl<'a> AddAnyAttr for $attr_type {
            type Output<SomeNewAttr: leptos::tachys::html::attribute::Attribute> = String;

            fn add_any_attr<NewAttr: leptos::tachys::html::attribute::Attribute>(
                self,
                _attr: NewAttr,
            ) -> Self::Output<NewAttr> {
                self.to_string()
            }
        }

        impl Render for $attr_type {
            type State = leptos::tachys::view::strings::StringState;

            fn build(self) -> Self::State {
                self.to_string().build()
            }
            fn rebuild(self, state: &mut Self::State) {
                self.to_string().rebuild(state)
            }
        }
        impl RenderHtml for $attr_type {
            type AsyncOutput = String;
            const MIN_LENGTH: usize = 0;
            fn dry_resolve(&mut self) {
                // no-op
            }
            async fn resolve(self) -> String {
                self.to_string()
            }
            fn to_html_with_buf(
                self,
                buf: &mut String,
                position: &mut leptos::tachys::view::Position,
                escape: bool,
                mark_branches: bool,
            ) {
                <&str as RenderHtml>::to_html_with_buf(
                    self.as_str(),
                    buf,
                    position,
                    escape,
                    mark_branches,
                )
            }
            fn hydrate<const FROM_SERVER: bool>(
                self,
                cursor: &leptos::tachys::hydration::Cursor,
                position: &leptos::tachys::view::PositionState,
            ) -> Self::State {
                self.to_string().hydrate::<FROM_SERVER>(cursor, position)
            }
        }
    };
}

#[aliri_braid::braid(serde)]
pub struct AlertId;
attr_type!(AlertId);

impl AlertId {
    pub fn new_id() -> Self {
        Self(nanoid::nanoid!())
    }
}

#[aliri_braid::braid(serde)]
pub struct AlertName;
attr_type!(AlertName);

#[aliri_braid::braid(serde)]
pub struct AlertMarkdown;

#[aliri_braid::braid(serde)]
pub struct AlertText;
attr_type!(AlertText);

#[aliri_braid::braid(serde)]
pub struct AlertFieldName;
attr_type!(AlertFieldName);

#[aliri_braid::braid(serde)]
pub struct AlertFieldId;
attr_type!(AlertFieldId);

impl Default for AlertFieldId {
    fn default() -> Self {
        Self::new_id()
    }
}

impl AlertFieldId {
    pub fn new_id() -> Self {
        Self(nanoid::nanoid!(4))
    }
}

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
            AlertMessage::Style { alert_id, .. } => alert_id,
        }
    }

    pub fn new_message(alert_id: AlertId, text: AlertMarkdown) -> Self {
        Self::MessageMarkdown { alert_id, text }
    }
    pub fn new_style(alert_id: AlertId, style: String) -> Self {
        Self::Style { alert_id, style }
    }

    #[cfg(feature = "ssr")]
    pub(crate) fn to_message(&self) -> Result<ws::Message, eyre::Report> {
        Ok(ws::Message::Text(serde_json::to_string(self)?))
    }
}
