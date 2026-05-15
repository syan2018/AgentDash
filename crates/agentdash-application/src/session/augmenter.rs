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

use super::construction::SourceContractPlan;
use super::launch::LaunchCommand;
use super::ownership::ResolvedSessionOwner;
use super::post_turn_handler::DynPostTurnHandler;
use super::types::{HookSnapshotReloadTrigger, UserPromptInput};

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

/// LaunchCommand 与 owner/context/capability augment 之间的唯一 payload。
///
/// 该类型从来源入口进入 augmenter，并在同一对象上补齐 construction seed 与
/// launch 局部字段。它不是 plan，也不是 session 构建事实源；进入执行前必须继续
/// 投影为 `SessionConstructionPlan` 与 `LaunchExecution`。
pub struct PromptAugmentInput {
    pub user_input: UserPromptInput,
    pub construction_owner: Option<ResolvedSessionOwner>,
    pub source_contract: SourceContractPlan,
    pub mcp_servers: Vec<agentdash_spi::SessionMcpServer>,
    pub vfs: Option<agentdash_spi::Vfs>,
    pub capability_state: Option<agentdash_spi::CapabilityState>,
    /// 结构化上下文 Bundle —— 所有 connector 的主数据源。
    pub context_bundle: Option<agentdash_spi::SessionContextBundle>,
    /// continuation 场景下的独立上下文 frame（不再退化为 bundle markdown 字符串）。
    pub continuation_context_frame: Option<agentdash_spi::hooks::ContextFrame>,
    /// 本轮 prompt 是否需要重载 hook snapshot + 触发 `SessionStart` hook。
    pub hook_snapshot_reload: HookSnapshotReloadTrigger,
    pub identity: Option<agentdash_spi::platform::auth::AuthIdentity>,
    pub post_turn_handler: Option<DynPostTurnHandler>,
    pub task: Option<PromptAugmentTaskInput>,
    pub companion: Option<PromptAugmentCompanionInput>,
}

impl PromptAugmentInput {
    pub fn from_user_input(input: UserPromptInput) -> Self {
        Self {
            user_input: input,
            construction_owner: None,
            source_contract: SourceContractPlan::default(),
            mcp_servers: Vec::new(),
            vfs: None,
            capability_state: None,
            context_bundle: None,
            continuation_context_frame: None,
            hook_snapshot_reload: HookSnapshotReloadTrigger::None,
            identity: None,
            post_turn_handler: None,
            task: None,
            companion: None,
        }
    }
}

/// 用于把原始 prompt 输入增强成与主通道一致的 launch payload。
#[async_trait]
pub trait PromptRequestAugmenter: Send + Sync {
    /// 依据 session 的 owner binding / workspace / agent preset / workflow 等信息，
    /// 补齐后端注入字段（mcp_servers / vfs / capability_state / context_bundle /
    /// hook_snapshot_reload 等）。
    async fn augment(
        &self,
        session_id: &str,
        command: &LaunchCommand,
    ) -> Result<PromptAugmentInput, ConnectorError>;
}

/// 动态类型别名，便于在 hub 内存储。
pub type SharedPromptRequestAugmenter = Arc<dyn PromptRequestAugmenter>;
