use std::sync::Arc;

use agentdash_agent_protocol::SourceInfo;
use agentdash_application_ports::mcp_discovery::McpToolDiscovery;
use agentdash_domain::backend::BackendExecutionLeaseRepository;
use agentdash_domain::settings::SettingsRepository;
use agentdash_domain::workflow::{
    AgentFrameRepository, LifecycleAgentRepository, RuntimeSessionExecutionAnchorRepository,
};
use agentdash_spi::AgentConnector;
use agentdash_spi::connector::RuntimeToolProvider;

use crate::agent_run::frame::launch_envelope_provider::SharedFrameLaunchEnvelopeProvider;
use crate::agent_run::{
    AgentRunAcceptedLaunchCommitAdapter, AgentRunAcceptedLaunchCommitDeps,
    AgentRunMailboxRuntimeAdapter,
};
use crate::context::SharedContextAuditBus;
use crate::session::core::SessionCoreService;
use crate::session::effects_service::SessionEffectsService;
use crate::session::eventing::SessionEventingService;
use crate::session::hooks_service::SessionHookService;
use crate::session::hub::SessionRuntimeInner;
use crate::session::persistence::SessionStoreSet;
use crate::session::post_turn_handler::DynTerminalHookEffectHandlerRegistry;
use crate::session::runtime_registry::SessionRuntimeRegistry;
use crate::session::runtime_transition_service::SessionRuntimeTransitionService;
use crate::session::title_generator::derive_session_title;
use crate::session::tool_assembly::{
    AssembledToolSurface, assemble_tool_surface_for_execution_context,
};
use crate::session::turn_supervisor::TurnSupervisor;
use crate::session::types::TitleSource;
use agentdash_application_ports::backend_transport::RelayPromptTransport;

#[derive(Clone)]
pub(in crate::session) struct SessionLaunchDeps {
    pub(super) connector: Arc<dyn AgentConnector>,
    pub(super) turn_supervisor: TurnSupervisor,
    pub(super) stores: SessionStoreSet,
    pub(super) frame_launch_envelope_provider:
        Arc<tokio::sync::RwLock<Option<SharedFrameLaunchEnvelopeProvider>>>,
    runtime_registry: SessionRuntimeRegistry,
    hook_effect_handler_registry:
        Arc<tokio::sync::RwLock<Option<DynTerminalHookEffectHandlerRegistry>>>,
    context_audit_bus: Arc<tokio::sync::RwLock<Option<SharedContextAuditBus>>>,
    base_system_prompt: String,
    settings_repo: Option<Arc<dyn SettingsRepository>>,
    runtime_tool_provider: Option<Arc<dyn RuntimeToolProvider>>,
    mcp_tool_discovery: Option<Arc<dyn McpToolDiscovery>>,
    pub(super) backend_execution_transport: Option<Arc<dyn RelayPromptTransport>>,
    pub(super) backend_execution_lease_repo: Option<Arc<dyn BackendExecutionLeaseRepository>>,
    pub(super) agent_frame_repo: Option<Arc<dyn AgentFrameRepository>>,
    pub(super) execution_anchor_repo: Option<Arc<dyn RuntimeSessionExecutionAnchorRepository>>,
    pub(super) lifecycle_agent_repo: Option<Arc<dyn LifecycleAgentRepository>>,
    pub(super) agent_run_mailbox_runtime_adapter:
        Arc<tokio::sync::RwLock<Option<Arc<AgentRunMailboxRuntimeAdapter>>>>,
    eventing: SessionEventingService,
    core: SessionCoreService,
    hooks: SessionHookService,
    runtime_transition: SessionRuntimeTransitionService,
    effects: SessionEffectsService,
}

impl SessionLaunchDeps {
    pub(in crate::session) fn from_inner(inner: &SessionRuntimeInner) -> Self {
        Self {
            connector: inner.connector.clone(),
            runtime_registry: inner.runtime_registry.clone(),
            turn_supervisor: inner.turn_supervisor.clone(),
            stores: inner.stores.clone(),
            frame_launch_envelope_provider: inner.frame_launch_envelope_provider.clone(),
            hook_effect_handler_registry: inner.hook_effect_handler_registry.clone(),
            context_audit_bus: inner.context_audit_bus.clone(),
            base_system_prompt: inner.base_system_prompt.clone(),
            settings_repo: inner.settings_repo.clone(),
            runtime_tool_provider: inner.runtime_tool_provider.clone(),
            mcp_tool_discovery: inner.mcp_tool_discovery.clone(),
            backend_execution_transport: inner.backend_execution_transport.clone(),
            backend_execution_lease_repo: inner.backend_execution_lease_repo.clone(),
            agent_frame_repo: inner.agent_frame_repo.clone(),
            execution_anchor_repo: inner.execution_anchor_repo.clone(),
            lifecycle_agent_repo: inner.lifecycle_agent_repo.clone(),
            agent_run_mailbox_runtime_adapter: inner.agent_run_mailbox_runtime_adapter.clone(),
            eventing: inner.eventing_service(),
            core: inner.core_service(),
            hooks: inner.hook_service(),
            runtime_transition: inner.runtime_transition_service(),
            effects: inner.effects_service(),
        }
    }

    pub(super) async fn current_frame_launch_envelope_provider(
        &self,
    ) -> Option<SharedFrameLaunchEnvelopeProvider> {
        self.frame_launch_envelope_provider.read().await.clone()
    }

    pub(super) fn planning(&self) -> LaunchPlanningDeps {
        LaunchPlanningDeps {
            connector: self.connector.clone(),
            runtime_registry: self.runtime_registry.clone(),
            eventing: self.eventing.clone(),
            hooks: self.hooks.clone(),
            hook_effect_handler_registry: self.hook_effect_handler_registry.clone(),
            context_audit_bus: self.context_audit_bus.clone(),
            backend_execution_transport: self.backend_execution_transport.clone(),
            backend_execution_lease_repo: self.backend_execution_lease_repo.clone(),
            agent_run_mailbox_runtime_adapter: self.agent_run_mailbox_runtime_adapter.clone(),
        }
    }

    pub(super) fn preparation(&self) -> TurnPreparationDeps {
        TurnPreparationDeps {
            connector: self.connector.clone(),
            turn_supervisor: self.turn_supervisor.clone(),
            base_system_prompt: self.base_system_prompt.clone(),
            settings_repo: self.settings_repo.clone(),
            runtime_tool_provider: self.runtime_tool_provider.clone(),
            mcp_tool_discovery: self.mcp_tool_discovery.clone(),
            hooks: self.hooks.clone(),
            runtime_transition: self.runtime_transition.clone(),
        }
    }

    pub(super) fn connector_start(&self) -> ConnectorStartDeps {
        ConnectorStartDeps {
            connector: self.connector.clone(),
            turn_supervisor: self.turn_supervisor.clone(),
            eventing: self.eventing.clone(),
        }
    }

    pub(super) fn commit(&self) -> TurnCommitDeps {
        TurnCommitDeps {
            stores: self.stores.clone(),
            eventing: self.eventing.clone(),
            core: self.core.clone(),
            turn_supervisor: self.turn_supervisor.clone(),
            accepted_launch_commit: self.accepted_launch_commit_adapter(),
        }
    }

    pub(super) fn accepted_launch_commit_adapter(&self) -> AgentRunAcceptedLaunchCommitAdapter {
        AgentRunAcceptedLaunchCommitAdapter::new(AgentRunAcceptedLaunchCommitDeps {
            frame_repo: self.agent_frame_repo.clone(),
            anchor_repo: self.execution_anchor_repo.clone(),
            agent_repo: self.lifecycle_agent_repo.clone(),
            hook_runtime_sync: Some(Arc::new(self.hooks.clone())),
        })
    }

    pub(super) fn ingestion(&self) -> StreamIngestionDeps {
        StreamIngestionDeps {
            turn_supervisor: self.turn_supervisor.clone(),
            eventing: self.eventing.clone(),
            effects: self.effects.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct LaunchPlanningDeps {
    pub(super) connector: Arc<dyn AgentConnector>,
    pub(super) runtime_registry: SessionRuntimeRegistry,
    pub(super) eventing: SessionEventingService,
    pub(super) hooks: SessionHookService,
    pub(super) hook_effect_handler_registry:
        Arc<tokio::sync::RwLock<Option<DynTerminalHookEffectHandlerRegistry>>>,
    context_audit_bus: Arc<tokio::sync::RwLock<Option<SharedContextAuditBus>>>,
    pub(super) backend_execution_transport: Option<Arc<dyn RelayPromptTransport>>,
    pub(super) backend_execution_lease_repo: Option<Arc<dyn BackendExecutionLeaseRepository>>,
    agent_run_mailbox_runtime_adapter:
        Arc<tokio::sync::RwLock<Option<Arc<AgentRunMailboxRuntimeAdapter>>>>,
}

impl LaunchPlanningDeps {
    pub(super) async fn current_context_audit_bus(&self) -> Option<SharedContextAuditBus> {
        self.context_audit_bus.read().await.clone()
    }

    pub(super) async fn current_agent_run_mailbox_runtime_adapter(
        &self,
    ) -> Option<Arc<AgentRunMailboxRuntimeAdapter>> {
        self.agent_run_mailbox_runtime_adapter.read().await.clone()
    }
}

#[derive(Clone)]
pub(super) struct TurnPreparationDeps {
    pub(super) connector: Arc<dyn AgentConnector>,
    pub(super) turn_supervisor: TurnSupervisor,
    pub(super) base_system_prompt: String,
    pub(super) settings_repo: Option<Arc<dyn SettingsRepository>>,
    pub(super) hooks: SessionHookService,
    pub(super) runtime_transition: SessionRuntimeTransitionService,
    runtime_tool_provider: Option<Arc<dyn RuntimeToolProvider>>,
    mcp_tool_discovery: Option<Arc<dyn McpToolDiscovery>>,
}

impl TurnPreparationDeps {
    pub(super) async fn assemble_tool_surface(
        &self,
        session_id: &str,
        context: &agentdash_spi::ExecutionContext,
    ) -> AssembledToolSurface {
        assemble_tool_surface_for_execution_context(
            session_id,
            context,
            self.runtime_tool_provider.as_deref(),
            self.mcp_tool_discovery.as_deref(),
        )
        .await
    }
}

#[derive(Clone)]
pub(super) struct ConnectorStartDeps {
    pub(super) connector: Arc<dyn AgentConnector>,
    pub(super) turn_supervisor: TurnSupervisor,
    pub(super) eventing: SessionEventingService,
}

#[derive(Clone)]
pub(super) struct TurnCommitDeps {
    pub(super) stores: SessionStoreSet,
    pub(super) eventing: SessionEventingService,
    pub(super) turn_supervisor: TurnSupervisor,
    pub(super) accepted_launch_commit: AgentRunAcceptedLaunchCommitAdapter,
    core: SessionCoreService,
}

impl TurnCommitDeps {
    pub(super) async fn apply_auto_title(&self, session_id: &str, user_prompt: &str) {
        let Some(title) = derive_session_title(user_prompt) else {
            return;
        };

        let updated = self
            .core
            .update_session_meta(session_id, |meta| {
                if meta.title_source != TitleSource::Auto {
                    return;
                }
                meta.title = title;
                meta.title_source = TitleSource::Auto;
            })
            .await;
        match updated {
            Ok(Some(meta)) => {
                let source = SourceInfo {
                    connector_id: "agentdash-server".to_string(),
                    connector_type: "system".to_string(),
                    executor_id: None,
                };
                let envelope = agentdash_agent_protocol::BackboneEnvelope::new(
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                            key: "session_meta_updated".to_string(),
                            value: serde_json::json!({
                                "title": meta.title,
                                "title_source": meta.title_source,
                            }),
                        },
                    ),
                    session_id,
                    source,
                );
                let _ = self
                    .eventing
                    .persist_notification(session_id, envelope)
                    .await;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    session_id = %session_id,
                    error = %error,
                    "自动标题写入失败"
                );
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct StreamIngestionDeps {
    pub(super) turn_supervisor: TurnSupervisor,
    pub(super) eventing: SessionEventingService,
    pub(super) effects: SessionEffectsService,
}
