//! `SessionRuntimeInner` 装配对象与尚待下沉的 session 内部实现。
//!
//! 按能力服务拆分后的剩余范围：
//! - [`facade`]：测试入口与少量 session 内部 helper。
//! - [`factory`]：构造与注入（`new_with_hooks_and_stores` + `with_*` / `set_*`）。
//! - [`tool_builder`]：runtime tool + 直连/relay MCP 工具发现 + 已持久化 AgentFrame adoption。
//! - [`hook_dispatch`]：`emit_session_hook_trigger` / `ensure_hook_runtime` /
//!   `collect_runtime_context_update_injections` / `schedule_unanchored_hook_auto_resume`。
//! - [`runtime_context_transition`]：AgentFrame adoption 通知、pending 入队与 next-turn 应用。
//!
//! 本模块最终只保留装配与 ready gate，tool / hook / transition / launch /
//! effects 的内部实现应持续下沉到具体服务或明确依赖包。

use std::{path::PathBuf, sync::Arc};

use super::persistence::SessionStoreSet;
use super::runtime_registry::SessionRuntimeRegistry;
use super::turn_supervisor::TurnSupervisor;
use crate::context::SharedContextAuditBus;
use agentdash_application_ports::agent_run_surface::{
    AgentRunEffectiveCapabilityPort, AgentRunRuntimeSurfaceQueryPort,
};
use agentdash_application_ports::frame_launch_envelope::{
    AcceptedLaunchCommitPort, SharedFrameLaunchEnvelopePort,
};
use agentdash_application_ports::runtime_session_live::{
    RuntimeSessionEffectiveCapabilityPort, RuntimeSessionHookTargetPort,
    RuntimeSessionMailboxRuntimePort,
};
use agentdash_domain::permission::PermissionGrantRepository;
use agentdash_domain::settings::SettingsRepository;
use agentdash_domain::workflow::{AgentFrameRepository, RuntimeSessionExecutionAnchorRepository};
use agentdash_spi::AgentConnector;
use agentdash_spi::hooks::ExecutionHookProvider;

mod facade;
mod factory;
mod hook_dispatch;
mod runtime_context_transition;
mod tool_builder;

pub(crate) use hook_dispatch::{HookTriggerDispatchResult, HookTriggerInput};
pub(crate) use runtime_context_transition::{
    ApplyPendingRuntimeContextTransitionInput, LiveRuntimeContextTransitionInput,
    PendingRuntimeContextApplication, build_initial_capability_state_frame,
};

#[derive(Clone)]
pub struct SessionRuntimeInner {
    pub(super) connector: Arc<dyn AgentConnector>,
    pub(super) hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
    pub(super) runtime_registry: SessionRuntimeRegistry,
    pub(super) turn_supervisor: TurnSupervisor,
    pub(super) stores: SessionStoreSet,
    pub(crate) vfs_service: Option<Arc<dyn Send + Sync>>,
    pub(super) extra_skill_dirs: Vec<PathBuf>,
    pub(super) skill_discovery_providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
    pub(super) terminal_callback:
        Arc<tokio::sync::RwLock<Option<super::post_turn_handler::DynSessionTerminalCallback>>>,
    pub(super) hook_effect_handler_registry: Arc<
        tokio::sync::RwLock<Option<super::post_turn_handler::DynTerminalHookEffectHandlerRegistry>>,
    >,
    /// 将来源 command 构建成与 HTTP 主通道一致的 `FrameLaunchEnvelope`。
    /// Hub 内部的 auto-resume 等场景必须经它补齐 frame/MCP/flow 上下文，
    /// 避免与主通道漂移。用 `Arc<RwLock<...>>` 以便延迟注入（循环依赖场景）。
    pub(super) frame_launch_envelope_provider:
        Arc<tokio::sync::RwLock<Option<SharedFrameLaunchEnvelopePort>>>,
    pub(super) accepted_launch_commit_port:
        Arc<tokio::sync::RwLock<Option<Arc<dyn AcceptedLaunchCommitPort>>>>,
    /// Context Inspector 使用的审计总线。Hub 内部创建 runtime delegate 时需要把它
    /// 传给 hook 链路，记录每轮 HookInjection → ContextFragment 的动态片段。
    pub(super) context_audit_bus: Arc<tokio::sync::RwLock<Option<SharedContextAuditBus>>>,
    /// Layer 0 base system prompt（由 factory 从 settings / 常量注入）。
    pub(super) base_system_prompt: String,
    /// 用户偏好设置仓储。每轮按当前 AuthIdentity 读取 user scope。
    pub(super) settings_repo: Option<Arc<dyn SettingsRepository>>,
    /// 运行时工具构建 provider（由 factory 注入，pipeline 在 prompt 前调用）。
    pub(super) runtime_tool_provider:
        Option<Arc<dyn agentdash_spi::connector::RuntimeToolProvider>>,
    /// MCP 工具发现端口（由 factory 注入，pipeline 在 prompt 前调用）。
    pub(super) mcp_tool_discovery:
        Option<Arc<dyn agentdash_application_ports::mcp_discovery::McpToolDiscovery>>,
    /// Relay backend execution placement dependencies.
    pub(super) backend_execution_transport:
        Option<Arc<dyn agentdash_application_ports::backend_transport::RelayPromptTransport>>,
    pub(super) backend_execution_lease_repo:
        Option<Arc<dyn agentdash_domain::backend::BackendExecutionLeaseRepository>>,
    /// AgentFrame revision 持久化仓储。
    /// 当 capability state 变更时通过 AgentFrameBuilder 写入新 revision，
    /// 使 AgentFrame 成为 capability surface 的唯一权威事实源。
    pub(super) agent_frame_repo: Option<Arc<dyn AgentFrameRepository>>,
    pub(super) execution_anchor_repo: Option<Arc<dyn RuntimeSessionExecutionAnchorRepository>>,
    pub(super) runtime_surface_query: Option<Arc<dyn AgentRunRuntimeSurfaceQueryPort>>,
    pub(super) agent_run_effective_capability_port:
        Option<Arc<dyn AgentRunEffectiveCapabilityPort>>,
    /// LifecycleAgent 仓储 — launch path 需要查询 agent bootstrap 状态。
    pub(super) lifecycle_agent_repo:
        Option<Arc<dyn agentdash_domain::workflow::LifecycleAgentRepository>>,
    pub(super) permission_grant_repo: Option<Arc<dyn PermissionGrantRepository>>,
    pub(super) effective_capability_port: Option<Arc<dyn RuntimeSessionEffectiveCapabilityPort>>,
    pub(super) hook_target_port: Option<Arc<dyn RuntimeSessionHookTargetPort>>,
    pub(super) mailbox_runtime_port:
        Arc<tokio::sync::RwLock<Option<Arc<dyn RuntimeSessionMailboxRuntimePort>>>>,
    /// LifecycleGate 仓储，用于 companion_wait durable 等待。
    pub(super) lifecycle_gate_repo:
        Option<Arc<dyn agentdash_domain::workflow::LifecycleGateRepository>>,
}
