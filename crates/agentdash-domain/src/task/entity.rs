use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::value_objects::{AgentBinding, Artifact, TaskExecutionMode, TaskStatus};

/// Task — 用户工作项与 Session 策略壳
///
/// 面向用户展示的工作项容器，承载归属关系、独立业务状态机、Session 默认执行策略和结果摘要。
/// 真实执行在 Session 中发生；Task 通过 workspace_id 外键关联逻辑工作空间。
///
/// Session 归属关系通过 `SessionBinding` 管理（owner_type=task, label="execution"），
/// Task entity 不再持有 session_id。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub project_id: Uuid,
    pub story_id: Uuid,
    /// 关联的 Workspace（外键，替代原 workspace_path 字符串）
    pub workspace_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
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
    pub fn new(project_id: Uuid, story_id: Uuid, title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            story_id,
            workspace_id: None,
            title,
            description,
            status: TaskStatus::Pending,
            executor_session_id: None,
            execution_mode: TaskExecutionMode::default(),
            agent_binding: AgentBinding::default(),
            artifacts: vec![],
            created_at: now,
            updated_at: now,
        }
    }
}
