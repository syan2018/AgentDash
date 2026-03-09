use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{AgentBinding, Artifact, TaskExecutionMode, TaskStatus};

/// Task — 执行容器
///
/// 一对一绑定 Agent 进程的执行单元，在隔离 Workspace 中完成具体工作。
/// 通过 workspace_id 外键关联物理工作空间。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub story_id: Uuid,
    /// 关联的 Workspace（外键，替代原 workspace_path 字符串）
    pub workspace_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    /// 绑定的执行会话 ID（首次 start 时创建）
    pub session_id: Option<String>,
    /// 执行器原生会话 ID（用于 follow-up/resume）
    pub executor_session_id: Option<String>,
    /// 执行模式 — 控制失败后的自动处理策略
    pub execution_mode: TaskExecutionMode,
    /// 结构化 Agent 绑定信息
    pub agent_binding: AgentBinding,
    /// 结构化执行产物列表
    pub artifacts: Vec<Artifact>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    pub fn new(story_id: Uuid, title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            story_id,
            workspace_id: None,
            title,
            description,
            status: TaskStatus::Pending,
            session_id: None,
            executor_session_id: None,
            execution_mode: TaskExecutionMode::default(),
            agent_binding: AgentBinding::default(),
            artifacts: vec![],
            created_at: now,
            updated_at: now,
        }
    }
}
