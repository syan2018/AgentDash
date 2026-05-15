//! Prompt augment 契约。
//!
//! Session 的主通道（用户 HTTP prompt）和 auto-resume 通道都必须通过同一份
//! 增强逻辑才能拿到 owner context / MCP server 绑定 / flow capabilities /
//! context bundle 等运行时字段，否则会出现"通道漂移"——auto-resume 拿到
//! 的是一个未补齐 owner 的 prompt，Agent 丢失工作流背景后容易复读。
//!
//! API 层实现此 trait，在 AppState 初始化时通过 `SessionHub::set_prompt_augmenter`
//! 注入。SessionHub 内部 follow-up 一律先经过 augmenter，与 HTTP 主通道对齐。

use std::sync::Arc;

use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleRun, LifecycleStepDefinition, WorkflowDefinition,
};
use agentdash_spi::ConnectorError;
use async_trait::async_trait;

use super::post_turn_handler::DynPostTurnHandler;
use super::types::{AugmentedLaunchInput, UserPromptInput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAugmentTaskPhase {
    Start,
    Continue,
}

#[derive(Debug, Clone, Default)]
pub struct PromptAugmentTaskInput {
    pub phase: Option<PromptAugmentTaskPhase>,
    pub override_prompt: Option<String>,
    pub additional_prompt: Option<String>,
}

#[derive(Clone)]
pub struct PromptAugmentCompanionWorkflowInput {
    pub run: LifecycleRun,
    pub lifecycle: LifecycleDefinition,
    pub step: LifecycleStepDefinition,
    pub workflow: Option<WorkflowDefinition>,
}

#[derive(Clone)]
pub struct PromptAugmentCompanionInput {
    pub parent_vfs: Option<agentdash_spi::Vfs>,
    pub parent_mcp_servers: Vec<agentdash_spi::SessionMcpServer>,
    pub parent_context_bundle: Option<agentdash_spi::SessionContextBundle>,
    pub slice_mode: agentdash_spi::CompanionSliceMode,
    pub companion_executor_config: agentdash_spi::AgentConfig,
    pub dispatch_prompt: String,
    pub workflow: Option<PromptAugmentCompanionWorkflowInput>,
}

/// 需要进入 owner/context/capability augment 的原始 prompt 输入。
///
/// 它是 `LaunchCommand` 与 API 层 augmenter 之间的窄协议；不承载 composition
/// 产物，也不表达 launch source。输出是当前 prompt pipeline 消费的
/// `AugmentedLaunchInput`；该输出不能被解释为 launch plan。
pub struct PromptAugmentInput {
    pub user_input: UserPromptInput,
    pub request_mcp_servers: Vec<agentdash_spi::SessionMcpServer>,
    pub existing_vfs: Option<agentdash_spi::Vfs>,
    pub identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
    pub task: Option<PromptAugmentTaskInput>,
    pub companion: Option<PromptAugmentCompanionInput>,
}

impl PromptAugmentInput {
    pub fn into_augmented_launch_input(self) -> AugmentedLaunchInput {
        let mut prompt = AugmentedLaunchInput::from_user_input(self.user_input);
        prompt.mcp_servers = self.request_mcp_servers;
        prompt.vfs = self.existing_vfs;
        prompt.identity = self.identity;
        prompt.post_turn_handler = self.post_turn_handler;
        prompt
    }
}

/// 用于把原始 prompt 输入增强成与主通道一致的 augmented launch input。
#[async_trait]
pub trait PromptRequestAugmenter: Send + Sync {
    /// 依据 session 的 owner binding / workspace / agent preset / workflow 等信息，
    /// 补齐后端注入字段（mcp_servers / vfs / capability_state / context_bundle /
    /// hook_snapshot_reload 等）。
    async fn augment(
        &self,
        session_id: &str,
        input: PromptAugmentInput,
    ) -> Result<AugmentedLaunchInput, ConnectorError>;
}

/// 动态类型别名，便于在 hub 内存储。
pub type SharedPromptRequestAugmenter = Arc<dyn PromptRequestAugmenter>;
