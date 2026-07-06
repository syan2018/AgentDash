use std::{path::PathBuf, sync::Arc};

use agentdash_application_ports::agent_run_control_effect::{
    AgentRunControlEffectPort, AgentRunTerminalHookTriggerPort,
};
use agentdash_application_ports::agent_run_surface::{
    AgentRunEffectiveCapabilityPort, AgentRunRuntimeSurfaceQueryPort,
};
use agentdash_application_ports::frame_launch_envelope::{
    AcceptedLaunchCommitPort, SharedFrameLaunchEnvelopePort,
};
use agentdash_application_ports::mcp_discovery::McpToolDiscovery;
use agentdash_application_ports::runtime_session_live::{
    RuntimeSessionEffectiveCapabilityPort, RuntimeSessionHookTargetPort,
    RuntimeSessionMailboxRuntimePort,
};
use agentdash_application_ports::runtime_surface_adoption::RuntimeSurfaceAdoptionPort;
use agentdash_spi::AgentConnector;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::hooks::ExecutionHookProvider;

use super::branching::SessionBranchingService;
use super::control::SessionControlService;
use super::core::SessionCoreService;
use super::eventing::SessionEventingService;
use super::hooks_service::SessionHookService;
use super::hub::SessionRuntimeInner;
use super::launch::SessionLaunchService;
use super::persistence::SessionStoreSet;
use super::runtime_control::SessionRuntimeService;
use super::runtime_transition_service::SessionRuntimeTransitionService;
use super::terminal_boundary_service::RuntimeTerminalBoundaryService;
use super::title_service::SessionTitleService;
use crate::context::SharedContextAuditBus;

pub struct SessionRuntimeBuilder {
    inner: SessionRuntimeInner,
}

impl SessionRuntimeBuilder {
    pub fn new_with_hooks_and_stores(
        connector: Arc<dyn AgentConnector>,
        hook_provider: Option<Arc<dyn ExecutionHookProvider>>,
        stores: SessionStoreSet,
    ) -> Self {
        Self {
            inner: SessionRuntimeInner::new_with_hooks_and_stores(connector, hook_provider, stores),
        }
    }

    pub fn with_vfs_service<T>(mut self, service: Arc<T>) -> Self
    where
        T: Send + Sync + 'static,
    {
        self.inner = self.inner.with_vfs_service(service);
        self
    }

    pub fn with_extra_skill_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.inner = self.inner.with_extra_skill_dirs(dirs);
        self
    }

    pub fn with_skill_discovery_providers(
        mut self,
        providers: Vec<Arc<dyn agentdash_spi::SkillDiscoveryProvider>>,
    ) -> Self {
        self.inner = self.inner.with_skill_discovery_providers(providers);
        self
    }

    pub fn with_runtime_tool_provider(mut self, provider: Arc<dyn RuntimeToolProvider>) -> Self {
        self.inner = self.inner.with_runtime_tool_provider(provider);
        self
    }

    pub fn with_mcp_tool_discovery(mut self, provider: Arc<dyn McpToolDiscovery>) -> Self {
        self.inner = self.inner.with_mcp_tool_discovery(provider);
        self
    }

    pub fn with_backend_execution_placement(
        mut self,
        transport: Arc<dyn agentdash_application_ports::backend_transport::RelayPromptTransport>,
        lease_repo: Arc<dyn agentdash_domain::backend::BackendExecutionLeaseRepository>,
    ) -> Self {
        self.inner = self
            .inner
            .with_backend_execution_placement(transport, lease_repo);
        self
    }

    pub fn with_agent_frame_repo(
        mut self,
        repo: Arc<dyn agentdash_domain::workflow::AgentFrameRepository>,
    ) -> Self {
        self.inner = self.inner.with_agent_frame_repo(repo);
        self
    }

    pub fn with_execution_anchor_repo(
        mut self,
        repo: Arc<dyn agentdash_domain::workflow::RuntimeSessionExecutionAnchorRepository>,
    ) -> Self {
        self.inner = self.inner.with_execution_anchor_repo(repo);
        self
    }

    pub fn with_runtime_surface_query(
        mut self,
        query: Arc<dyn AgentRunRuntimeSurfaceQueryPort>,
    ) -> Self {
        self.inner = self.inner.with_runtime_surface_query(query);
        self
    }

    pub fn with_agent_run_effective_capability_port(
        mut self,
        port: Arc<dyn AgentRunEffectiveCapabilityPort>,
    ) -> Self {
        self.inner = self.inner.with_agent_run_effective_capability_port(port);
        self
    }

    pub fn with_system_prompt_config(mut self, base_system_prompt: String) -> Self {
        self.inner = self.inner.with_system_prompt_config(base_system_prompt);
        self
    }

    pub fn with_settings_repository(
        mut self,
        repo: Arc<dyn agentdash_domain::settings::SettingsRepository>,
    ) -> Self {
        self.inner = self.inner.with_settings_repository(repo);
        self
    }

    pub fn core_service(&self) -> SessionCoreService {
        self.inner.core_service()
    }

    pub fn branching_service(&self) -> SessionBranchingService {
        self.inner.branching_service()
    }

    pub fn eventing_service(&self) -> SessionEventingService {
        self.inner.eventing_service()
    }

    pub fn runtime_service(&self) -> SessionRuntimeService {
        self.inner.runtime_service()
    }

    pub fn control_service(&self) -> SessionControlService {
        self.inner.control_service()
    }

    pub fn launch_service(&self) -> SessionLaunchService {
        self.inner.launch_service()
    }

    pub fn hook_service(&self) -> SessionHookService {
        self.inner.hook_service()
    }

    pub fn runtime_transition_service(&self) -> SessionRuntimeTransitionService {
        self.inner.runtime_transition_service()
    }

    pub fn runtime_surface_adoption_port(&self) -> Arc<dyn RuntimeSurfaceAdoptionPort> {
        Arc::new(self.inner.clone())
    }

    pub fn agent_run_terminal_hook_trigger_port(&self) -> Arc<dyn AgentRunTerminalHookTriggerPort> {
        self.inner.agent_run_terminal_hook_trigger_port()
    }

    pub fn terminal_boundary_service(&self) -> RuntimeTerminalBoundaryService {
        self.inner.terminal_boundary_service()
    }

    pub fn title_service(&self) -> SessionTitleService {
        self.inner.title_service()
    }

    pub fn with_lifecycle_gate_repo(
        mut self,
        repo: Arc<dyn agentdash_domain::workflow::LifecycleGateRepository>,
    ) -> Self {
        self.inner = self.inner.with_lifecycle_gate_repo(repo);
        self
    }

    pub fn with_lifecycle_agent_repo(
        mut self,
        repo: Arc<dyn agentdash_domain::workflow::LifecycleAgentRepository>,
    ) -> Self {
        self.inner = self.inner.with_lifecycle_agent_repo(repo);
        self
    }

    pub fn with_permission_grant_repo(
        mut self,
        repo: Arc<dyn agentdash_domain::permission::PermissionGrantRepository>,
    ) -> Self {
        self.inner = self.inner.with_permission_grant_repo(repo);
        self
    }

    pub fn with_effective_capability_port(
        mut self,
        port: Arc<dyn RuntimeSessionEffectiveCapabilityPort>,
    ) -> Self {
        self.inner = self.inner.with_effective_capability_port(port);
        self
    }

    pub fn with_hook_target_port(mut self, port: Arc<dyn RuntimeSessionHookTargetPort>) -> Self {
        self.inner = self.inner.with_hook_target_port(port);
        self
    }

    pub fn with_workspace_title_port(
        mut self,
        port: Arc<dyn agentdash_application_ports::workspace_title::WorkspaceTitlePort>,
    ) -> Self {
        self.inner = self.inner.with_workspace_title_port(port);
        self
    }

    pub async fn set_mailbox_runtime_port(&self, port: Arc<dyn RuntimeSessionMailboxRuntimePort>) {
        self.inner.set_mailbox_runtime_port(port).await;
    }

    pub async fn set_agent_run_control_effect_port(
        &self,
        port: Arc<dyn AgentRunControlEffectPort>,
    ) {
        self.inner.set_agent_run_control_effect_port(port).await;
    }

    pub async fn set_hook_effect_handler_registry(
        &self,
        registry: super::post_turn_handler::DynTerminalHookEffectHandlerRegistry,
    ) {
        self.inner.set_hook_effect_handler_registry(registry).await;
    }

    pub async fn set_frame_launch_envelope_provider(
        &self,
        provider: SharedFrameLaunchEnvelopePort,
    ) {
        self.inner
            .set_frame_launch_envelope_provider(provider)
            .await;
    }

    pub async fn set_accepted_launch_commit_port(&self, port: Arc<dyn AcceptedLaunchCommitPort>) {
        self.inner.set_accepted_launch_commit_port(port).await;
    }

    pub async fn set_context_audit_bus(&self, bus: SharedContextAuditBus) {
        self.inner.set_context_audit_bus(bus).await;
    }

    pub async fn assert_ready_for_app_state(&self) -> Result<(), String> {
        self.inner.assert_ready_for_app_state().await
    }
}
