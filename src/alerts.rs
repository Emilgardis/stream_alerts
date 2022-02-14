#[derive(Clone)]
pub struct AlertMessage {
    pub alert_id: AlertId,
    pub text: String,

}

#[aliri_braid::braid]
#[derive(serde::Deserialize)]
pub struct AlertId;

impl AlertMessage {
    pub(crate) fn to_message(&self) -> Result<axum::extract::ws::Message, eyre::Report> { todo!() }
}
