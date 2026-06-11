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
    pub avatar_url: Option<String>,
    pub is_admin: bool,
    pub provider: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UserProfile {
    pub user_id: String,
    pub subject: String,
    pub auth_mode: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub is_admin: bool,
    pub provider: Option<String>,
}

impl User {
    pub fn new(profile: UserProfile) -> Self {
        let now = Utc::now();
        Self {
            user_id: profile.user_id,
            subject: profile.subject,
            auth_mode: profile.auth_mode,
            display_name: profile.display_name,
            email: profile.email,
            avatar_url: profile.avatar_url,
            is_admin: profile.is_admin,
            provider: profile.provider,
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
