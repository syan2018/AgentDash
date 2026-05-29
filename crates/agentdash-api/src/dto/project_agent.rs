use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OpenSessionQuery {
    #[serde(default)]
    pub force_new: bool,
}
