use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{ProjectConfig, ProjectVisibility};

/// Project — 项目容器
///
/// 组织 Story 和 Workspace 的顶层业务单元。
/// 管理 Agent 预设配置，提供默认 Workspace 绑定。
/// backend_id 已移除，通过 config.default_workspace_id → Workspace.backend_id 获取。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    /// 项目级配置（Agent 预设、默认 Workspace 等）
    pub config: ProjectConfig,
    pub created_by_user_id: String,
    pub updated_by_user_id: String,
    pub visibility: ProjectVisibility,
    pub is_template: bool,
    pub cloned_from_project_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn new(name: String, description: String) -> Self {
        Self::new_with_creator(name, description, "system".to_string())
    }

    pub fn new_with_creator(name: String, description: String, created_by_user_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description,
            config: ProjectConfig::default(),
            created_by_user_id: created_by_user_id.clone(),
            updated_by_user_id: created_by_user_id,
            visibility: ProjectVisibility::Private,
            is_template: false,
            cloned_from_project_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn touch_updated_by(&mut self, updated_by_user_id: String) {
        self.updated_by_user_id = updated_by_user_id;
        self.updated_at = Utc::now();
    }
}
