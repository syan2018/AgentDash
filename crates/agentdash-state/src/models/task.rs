use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Task 状态枚举
/// 生命周期: Pending → Assigned → Running → AwaitingVerification → Completed/Failed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Assigned,
    Running,
    AwaitingVerification,
    Completed,
    Failed,
}

/// Task — 执行容器
///
/// 一对一绑定 Agent 进程的执行单元，在隔离环境中完成具体工作。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub story_id: Uuid,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    /// 绑定的 Agent 类型（如 "claude-code", "codex", "gemini"）
    pub agent_type: Option<String>,
    /// Agent 进程标识
    pub agent_pid: Option<String>,
    /// 工作空间路径（worktree / container）
    pub workspace_path: Option<String>,
    /// 执行产物（Artifacts），遵循 Agent Client Protocol 格式
    pub artifacts: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    pub fn new(story_id: Uuid, title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            story_id,
            title,
            description,
            status: TaskStatus::Pending,
            agent_type: None,
            agent_pid: None,
            workspace_path: None,
            artifacts: serde_json::Value::Array(vec![]),
            created_at: now,
            updated_at: now,
        }
    }
}
