use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

/// 结构化 Agent 绑定信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentBinding {
    /// Agent 类型（如 "claude-code", "codex", "gemini"）
    pub agent_type: Option<String>,
    /// Agent 进程标识
    pub agent_pid: Option<String>,
    /// 使用的预设名称（对应 ProjectConfig.agent_presets）
    pub preset_name: Option<String>,
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
}
