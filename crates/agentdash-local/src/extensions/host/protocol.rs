use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(super) struct RunnerRequest<'a> {
    pub kind: &'static str,
    pub id: &'a str,
    pub method: &'a str,
    pub params: Value,
}

#[derive(Debug, Deserialize)]
pub(super) struct RunnerMessage {
    pub kind: String,
    pub id: Option<String>,
    pub method: Option<String>,
    pub params: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub level: Option<String>,
    pub message: Option<String>,
}
