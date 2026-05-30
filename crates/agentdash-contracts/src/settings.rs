use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SettingsScopeKind {
    System,
    User,
    Project,
}

impl From<agentdash_domain::settings::SettingScopeKind> for SettingsScopeKind {
    fn from(value: agentdash_domain::settings::SettingScopeKind) -> Self {
        match value {
            agentdash_domain::settings::SettingScopeKind::System => Self::System,
            agentdash_domain::settings::SettingScopeKind::User => Self::User,
            agentdash_domain::settings::SettingScopeKind::Project => Self::Project,
        }
    }
}

impl From<SettingsScopeKind> for agentdash_domain::settings::SettingScopeKind {
    fn from(value: SettingsScopeKind) -> Self {
        match value {
            SettingsScopeKind::System => Self::System,
            SettingsScopeKind::User => Self::User,
            SettingsScopeKind::Project => Self::Project,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SettingsScopeQuery {
    #[serde(default)]
    #[ts(optional)]
    pub category: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub scope: Option<SettingsScopeKind>,
    #[serde(default)]
    #[ts(optional)]
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SettingResponse {
    pub scope_kind: SettingsScopeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope_id: Option<String>,
    pub key: String,
    pub value: Value,
    pub updated_at: String,
    pub masked: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct UpdateSettingsRequest {
    pub settings: Vec<SettingUpdate>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SettingUpdate {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateSettingsResponse {
    pub scope_kind: SettingsScopeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope_id: Option<String>,
    pub updated: Vec<String>,
}
