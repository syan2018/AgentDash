//! Session construction provider 契约。
//!
//! Session 的主通道（用户 HTTP prompt）和 auto-resume 通道都必须通过同一份
//! construction 逻辑才能拿到 owner context / MCP server 绑定 / flow capabilities /
//! context bundle 等运行时字段，否则会出现"通道漂移"——auto-resume 拿到
//! 的是一个未补齐 owner 的 prompt，Agent 丢失工作流背景后容易复读。
//!
//! API 层实现此 trait，在 AppState 初始化时通过 `SessionHub::set_session_construction_provider`
//! 注入。SessionHub 内部 follow-up 一律先经过 construction provider，与 HTTP 主通道对齐。

use std::sync::Arc;

use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, WorkflowDefinition,
};
use agentdash_spi::ConnectorError;
use async_trait::async_trait;

use super::construction::SessionConstructionFacts;
use super::launch::LaunchCommand;
use super::types::UserPromptInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLaunchPhase {
    Start,
    Continue,
}

#[derive(Debug, Clone, Default)]
pub struct TaskLaunchSource {
    pub phase: Option<TaskLaunchPhase>,
    pub override_prompt: Option<String>,
    pub additional_prompt: Option<String>,
}

#[derive(Clone)]
pub struct CompanionLaunchWorkflowSource {
    pub run: LifecycleRun,
    pub lifecycle: LifecycleDefinition,
    pub step: LifecycleStepDefinition,
    pub workflow: Option<WorkflowDefinition>,
}

#[derive(Clone)]
pub struct CompanionLaunchSource {
    pub parent_session_id: String,
    pub slice_mode: agentdash_spi::CompanionSliceMode,
    pub companion_executor_config: agentdash_spi::AgentConfig,
    pub dispatch_prompt: String,
    pub workflow: Option<CompanionLaunchWorkflowSource>,
}

/// 用于把原始 prompt 输入构建成与主通道一致的 construction facts。
#[async_trait]
pub trait SessionConstructionProvider: Send + Sync {
    /// 依据 session 的 owner binding / workspace / agent preset / workflow 等信息，
    /// 补齐后端注入字段（mcp_servers / vfs / capability_state / context_bundle 等）。
    async fn build_construction(
        &self,
        session_id: &str,
        command: &LaunchCommand,
    ) -> Result<(UserPromptInput, SessionConstructionFacts), ConnectorError>;
}

/// 动态类型别名，便于在 hub 内存储。
pub type SharedSessionConstructionProvider = Arc<dyn SessionConstructionProvider>;
