//! `SessionHub` 门面 + 职责子模块。
//!
//! 按 PR 6 拆分：
//! - [`facade`]：session CRUD / subscribe / inject / 基本 prompt routing / companion。
//! - [`factory`]：构造与注入（`new_with_hooks_and_persistence` + `with_*` / `set_*`）。
//! - [`tool_builder`]：runtime tool + 直连/relay MCP 工具发现 + `replace_current_capability_state`。
//! - [`hook_dispatch`]：`emit_session_hook_trigger` / `ensure_hook_session_runtime` /
//!   `collect_runtime_context_update_injections` / `schedule_hook_auto_resume`。
//!   （原 `session/event_bridge.rs` 已于 PR 6 迁入本模块并顺手删除 `_tx` 占位参数。）
//! - [`runtime_context_transition`]：workflow phase/runtime context transition 的 live
//!   apply、pending 入队与 next-turn 应用。
//! - [`cancel`]：`cancel` 路径与 interrupted 事件补发。
//! - [`compaction`]：`context_compacted` 事件元数据富化（填 `compacted_until_ref`）。
//!
//! 对外路径 `crate::session::hub::SessionHub` 保持不变。

use std::{path::PathBuf, sync::Arc};

use super::augmenter::SharedPromptRequestAugmenter;
use super::companion_wait::CompanionWaitRegistry;
use super::persistence::{SessionPersistence, SessionStoreSet};
use super::runtime_registry::SessionRuntimeRegistry;
use super::turn_supervisor::TurnSupervisor;
use crate::context::SharedContextAuditBus;
use agentdash_spi::hooks::ExecutionHookProvider;
use agentdash_spi::{AgentConnector, Vfs};

mod cancel;
mod compaction;
mod facade;
mod factory;
mod hook_dispatch;
mod runtime_context_transition;
mod tool_builder;

#[cfg(test)]
mod tests;

pub(super) use hook_dispatch::HookTriggerInput;
pub(crate) use runtime_context_transition::{
    LiveRuntimeContextTransitionInput, PendingRuntimeContextTransitionInput,
    RuntimeContextTransitionOutcome, build_initial_capability_state_frame,
};

#[derive(Clone)]
pub struct SessionHub {
    /// 当 `AugmentedLaunchInput.vfs` 为 None 时回退使用（如云宿主 cwd、本机首个 accessible root）。
    pub(super) default_vfs: Option<Vfs>,
    pub(super) connector: Arc<dyn AgentConnector>,
    pub(super) hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
    pub(super) runtime_registry: SessionRuntimeRegistry,
    pub(super) turn_supervisor: TurnSupervisor,
    pub(super) stores: SessionStoreSet,
    pub(super) persistence: Arc<dyn SessionPersistence>,
    pub(crate) vfs_service: Option<Arc<crate::vfs::RelayVfsService>>,
    pub(super) extra_skill_dirs: Vec<PathBuf>,
    pub companion_wait_registry: CompanionWaitRegistry,
    pub(super) title_generator: Option<Arc<dyn super::title_generator::SessionTitleGenerator>>,
    pub(super) terminal_callback:
        Arc<tokio::sync::RwLock<Option<super::post_turn_handler::DynSessionTerminalCallback>>>,
    pub(super) hook_effect_handler_registry: Arc<
        tokio::sync::RwLock<Option<super::post_turn_handler::DynTerminalHookEffectHandlerRegistry>>,
    >,
    /// 将"裸" AugmentedLaunchInput 增强成与 HTTP 主通道一致的完整请求。
    /// Hub 内部的 auto-resume 等场景必须经它补齐 owner/mcp/flow 上下文，
    /// 避免与主通道漂移。用 `Arc<RwLock<...>>` 以便延迟注入（循环依赖场景）。
    pub(super) prompt_augmenter: Arc<tokio::sync::RwLock<Option<SharedPromptRequestAugmenter>>>,
    /// Context Inspector 使用的审计总线。Hub 内部创建 runtime delegate 时需要把它
    /// 传给 hook 链路，记录每轮 HookInjection → ContextFragment 的动态片段。
    pub(super) context_audit_bus: Arc<tokio::sync::RwLock<Option<SharedContextAuditBus>>>,
    /// Layer 0 base system prompt（由 factory 从 settings / 常量注入）。
    pub(super) base_system_prompt: String,
    /// Layer 2 用户偏好提示列表（由 factory 从 settings 注入）。
    pub(super) user_preferences: Vec<String>,
    /// 运行时工具构建 provider（由 factory 注入，pipeline 在 prompt 前调用）。
    pub(super) runtime_tool_provider:
        Option<Arc<dyn agentdash_spi::connector::RuntimeToolProvider>>,
    /// MCP Relay 工具发现 provider（由 factory 注入，pipeline 在 prompt 前调用）。
    pub(super) mcp_relay_provider: Option<Arc<dyn agentdash_spi::McpRelayProvider>>,
}
