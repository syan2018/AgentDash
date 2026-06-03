use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantScopeDto {
    Turn,
    AgentFrame,
    Activity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantStatusDto {
    Created,
    PendingPolicy,
    PendingUserApproval,
    Approved,
    Rejected,
    Applied,
    Failed,
    Expired,
    Revoked,
    ScopeEscalated,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct ListPermissionGrantsQuery {
    #[serde(default)]
    #[ts(optional)]
    pub effect_frame_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub run_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub status: Option<PermissionGrantStatusDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PermissionGrantResponse {
    pub id: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effect_frame_id: Option<String>,
    pub source_runtime_session_id: String,
    pub requested_paths: Vec<String>,
    pub reason: String,
    pub grant_scope: PermissionGrantScopeDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope_escalation_intent: Option<Value>,
    pub status: PermissionGrantStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub policy_decision: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub approved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
