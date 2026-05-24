use serde::{Deserialize, Serialize};

use super::AgentInfoRelay;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDiscoverOptionsPayload {
    pub executor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseDiscoverPayload {
    pub executors: Vec<AgentInfoRelay>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverOptionsPatchPayload {
    pub request_id: String,
    pub patch: serde_json::Value,
    #[serde(default)]
    pub done: bool,
}
