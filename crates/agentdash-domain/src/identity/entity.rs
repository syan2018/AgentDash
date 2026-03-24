use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 用户身份快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
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

impl User {
    pub fn new(
        user_id: String,
        subject: String,
        auth_mode: String,
        display_name: Option<String>,
        email: Option<String>,
        is_admin: bool,
        provider: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            user_id,
            subject,
            auth_mode,
            display_name,
            email,
            is_admin,
            provider,
            created_at: now,
            updated_at: now,
        }
    }
}

/// 用户组快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub group_id: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Group {
    pub fn new(group_id: String, display_name: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            group_id,
            display_name,
            created_at: now,
            updated_at: now,
        }
    }
}
