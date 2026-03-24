use chrono::{DateTime, Utc};
use serde::Serialize;

use agentdash_domain::identity::{Group, User};

#[derive(Debug, Serialize)]
pub struct DirectoryUserResponse {
    pub user_id: String,
    pub subject: String,
    pub auth_mode: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_admin: bool,
    pub provider: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<User> for DirectoryUserResponse {
    fn from(user: User) -> Self {
        Self {
            user_id: user.user_id,
            subject: user.subject,
            auth_mode: user.auth_mode,
            display_name: user.display_name,
            email: user.email,
            is_admin: user.is_admin,
            provider: user.provider,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DirectoryGroupResponse {
    pub group_id: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Group> for DirectoryGroupResponse {
    fn from(group: Group) -> Self {
        Self {
            group_id: group.group_id,
            display_name: group.display_name,
            created_at: group.created_at,
            updated_at: group.updated_at,
        }
    }
}
