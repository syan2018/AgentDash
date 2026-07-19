use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use agentdash_domain::identity::{Group, User};
use agentdash_platform_spi::platform::auth as spi_auth;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    Personal,
    Enterprise,
}

impl From<spi_auth::AuthMode> for AuthMode {
    fn from(mode: spi_auth::AuthMode) -> Self {
        match mode {
            spi_auth::AuthMode::Personal => Self::Personal,
            spi_auth::AuthMode::Enterprise => Self::Enterprise,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct AuthGroup {
    pub group_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
}

impl From<spi_auth::AuthGroup> for AuthGroup {
    fn from(group: spi_auth::AuthGroup) -> Self {
        Self {
            group_id: group.group_id,
            display_name: group.display_name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CurrentUser {
    pub auth_mode: AuthMode,
    pub user_id: String,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub groups: Vec<AuthGroup>,
    pub is_admin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(default)]
    pub extra: Value,
}

impl From<spi_auth::AuthIdentity> for CurrentUser {
    fn from(identity: spi_auth::AuthIdentity) -> Self {
        Self {
            auth_mode: identity.auth_mode.into(),
            user_id: identity.user_id,
            subject: identity.subject,
            display_name: identity.display_name,
            email: identity.email,
            avatar_url: identity.avatar_url,
            groups: identity.groups.into_iter().map(Into::into).collect(),
            is_admin: identity.is_admin,
            provider: identity.provider,
            extra: identity.extra,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LoginCredentials {
    pub username: String,
    pub password: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub extra: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoginMode {
    Form,
    Redirect,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LoginFieldDescriptor {
    pub name: String,
    pub label: String,
    pub field_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub placeholder: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LoginMetadata {
    pub provider_type: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub fields: Vec<LoginFieldDescriptor>,
    pub login_mode: LoginMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_url: Option<String>,
    pub requires_login: bool,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct AuthStartRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub return_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AuthStartResponse {
    pub auth_url: String,
    pub state: String,
    #[ts(type = "number")]
    pub expires_at_epoch_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LoginResponse {
    pub access_token: String,
    pub identity: CurrentUser,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DirectoryUser {
    pub user_id: String,
    pub subject: String,
    pub auth_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub avatar_url: Option<String>,
    pub is_admin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<User> for DirectoryUser {
    fn from(user: User) -> Self {
        Self {
            user_id: user.user_id,
            subject: user.subject,
            auth_mode: user.auth_mode,
            display_name: user.display_name,
            email: user.email,
            avatar_url: user.avatar_url,
            is_admin: user.is_admin,
            provider: user.provider,
            source: Some("projection".to_string()),
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DirectoryGroup {
    pub group_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Group> for DirectoryGroup {
    fn from(group: Group) -> Self {
        Self {
            group_id: group.group_id,
            display_name: group.display_name,
            path: None,
            provider: None,
            source: Some("projection".to_string()),
            created_at: group.created_at,
            updated_at: group.updated_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct DirectoryResolveRequest {
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DirectoryTreeNode {
    pub group_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
    pub has_children: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub children: Option<Vec<DirectoryTreeNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorySearchResponse<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub source: Option<String>,
    pub is_projection_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryResolveResponse<T> {
    pub item: T,
    pub source: Option<String>,
    pub is_projection_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DirectoryUserSearchResponse {
    pub items: Vec<DirectoryUser>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    pub is_projection_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DirectoryGroupSearchResponse {
    pub items: Vec<DirectoryGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    pub is_projection_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DirectoryTreeResponse {
    pub items: Vec<DirectoryTreeNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    pub is_projection_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DirectoryUserResolveResponse {
    pub item: DirectoryUser,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    pub is_projection_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DirectoryGroupResolveResponse {
    pub item: DirectoryGroup,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    pub is_projection_only: bool,
}
