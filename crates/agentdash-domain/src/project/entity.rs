use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::ProjectConfig;

/// Project — 项目容器
///
/// 组织 Story 和 Workspace 的顶层业务单元。
/// 管理 Agent 预设配置，提供默认后端绑定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    /// 默认执行后端标识
    pub backend_id: String,
    /// 项目级配置（Agent 预设、默认 Workspace 等）
    pub config: ProjectConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn new(name: String, description: String, backend_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description,
            backend_id,
            config: ProjectConfig::default(),
            created_at: now,
            updated_at: now,
        }
    }
}
