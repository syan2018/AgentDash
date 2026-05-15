//! `SessionHub` 构造与依赖注入。
//!
//! 集中 `new_with_hooks_and_persistence` + `with_*` builder 链 + `set_*`
//! 运行时注入方法。AppState / local main / companion tool 构造 hub 的
//! 入口就在这里。

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::sync::Mutex;

use super::super::companion_wait::CompanionWaitRegistry;
use super::super::construction_provider::SharedSessionConstructionProvider;
use super::super::persistence::{SessionPersistence, SessionStoreSet};
use super::super::runtime_registry::SessionRuntimeRegistry;
use super::super::turn_supervisor::TurnSupervisor;
use super::SessionHub;
use crate::context::SharedContextAuditBus;
use agentdash_spi::hooks::ExecutionHookProvider;
use agentdash_spi::{AgentConnector, Vfs};

impl SessionHub {
    pub fn core_service(&self) -> super::super::core::SessionCoreService {
        super::super::core::SessionCoreService::new(
            self.stores.clone(),
            self.runtime_registry.clone(),
            self.connector.clone(),
        )
    }

    pub fn eventing_service(&self) -> super::super::eventing::SessionEventingService {
        super::super::eventing::SessionEventingService::new(
            self.stores.clone(),
            self.runtime_registry.clone(),
            self.connector.clone(),
        )
    }

    pub fn runtime_service(&self) -> super::super::runtime_control::SessionRuntimeService {
        super::super::runtime_control::SessionRuntimeService::new(
            self.stores.clone(),
            self.turn_supervisor.clone(),
            self.eventing_service(),
            self.connector.clone(),
        )
    }

    pub fn control_service(&self) -> super::super::control::SessionControlService {
        super::super::control::SessionControlService::new(
            self.stores.clone(),
            self.eventing_service(),
            self.companion_wait_registry.clone(),
            self.connector.clone(),
        )
    }

    pub fn launch_service(&self) -> super::super::launch_service::SessionLaunchService {
        super::super::launch_service::SessionLaunchService::new(self.clone())
    }

    pub fn hook_service(&self) -> super::super::hooks_service::SessionHookService {
        super::super::hooks_service::SessionHookService::new(self.clone())
    }

    pub fn effects_service(&self) -> super::super::effects_service::SessionEffectsService {
        super::super::effects_service::SessionEffectsService::new(self.clone())
    }

    pub fn title_service(&self) -> super::super::title_service::SessionTitleService {
        super::super::title_service::SessionTitleService::new(self.clone())
    }

    pub fn capability_service(&self) -> super::super::capability_service::SessionCapabilityService {
        super::super::capability_service::SessionCapabilityService::new(self.clone())
    }

    pub fn new_with_hooks_and_persistence(
        default_vfs: Option<Vfs>,
        connector: Arc<dyn AgentConnector>,
        hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
        persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let runtime_registry = SessionRuntimeRegistry::new(sessions.clone());
        let turn_supervisor = TurnSupervisor::new(runtime_registry.clone());
        let stores = SessionStoreSet::from_persistence(persistence.clone());
        Self {
            default_vfs,
            connector,
            hook_provider,
            runtime_registry,
            turn_supervisor,
            stores,
            persistence,
            vfs_service: None,
            extra_skill_dirs: Vec::new(),
            companion_wait_registry: CompanionWaitRegistry::default(),
            title_generator: None,
            terminal_callback: Arc::new(tokio::sync::RwLock::new(None)),
            hook_effect_handler_registry: Arc::new(tokio::sync::RwLock::new(None)),
            session_construction_provider: Arc::new(tokio::sync::RwLock::new(None)),
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

    pub async fn set_hook_effect_handler_registry(
        &self,
        registry: super::super::post_turn_handler::DynTerminalHookEffectHandlerRegistry,
    ) {
        *self.hook_effect_handler_registry.write().await = Some(registry);
    }

    /// 注入 session construction provider（owner / MCP / flow capabilities / system context 等）。
    ///
    /// **何时必须注入**：只要 SessionHub 会在内部发起 strict launch（如
    /// hook auto-resume、未来可能的其他系统驱动续跑），就必须注入此 construction provider——否则
    /// auto-resume 的 prompt 与 HTTP 主通道漂移，Agent 会失去工作流背景并倾向复读。
    ///
    /// 延迟注入设计：用 `Arc<RwLock<...>>` 以便在 AppState 构造完成后再绑定到 hub。
    pub async fn set_session_construction_provider(
        &self,
        provider: SharedSessionConstructionProvider,
    ) {
        *self.session_construction_provider.write().await = Some(provider);
    }

    /// 注入 Context Audit 总线，使 Hub 创建的 runtime delegate 能发出 hook fragment 审计。
    pub async fn set_context_audit_bus(&self, bus: SharedContextAuditBus) {
        *self.context_audit_bus.write().await = Some(bus);
    }

    pub(crate) async fn current_context_audit_bus(&self) -> Option<SharedContextAuditBus> {
        self.context_audit_bus.read().await.clone()
    }

    /// 云端 AppState 返回前的 ready gate。
    ///
    /// 这里不负责补依赖，只验证构造阶段已经完成所有 session 主链路需要的绑定，
    /// 避免把“稍后注入”的空值暴露给正式运行态。
    pub async fn assert_ready_for_app_state(&self) -> Result<(), String> {
        if self.runtime_tool_provider.is_none() {
            return Err("SessionHub 缺少 runtime_tool_provider".to_string());
        }
        if self.mcp_relay_provider.is_none() {
            return Err("SessionHub 缺少 mcp_relay_provider".to_string());
        }
        if self.terminal_callback.read().await.is_none() {
            return Err("SessionHub 缺少 terminal_callback".to_string());
        }
        if self.hook_effect_handler_registry.read().await.is_none() {
            return Err("SessionHub 缺少 hook_effect_handler_registry".to_string());
        }
        if self.session_construction_provider.read().await.is_none() {
            return Err("SessionHub 缺少 session_construction_provider".to_string());
        }
        if self.context_audit_bus.read().await.is_none() {
            return Err("SessionHub 缺少 context_audit_bus".to_string());
        }
        Ok(())
    }
}
