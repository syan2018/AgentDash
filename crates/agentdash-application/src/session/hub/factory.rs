//! `SessionRuntimeInner` 构造与依赖注入。
//!
//! 集中 `new_with_hooks_and_persistence` + `with_*` builder 链 + `set_*`
//! 运行时注入方法。AppState / local main / companion tool 构造 hub 的
//! 入口就在这里。

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::sync::Mutex;

use super::super::persistence::{SessionPersistence, SessionStoreSet};
use super::super::runtime_registry::SessionRuntimeRegistry;
use super::super::turn_supervisor::TurnSupervisor;
use super::SessionRuntimeInner;
use crate::agent_run::AgentRunMailboxRuntimeAdapter;
use crate::agent_run::frame::launch_envelope_provider::SharedFrameLaunchEnvelopeProvider;
use crate::context::SharedContextAuditBus;
use agentdash_spi::AgentConnector;
use agentdash_spi::hooks::ExecutionHookProvider;

impl SessionRuntimeInner {
    pub fn core_service(&self) -> super::super::core::SessionCoreService {
        super::super::core::SessionCoreService::new(
            self.stores.clone(),
            self.runtime_registry.clone(),
            self.connector.clone(),
        )
    }

    pub fn branching_service(&self) -> super::super::branching::SessionBranchingService {
        super::super::branching::SessionBranchingService::new(self.stores.clone())
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
        super::super::control::SessionControlService::new(self.connector.clone())
    }

    pub fn launch_service(&self) -> super::super::launch::SessionLaunchService {
        super::super::launch::SessionLaunchService::new(self.clone())
    }

    pub fn hook_service(&self) -> super::super::hooks_service::SessionHookService {
        super::super::hooks_service::SessionHookService::new(self.clone())
    }

    pub fn effects_service(&self) -> super::super::effects_service::SessionEffectsService {
        super::super::effects_service::SessionEffectsService::new(
            super::super::terminal_effects::TerminalEffectDeps {
                terminal_effects: self.stores.terminal_effects.clone(),
                hook_trigger: Arc::new(self.clone()),
                terminal_callback: self.terminal_callback.clone(),
                hook_effect_handler_registry: self.hook_effect_handler_registry.clone(),
                auto_resume: Arc::new(self.clone()),
            },
        )
    }

    pub fn title_service(&self) -> super::super::title_service::SessionTitleService {
        super::super::title_service::SessionTitleService::new(
            self.core_service(),
            self.eventing_service(),
        )
    }

    pub fn runtime_transition_service(
        &self,
    ) -> super::super::runtime_transition_service::SessionRuntimeTransitionService {
        super::super::runtime_transition_service::SessionRuntimeTransitionService::new(self.clone())
    }

    pub fn new_with_hooks_and_persistence(
        connector: Arc<dyn AgentConnector>,
        hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
        persistence: Arc<dyn SessionPersistence>,
    ) -> Self {
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let runtime_registry = SessionRuntimeRegistry::new(sessions.clone());
        let turn_supervisor = TurnSupervisor::new(runtime_registry.clone());
        let stores = SessionStoreSet::from_persistence(persistence.clone());
        Self {
            connector,
            hook_provider,
            runtime_registry,
            turn_supervisor,
            stores,
            persistence,
            vfs_service: None,
            extra_skill_dirs: Vec::new(),
            skill_discovery_providers: Vec::new(),
            terminal_callback: Arc::new(tokio::sync::RwLock::new(None)),
            hook_effect_handler_registry: Arc::new(tokio::sync::RwLock::new(None)),
            frame_launch_envelope_provider: Arc::new(tokio::sync::RwLock::new(None)),
            context_audit_bus: Arc::new(tokio::sync::RwLock::new(None)),
            base_system_prompt: String::new(),
            settings_repo: None,
            runtime_tool_provider: None,
            mcp_tool_discovery: None,
            backend_execution_transport: None,
            backend_execution_lease_repo: None,
            agent_frame_repo: None,
            execution_anchor_repo: None,
            lifecycle_agent_repo: None,
            permission_grant_repo: None,
            agent_run_mailbox_runtime_adapter: Arc::new(tokio::sync::RwLock::new(None)),
            lifecycle_gate_repo: None,
        }
    }

    pub fn with_system_prompt_config(mut self, base_system_prompt: String) -> Self {
        self.base_system_prompt = base_system_prompt;
        self
    }

    pub fn with_settings_repository(
        mut self,
        repo: Arc<dyn agentdash_domain::settings::SettingsRepository>,
    ) -> Self {
        self.settings_repo = Some(repo);
        self
    }

    pub fn with_runtime_tool_provider(
        mut self,
        provider: Arc<dyn agentdash_spi::connector::RuntimeToolProvider>,
    ) -> Self {
        self.runtime_tool_provider = Some(provider);
        self
    }

    pub fn with_mcp_tool_discovery(
        mut self,
        provider: Arc<dyn agentdash_application_ports::mcp_discovery::McpToolDiscovery>,
    ) -> Self {
        self.mcp_tool_discovery = Some(provider);
        self
    }

    pub fn with_backend_execution_placement(
        mut self,
        transport: Arc<dyn agentdash_application_ports::backend_transport::RelayPromptTransport>,
        lease_repo: Arc<dyn agentdash_domain::backend::BackendExecutionLeaseRepository>,
    ) -> Self {
        self.backend_execution_transport = Some(transport);
        self.backend_execution_lease_repo = Some(lease_repo);
        self
    }

    /// 注入 LifecycleGate 仓储（用于 companion_wait durable 等待）
    pub fn with_lifecycle_gate_repo(
        mut self,
        repo: Arc<dyn agentdash_domain::workflow::LifecycleGateRepository>,
    ) -> Self {
        self.lifecycle_gate_repo = Some(repo);
        self
    }

    /// 注入 AgentFrame 仓储（用于 capability state 变更时写入 frame revision）
    pub fn with_agent_frame_repo(
        mut self,
        repo: Arc<dyn agentdash_domain::workflow::AgentFrameRepository>,
    ) -> Self {
        self.agent_frame_repo = Some(repo);
        self
    }

    pub fn with_execution_anchor_repo(
        mut self,
        repo: Arc<dyn agentdash_domain::workflow::RuntimeSessionExecutionAnchorRepository>,
    ) -> Self {
        self.execution_anchor_repo = Some(repo);
        self
    }

    /// 注入 LifecycleAgent 仓储（launch path 需要查询 agent bootstrap 状态）
    pub fn with_lifecycle_agent_repo(
        mut self,
        repo: Arc<dyn agentdash_domain::workflow::LifecycleAgentRepository>,
    ) -> Self {
        self.lifecycle_agent_repo = Some(repo);
        self
    }

    pub fn with_permission_grant_repo(
        mut self,
        repo: Arc<dyn agentdash_domain::permission::PermissionGrantRepository>,
    ) -> Self {
        self.permission_grant_repo = Some(repo);
        self
    }

    pub async fn set_agent_run_mailbox_runtime_adapter(
        &self,
        adapter: Arc<AgentRunMailboxRuntimeAdapter>,
    ) {
        *self.agent_run_mailbox_runtime_adapter.write().await = Some(adapter);
    }

    /// 注入 VFS 访问服务（用于 skill 扫描等需要跨 mount 读取的场景）
    pub fn with_vfs_service(mut self, service: Arc<crate::vfs::VfsService>) -> Self {
        self.vfs_service = Some(service);
        self
    }

    /// 注入插件提供的额外 Skill 扫描目录
    pub fn with_extra_skill_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.extra_skill_dirs = dirs;
        self
    }

    /// 注入 Host Integration 动态 Skill Discovery providers。
    pub fn with_skill_discovery_providers(
        mut self,
        providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
    ) -> Self {
        self.skill_discovery_providers = providers;
        self
    }

    /// 注入 session 终态全局回调（如 LifecycleOrchestrator）。
    ///
    /// 可在 SessionRuntimeInner 构造完成后调用（支持延迟注入解决循环依赖）。
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

    /// 注入 session launch envelope provider（frame / MCP / flow capabilities / context 等）。
    ///
    /// **何时必须注入**：只要 SessionRuntimeInner 会在内部发起 strict launch（如
    /// hook auto-resume、未来可能的其他系统驱动续跑），就必须注入此 provider，
    /// 让 auto-resume 的 prompt 与 HTTP 主通道使用同一份 envelope。
    ///
    /// 延迟注入设计：用 `Arc<RwLock<...>>` 以便在 AppState 构造完成后再绑定到 hub。
    pub async fn set_frame_launch_envelope_provider(
        &self,
        provider: SharedFrameLaunchEnvelopeProvider,
    ) {
        *self.frame_launch_envelope_provider.write().await = Some(provider);
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
            return Err("SessionRuntimeInner 缺少 runtime_tool_provider".to_string());
        }
        if self.mcp_tool_discovery.is_none() {
            return Err("SessionRuntimeInner 缺少 mcp_tool_discovery".to_string());
        }
        if self.terminal_callback.read().await.is_none() {
            return Err("SessionRuntimeInner 缺少 terminal_callback".to_string());
        }
        if self.hook_effect_handler_registry.read().await.is_none() {
            return Err("SessionRuntimeInner 缺少 hook_effect_handler_registry".to_string());
        }
        if self.frame_launch_envelope_provider.read().await.is_none() {
            return Err("SessionRuntimeInner 缺少 session_launch_envelope_provider".to_string());
        }
        if self.context_audit_bus.read().await.is_none() {
            return Err("SessionRuntimeInner 缺少 context_audit_bus".to_string());
        }
        if self.backend_execution_transport.is_none() {
            return Err("SessionRuntimeInner 缺少 backend_execution_transport".to_string());
        }
        if self.backend_execution_lease_repo.is_none() {
            return Err("SessionRuntimeInner 缺少 backend_execution_lease_repo".to_string());
        }
        if self.agent_frame_repo.is_none() {
            return Err("SessionRuntimeInner 缺少 agent_frame_repo".to_string());
        }
        if self.execution_anchor_repo.is_none() {
            return Err("SessionRuntimeInner 缺少 execution_anchor_repo".to_string());
        }
        if self.lifecycle_agent_repo.is_none() {
            return Err("SessionRuntimeInner 缺少 lifecycle_agent_repo".to_string());
        }
        if self.permission_grant_repo.is_none() {
            return Err("SessionRuntimeInner 缺少 permission_grant_repo".to_string());
        }
        if self
            .agent_run_mailbox_runtime_adapter
            .read()
            .await
            .is_none()
        {
            return Err("SessionRuntimeInner 缺少 agent_run_mailbox_runtime_adapter".to_string());
        }
        Ok(())
    }
}
