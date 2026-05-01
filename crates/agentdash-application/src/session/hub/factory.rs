//! `SessionHub` 构造与依赖注入。
//!
//! 集中 `new_with_hooks_and_persistence` + `with_*` builder 链 + `set_*`
//! 运行时注入方法。AppState / local main / companion tool 构造 hub 的
//! 入口就在这里。

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::sync::Mutex;

use super::super::augmenter::SharedPromptRequestAugmenter;
use super::super::companion_wait::CompanionWaitRegistry;
use super::super::persistence::SessionPersistence;
use super::SessionHub;
use crate::context::SharedContextAuditBus;
use agentdash_spi::hooks::ExecutionHookProvider;
use agentdash_spi::{AgentConnector, Vfs};

impl SessionHub {
    pub fn new_with_hooks_and_persistence(
        default_vfs: Option<Vfs>,
        connector: Arc<dyn AgentConnector>,
        hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
        persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        Self {
            default_vfs,
            connector,
            hook_provider,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            persistence,
            vfs_service: None,
            extra_skill_dirs: Vec::new(),
            companion_wait_registry: CompanionWaitRegistry::default(),
            title_generator: None,
            terminal_callback: Arc::new(tokio::sync::RwLock::new(None)),
            prompt_augmenter: Arc::new(tokio::sync::RwLock::new(None)),
            context_audit_bus: Arc::new(tokio::sync::RwLock::new(None)),
            base_system_prompt: String::new(),
            user_preferences: Vec::new(),
            runtime_tool_provider: None,
            mcp_relay_provider: None,
        }
    }

    pub fn with_system_prompt_config(
        mut self,
        base_system_prompt: String,
        user_preferences: Vec<String>,
    ) -> Self {
        self.base_system_prompt = base_system_prompt;
        self.user_preferences = user_preferences;
        self
    }

    pub fn with_runtime_tool_provider(
        mut self,
        provider: Arc<dyn agentdash_spi::connector::RuntimeToolProvider>,
    ) -> Self {
        self.runtime_tool_provider = Some(provider);
        self
    }

    pub fn with_mcp_relay_provider(
        mut self,
        provider: Arc<dyn agentdash_spi::McpRelayProvider>,
    ) -> Self {
        self.mcp_relay_provider = Some(provider);
        self
    }

    /// 注入 VFS 访问服务（用于 skill 扫描等需要跨 mount 读取的场景）
    pub fn with_vfs_service(mut self, service: Arc<crate::vfs::RelayVfsService>) -> Self {
        self.vfs_service = Some(service);
        self
    }

    /// 注入插件提供的额外 Skill 扫描目录
    pub fn with_extra_skill_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.extra_skill_dirs = dirs;
        self
    }

    /// 注入会话标题自动生成器（可选；未注入时不触发自动标题生成）
    pub fn with_title_generator(
        mut self,
        generator: Arc<dyn super::super::title_generator::SessionTitleGenerator>,
    ) -> Self {
        self.title_generator = Some(generator);
        self
    }

    /// 注入 session 终态全局回调（如 LifecycleOrchestrator）。
    ///
    /// 可在 SessionHub 构造完成后调用（支持延迟注入解决循环依赖）。
    /// 由于 `terminal_callback` 是共享状态（`Arc<RwLock<...>>`），
    /// 调用后所有已 clone 的 hub 实例都会生效。
    pub async fn set_terminal_callback(
        &self,
        callback: super::super::post_turn_handler::DynSessionTerminalCallback,
    ) {
        *self.terminal_callback.write().await = Some(callback);
    }

    /// 注入 Prompt 请求增强器（owner / MCP / flow capabilities / system context 等）。
    ///
    /// **何时必须注入**：只要 SessionHub 会在内部构造 `PromptSessionRequest`（如
    /// hook auto-resume、未来可能的其他系统驱动续跑），就必须注入此增强器——否则
    /// auto-resume 的 prompt 与 HTTP 主通道漂移，Agent 会失去工作流背景并倾向复读。
    ///
    /// 延迟注入设计：用 `Arc<RwLock<...>>` 以便在 AppState 构造完成后再绑定到 hub。
    pub async fn set_prompt_augmenter(&self, augmenter: SharedPromptRequestAugmenter) {
        *self.prompt_augmenter.write().await = Some(augmenter);
    }

    /// 取出当前已注入的增强器（主要用于 hub 内部调用与测试检查）。
    pub(super) async fn current_prompt_augmenter(&self) -> Option<SharedPromptRequestAugmenter> {
        self.prompt_augmenter.read().await.clone()
    }

    /// 注入 Context Audit 总线，使 Hub 创建的 runtime delegate 能发出 hook fragment 审计。
    pub async fn set_context_audit_bus(&self, bus: SharedContextAuditBus) {
        *self.context_audit_bus.write().await = Some(bus);
    }

    pub(crate) async fn current_context_audit_bus(&self) -> Option<SharedContextAuditBus> {
        self.context_audit_bus.read().await.clone()
    }
}

