use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::context_source::ContextSourceRef;

/// Task 状态枚举
/// 生命周期: Pending → Assigned → Running → AwaitingVerification → Completed/Failed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Assigned,
    Running,
    AwaitingVerification,
    Completed,
    Failed,
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
            other => Err(format!("Unknown task status: {other}")),
        }
    }
}

/// Task 执行模式 — 控制失败后的自动处理策略
///
/// 不同模式决定了 Turn Monitor 和 State Reconciler 在 Task 失败时的行为。
/// 参考 Actant LaunchMode 策略模式，适配 AgentDashboard 的 Story → Task 场景。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionMode {
    /// 失败后标记 Failed，等待人工介入（默认行为）
    #[default]
    Standard,
    /// 失败后自动重试（配合 RestartTracker 指数退避策略）
    AutoRetry,
    /// 一次性执行：完成或失败后自动清理 session 绑定
    OneShot,
}

/// 结构化 Agent 绑定信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentBinding {
    /// Agent 类型（如 "claude-code", "codex", "gemini"）
    pub agent_type: Option<String>,
    /// Agent 进程标识
    pub agent_pid: Option<String>,
    /// 使用的预设名称（对应 ProjectConfig.agent_presets）
    pub preset_name: Option<String>,
    /// 提示词模板（支持占位符渲染）
    pub prompt_template: Option<String>,
    /// 初始上下文（拼接在提示词前）
    pub initial_context: Option<String>,
    /// 声明式 Task 特定上下文来源
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
