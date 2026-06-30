use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantStatusGroupDto {
    Pending,
    Active,
    Terminal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOutcomeDto {
    AutoApproved,
    NeedsUserApproval,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct PolicyDecisionDto {
    pub outcome: PolicyOutcomeDto,
    pub matched_rules: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct ScopeEscalationIntentDto {
    pub target_subject_kind: String,
    pub unlocked_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantVfsOperationDto {
    Read,
    List,
    Search,
    Write,
    Exec,
    ApplyPatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantVfsPathScopeDto {
    All,
    Prefix(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct PermissionGrantVfsAccessRuleDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub surface_ref: Option<String>,
    pub mount_id: String,
    pub path_scope: PermissionGrantVfsPathScopeDto,
    pub operations: Vec<PermissionGrantVfsOperationDto>,
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
    #[serde(default)]
    #[ts(optional)]
    pub status_group: Option<PermissionGrantStatusGroupDto>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_vfs_access: Vec<PermissionGrantVfsAccessRuleDto>,
    pub reason: String,
    pub grant_scope: PermissionGrantScopeDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope_escalation_intent: Option<ScopeEscalationIntentDto>,
    pub status: PermissionGrantStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub policy_decision: Option<PolicyDecisionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub approved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
