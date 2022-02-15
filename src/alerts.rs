use axum::extract::ws::Message as WsMessage;

#[derive(Clone, serde::Serialize, Debug)]
#[serde(tag = "type")]
pub enum AlertMessage {
    Message { alert_id: AlertId, text: String },
    Update { alert_id: AlertId },
}

#[derive(Clone)]
pub struct Alert {
    pub last_text: String,
    pub name: String,
}

impl Alert {
    pub fn new(last_text: String, name: String) -> Self { Self { last_text, name } }
}

#[aliri_braid::braid(serde)]
pub struct AlertId;

impl AlertMessage {
    pub fn alert_id(&self) -> &AlertIdRef {
        match self {
            AlertMessage::Message { alert_id, .. } => alert_id,
            AlertMessage::Update { alert_id } => alert_id,
        }
    }

    pub fn new_message(alert_id: AlertId, text: String) -> Self { Self::Message { alert_id, text } }

    pub(crate) fn to_message(&self) -> Result<WsMessage, eyre::Report> {
        Ok(WsMessage::Text(serde_json::to_string(self)?))
    }
}
