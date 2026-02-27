use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{WorkspaceType, WorkspaceStatus, GitConfig};

/// Workspace — 物理工作空间
///
/// 代表一个实际的代码目录，可被多个 Task 共享。
/// 与 vibe-kanban 的 Workspace 概念对齐，container_ref 指向物理路径。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    /// 所属项目
    pub project_id: Uuid,
    /// 显示名称
    pub name: String,
    /// 物理路径（磁盘目录）
    pub container_ref: String,
    pub workspace_type: WorkspaceType,
    pub status: WorkspaceStatus,
    /// Git worktree 配置（仅 GitWorktree 类型使用）
    pub git_config: Option<GitConfig>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Workspace {
    pub fn new(project_id: Uuid, name: String, container_ref: String, workspace_type: WorkspaceType) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name,
            container_ref,
            workspace_type,
            status: WorkspaceStatus::Pending,
            git_config: None,
            created_at: now,
            updated_at: now,
        }
    }
}
