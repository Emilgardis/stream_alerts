use std::{collections::HashMap, path::Path};

use axum::extract::ws::Message as WsMessage;
use eyre::Context;
use tokio::{
    fs::OpenOptions,
    io::{AsyncReadExt, AsyncWriteExt},
};

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
    pub fn from_ws_message(message: &WsMessage) -> Result<Self, eyre::Report> {
        match message {
            WsMessage::Text(text) => {
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

    pub(crate) fn to_message(&self) -> Result<WsMessage, eyre::Report> {
        Ok(WsMessage::Text(serde_json::to_string(self)?))
    }
}
