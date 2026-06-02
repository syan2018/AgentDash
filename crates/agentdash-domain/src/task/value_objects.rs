use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::context_source::ContextSourceRef;
use crate::workflow::ActivityAttemptStatus;

/// Task 状态枚举
/// 生命周期: Pending → Assigned → Running → AwaitingVerification → Completed/Failed/Cancelled
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Assigned,
    Running,
    AwaitingVerification,
    Completed,
    Failed,
    Cancelled,
}

impl std::str::FromStr for TaskStatus {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim() {
            "pending" => Ok(Self::Pending),
            "assigned" => Ok(Self::Assigned),
            "running" => Ok(Self::Running),
            "awaiting_verification" => Ok(Self::AwaitingVerification),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("Unknown task status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskExecutionProjection {
    pub status: TaskStatus,
}

impl TaskExecutionProjection {
    /// 将 workflow attempt 状态翻译成 Task 自己的 execution projection。
    ///
    /// `Completed` 会先进入 `AwaitingVerification`，因为 Task 的业务完成态仍由
    /// hook / verification 决策；`Cancelled` 独立于 `Failed`，表示执行被停止。
    pub fn from_attempt_status(attempt_status: ActivityAttemptStatus) -> Self {
        let status = match attempt_status {
            ActivityAttemptStatus::Pending => TaskStatus::Pending,
            ActivityAttemptStatus::Ready | ActivityAttemptStatus::Claiming => TaskStatus::Assigned,
            ActivityAttemptStatus::Running => TaskStatus::Running,
            ActivityAttemptStatus::Completed => TaskStatus::AwaitingVerification,
            ActivityAttemptStatus::Failed => TaskStatus::Failed,
            ActivityAttemptStatus::Cancelled => TaskStatus::Cancelled,
        };
        Self { status }
    }
}

/// Task authoring preference for agent execution — a **static declaration**, not runtime truth.
///
/// Captures the user's intent for which agent type, preset, prompt template,
/// and initial context to use when executing a Task.
///
/// At dispatch time, the resolver consumes these preferences to build `AgentConfig`
/// for `SubjectExecutionIntent`. Once dispatch completes, runtime truth lives in
/// `LifecycleAgent → AgentFrame`.
///
/// **Boundary**: belongs to the Task *spec* layer (user-editable fields), not runtime state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskDispatchPreference {
    /// Preferred agent type identifier (e.g. "claude-code", "codex", "gemini").
    /// Consumed by `resolve_task_agent_config` at dispatch time.
    pub agent_type: Option<String>,
    /// Agent process identifier (informational / external tracking).
    pub agent_pid: Option<String>,
    /// Named preset reference (resolved against `ProjectConfig.agent_presets`).
    /// When set, the preset's `agent_type` and extended config override the bare `agent_type` field.
    pub preset_name: Option<String>,
    /// Prompt template with placeholder support (rendered before agent launch).
    pub prompt_template: Option<String>,
    /// Initial context prepended to the prompt.
    pub initial_context: Option<String>,
    /// Declarative context source references specific to this Task.
    #[serde(default)]
    pub context_sources: Vec<ContextSourceRef>,
}

/// 执行产物
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub artifact_type: ArtifactType,
    pub content: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// 产物类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    /// 代码变更
    CodeChange,
    /// 测试结果
    TestResult,
    /// 日志输出
    LogOutput,
    /// 生成文件
    File,
    /// 工具执行过程（tool_call/tool_call_update）
    ToolExecution,
}
