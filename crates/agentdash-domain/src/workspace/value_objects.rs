use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// 逻辑工作空间身份类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceIdentityKind {
    GitRepo,
    P4Workspace,
    LocalDir,
}

/// 工作空间绑定状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceBindingStatus {
    Pending,
    Ready,
    Offline,
    Error,
}

/// 运行时绑定解析策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceResolutionPolicy {
    PreferDefaultBinding,
    PreferOnline,
}

/// 逻辑工作空间整体状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Pending,
    Ready,
    Active,
    Archived,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceBinding {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub backend_id: String,
    pub root_ref: String,
    pub status: WorkspaceBindingStatus,
    pub detected_facts: Value,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkspaceBinding {
    pub fn new(
        workspace_id: Uuid,
        backend_id: String,
        root_ref: String,
        detected_facts: Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            workspace_id,
            backend_id,
            root_ref,
            status: WorkspaceBindingStatus::Pending,
            detected_facts,
            last_verified_at: None,
            priority: 0,
            created_at: now,
            updated_at: now,
        }
    }
}
