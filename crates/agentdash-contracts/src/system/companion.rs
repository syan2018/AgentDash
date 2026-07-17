use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CompanionGateRespondRequest {
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct CompanionGateRespondResponse {
    pub responded: bool,
    pub gate_id: String,
    pub request_id: String,
    pub gate_resolved: bool,
}
