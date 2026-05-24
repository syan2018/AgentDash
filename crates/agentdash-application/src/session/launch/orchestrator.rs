use std::sync::Arc;

use agentdash_agent_protocol::SourceInfo;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::{AgentConnector, ConnectorError, McpRelayProvider};

use crate::context::SharedContextAuditBus;
use crate::session::capability_service::SessionCapabilityService;
use crate::session::construction::SessionConstructionPlan;
use crate::session::construction_provider::{
    SessionConstructionProviderInput, SharedSessionConstructionProvider,
};
use crate::session::core::SessionCoreService;
use crate::session::effects_service::SessionEffectsService;
use crate::session::eventing::SessionEventingService;
use crate::session::hooks_service::SessionHookService;
use crate::session::launch::{
    ConnectorStarter, LaunchCommand, LaunchCommandOutcome, LaunchPlanner, LaunchPlannerInput,
    StreamIngestionAttacher, TurnCommitter, TurnPreparationInput, TurnPreparer,
};
use crate::session::persistence::SessionStoreSet;
use crate::session::post_turn_handler::DynTerminalHookEffectHandlerRegistry;
use crate::session::runtime_commands::RuntimeCommandRecord;
use crate::session::runtime_registry::SessionRuntimeRegistry;
use crate::session::title_generator::SessionTitleGenerator;
use crate::session::turn_supervisor::TurnSupervisor;
use crate::session::types::*;

#[derive(Clone)]
pub(in crate::session) struct SessionLaunchDeps {
    pub(super) connector: Arc<dyn AgentConnector>,
    pub(super) runtime_registry: SessionRuntimeRegistry,
    pub(super) turn_supervisor: TurnSupervisor,
    pub(super) stores: SessionStoreSet,
    pub(super) title_generator: Option<Arc<dyn SessionTitleGenerator>>,
    pub(super) session_construction_provider:
        Arc<tokio::sync::RwLock<Option<SharedSessionConstructionProvider>>>,
    pub(super) hook_effect_handler_registry:
        Arc<tokio::sync::RwLock<Option<DynTerminalHookEffectHandlerRegistry>>>,
    pub(super) context_audit_bus: Arc<tokio::sync::RwLock<Option<SharedContextAuditBus>>>,
    pub(super) base_system_prompt: String,
    pub(super) user_preferences: Vec<String>,
    pub(super) runtime_tool_provider: Option<Arc<dyn RuntimeToolProvider>>,
    pub(super) mcp_relay_provider: Option<Arc<dyn McpRelayProvider>>,
    pub(super) eventing: SessionEventingService,
    pub(super) core: SessionCoreService,
    pub(super) hooks: SessionHookService,
    pub(super) capability: SessionCapabilityService,
    pub(super) effects: SessionEffectsService,
}

impl SessionLaunchDeps {
    pub(super) async fn current_session_construction_provider(
        &self,
    ) -> Option<SharedSessionConstructionProvider> {
        self.session_construction_provider.read().await.clone()
    }

    pub(super) async fn current_context_audit_bus(&self) -> Option<SharedContextAuditBus> {
        self.context_audit_bus.read().await.clone()
    }

    pub(super) async fn build_tools_for_execution_context(
        &self,
        session_id: &str,
        context: &agentdash_spi::ExecutionContext,
        mcp_servers: &[agentdash_spi::SessionMcpServer],
    ) -> Vec<agentdash_agent_types::DynAgentTool> {
        use agentdash_executor::mcp::{self as mcp_discovery};

        let mut all_tools = Vec::new();

        if let Some(provider) = &self.runtime_tool_provider {
            match provider.build_tools(context).await {
                Ok(tools) => all_tools.extend(tools),
                Err(e) => tracing::warn!(
                    session_id = %session_id,
                    "runtime tool 构建失败: {e}"
                ),
            }
        }

        let (relay_names, direct_servers) =
            agentdash_spi::partition_session_mcp_servers(mcp_servers);
        match mcp_discovery::discover_mcp_tools(&direct_servers, &context.turn.capability_state)
            .await
        {
            Ok(tools) => all_tools.extend(tools),
            Err(e) => tracing::warn!(
                session_id = %session_id,
                "直连 MCP 工具发现失败: {e}"
            ),
        }

        if let Some(relay) = &self.mcp_relay_provider {
            let call_context = agentdash_spi::RelayMcpCallContext {
                session_id: session_id.to_string(),
                turn_id: Some(context.session.turn_id.clone()),
                tool_call_id: None,
                vfs: context.session.vfs.clone(),
                identity: context.session.identity.clone(),
            };
            let tools = mcp_discovery::discover_relay_mcp_tools(
                relay.clone(),
                &relay_names,
                &context.turn.capability_state,
                Some(call_context),
            )
            .await;
            all_tools.extend(tools);
        }

        all_tools
    }

    pub(super) fn spawn_title_generation(&self, session_id: String, user_prompt: String) {
        let Some(generator) = self.title_generator.clone() else {
            return;
        };
        let eventing = self.eventing.clone();
        let core = self.core.clone();

        tokio::spawn(async move {
            let result = generator.generate_title(&user_prompt).await;
            match result {
                Ok(title) if !title.trim().is_empty() => {
                    let title = title.trim().to_string();
                    let updated = core
                        .update_session_meta(&session_id, |meta| {
                            if meta.title_source == TitleSource::User {
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
                                &session_id,
                                source,
                            );
                            let _ = eventing.persist_notification(&session_id, envelope).await;
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
                Ok(_) => {
                    tracing::warn!(session_id = %session_id, "LLM 返回了空标题，保留原标题");
                }
                Err(reason) => {
                    tracing::warn!(
                        session_id = %session_id,
                        reason = %reason,
                        "自动标题生成失败，保留原标题"
                    );
                }
            }
        });
    }
}

pub(in crate::session) struct SessionLaunchOrchestrator {
    deps: SessionLaunchDeps,
}

struct LaunchRuntimeFacts {
    turn_id: String,
    had_existing_runtime: bool,
    session_meta: SessionMeta,
    requested_runtime_commands: Vec<RuntimeCommandRecord>,
    context_sources: Vec<String>,
}

impl SessionLaunchOrchestrator {
    pub fn new(deps: SessionLaunchDeps) -> Self {
        Self { deps }
    }

    pub async fn launch(
        &self,
        session_id: &str,
        command: LaunchCommand,
    ) -> Result<LaunchCommandOutcome, ConnectorError> {
        let reason = command.reason_tag();
        let Some(provider) = self.deps.current_session_construction_provider().await else {
            return Err(ConnectorError::Runtime(format!(
                "session_construction_provider 未注入，拒绝 session launch: {reason}"
            )));
        };
        let turn_id = format!("t{}", chrono::Utc::now().timestamp_millis());
        let had_existing_runtime = self.deps.connector.has_live_session(session_id).await;
        let _cached_continuation = self.deps.turn_supervisor.claim_prompt(session_id).await?;
        let sid = session_id.to_string();
        let meta_store = self.deps.stores.meta.clone();
        let runtime_command_store = self.deps.stores.runtime_commands.clone();
        let session_meta = match meta_store.get_session_meta(&sid).await {
            Ok(Some(meta)) => meta,
            Ok(None) => {
                self.deps
                    .turn_supervisor
                    .clear_turn_and_hook(session_id)
                    .await;
                return Err(ConnectorError::Runtime(format!(
                    "session {sid} 不存在，请先调用 create_session 再 prompt"
                )));
            }
            Err(e) => {
                self.deps
                    .turn_supervisor
                    .clear_turn_and_hook(session_id)
                    .await;
                return Err(ConnectorError::Runtime(format!(
                    "读取 session {sid} meta 失败: {e}"
                )));
            }
        };
        let requested_runtime_commands = match runtime_command_store
            .list_requested_runtime_commands(&sid)
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!(
                    "读取 session `{sid}` requested runtime commands 失败: {error}"
                ))
            }) {
            Ok(commands) => commands,
            Err(error) => {
                self.deps
                    .turn_supervisor
                    .clear_turn_and_hook(session_id)
                    .await;
                return Err(error);
            }
        };
        let construction = match provider
            .build_construction(SessionConstructionProviderInput {
                session_id: sid.clone(),
                command: command.clone(),
                session_meta: session_meta.clone(),
                had_existing_runtime,
                requested_runtime_commands: requested_runtime_commands.clone(),
            })
            .await
        {
            Ok(construction) => construction,
            Err(error) => {
                self.deps
                    .turn_supervisor
                    .clear_turn_and_hook(session_id)
                    .await;
                return Err(error);
            }
        };
        let context_sources = construction
            .context
            .bundle
            .as_ref()
            .map(|bundle| {
                bundle
                    .iter_fragments()
                    .map(|fragment| format!("{}({})", fragment.label, fragment.slot))
                    .collect()
            })
            .unwrap_or_default();
        let facts = LaunchRuntimeFacts {
            turn_id,
            had_existing_runtime,
            session_meta,
            requested_runtime_commands,
            context_sources,
        };
        let context_sources = facts.context_sources.clone();
        let turn_id = self
            .launch_with_construction(session_id, &command, construction, facts)
            .await?;
        Ok(LaunchCommandOutcome {
            turn_id,
            context_sources,
        })
    }

    #[cfg(test)]
    pub(crate) async fn launch_with_construction_for_test(
        &self,
        session_id: &str,
        construction: SessionConstructionPlan,
    ) -> Result<String, ConnectorError> {
        let user_input = UserPromptInput {
            prompt_blocks: construction.prompt.prompt_blocks.clone(),
            env: construction.prompt.environment_variables.clone(),
            executor_config: construction.execution_profile.executor_config.clone(),
        };
        let command = LaunchCommand::http_prompt_input(user_input, None);
        let turn_id = format!("t{}", chrono::Utc::now().timestamp_millis());
        let had_existing_runtime = self.deps.connector.has_live_session(session_id).await;
        let _cached_continuation = self.deps.turn_supervisor.claim_prompt(session_id).await?;
        let sid = session_id.to_string();
        let session_meta = self
            .deps
            .stores
            .meta
            .get_session_meta(&sid)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let Some(session_meta) = session_meta else {
            self.deps
                .turn_supervisor
                .clear_turn_and_hook(session_id)
                .await;
            return Err(ConnectorError::Runtime(format!("session {sid} 不存在")));
        };
        let requested_runtime_commands = self
            .deps
            .stores
            .runtime_commands
            .list_requested_runtime_commands(&sid)
            .await
            .map_err(|error| ConnectorError::Runtime(error.to_string()))?;
        let construction =
            Self::finalize_construction_for_test(construction, &requested_runtime_commands);
        let facts = LaunchRuntimeFacts {
            turn_id,
            had_existing_runtime,
            session_meta,
            requested_runtime_commands,
            context_sources: Vec::new(),
        };
        self.launch_with_construction(session_id, &command, construction, facts)
            .await
    }

    #[cfg(test)]
    fn finalize_construction_for_test(
        mut construction: SessionConstructionPlan,
        requested_runtime_commands: &[crate::session::runtime_commands::RuntimeCommandRecord],
    ) -> SessionConstructionPlan {
        let mut base_capability_state = construction
            .projections
            .capability_state
            .clone()
            .unwrap_or_default();
        if let Some(vfs) = construction.surface.vfs.clone() {
            base_capability_state.vfs.active = Some(vfs);
        }
        base_capability_state.tool.mcp_servers = construction.projections.mcp_servers.clone();

        let requested_transitions = requested_runtime_commands
            .iter()
            .map(|command| command.transition.clone())
            .collect::<Vec<_>>();
        let replay = if requested_transitions.is_empty() {
            None
        } else {
            crate::session::capability_state::replay_runtime_capability_transitions(
                &base_capability_state,
                &requested_transitions,
            )
            .ok()
        };
        let mut final_capability_state = replay
            .as_ref()
            .map(|replay| replay.capability_state.clone())
            .unwrap_or_else(|| base_capability_state.clone());
        if let Some(base_vfs) = construction.surface.vfs.clone() {
            let effective_vfs = replay
                .as_ref()
                .and_then(|replay| replay.effective_vfs.clone())
                .unwrap_or(base_vfs);
            construction.workspace.working_directory = effective_vfs
                .default_mount()
                .map(|mount| std::path::PathBuf::from(mount.root_ref.trim()))
                .filter(|path| !path.as_os_str().is_empty())
                .or(construction.workspace.working_directory);
            construction.surface.vfs = Some(effective_vfs.clone());
            final_capability_state.vfs.active = Some(effective_vfs);
        }
        let effective_mcp_servers = replay
            .as_ref()
            .and_then(|replay| replay.effective_mcp_servers.clone())
            .unwrap_or_else(|| construction.projections.mcp_servers.clone());
        construction.projections.mcp_servers = effective_mcp_servers.clone();
        final_capability_state.tool.mcp_servers = effective_mcp_servers;
        construction.projections.capability_state = Some(final_capability_state);
        construction.resolution.runtime_base_capability_state = Some(base_capability_state);
        if requested_runtime_commands.is_empty() {
            construction.resolution.pending_overlay_applied = false;
        } else {
            construction.resolution.vfs_source = Some("test.pending_runtime_command".to_string());
            construction.resolution.mcp_source = Some("test.pending_runtime_command".to_string());
            construction.resolution.capability_source =
                Some("test.pending_runtime_command".to_string());
            construction.resolution.pending_overlay_applied = true;
        }
        construction
    }

    /// 已完成 construction plan 准备后的内部 stage runner。生产入口只能从 `launch` 进入。
    async fn launch_with_construction(
        &self,
        session_id: &str,
        command: &LaunchCommand,
        construction: SessionConstructionPlan,
        facts: LaunchRuntimeFacts,
    ) -> Result<String, ConnectorError> {
        let LaunchRuntimeFacts {
            turn_id,
            had_existing_runtime,
            mut session_meta,
            requested_runtime_commands,
            context_sources: _context_sources,
        } = facts;
        let deps = &self.deps;
        let sid = session_id.to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let launch_plan = match LaunchPlanner::new(deps.clone())
            .plan(LaunchPlannerInput {
                session_id,
                turn_id: &turn_id,
                command,
                had_existing_runtime,
                session_meta: &session_meta,
                requested_runtime_commands,
                construction,
            })
            .await
        {
            Ok(launch_plan) => launch_plan,
            Err(error) => {
                deps.turn_supervisor.clear_turn_and_hook(session_id).await;
                return Err(error);
            }
        };
        let prepared = TurnPreparer::new(deps.clone())
            .prepare(TurnPreparationInput {
                launch_plan,
                session_id: sid.clone(),
                turn_id: turn_id.clone(),
                had_existing_runtime,
            })
            .await?;
        let accepted = ConnectorStarter::new(deps.clone()).start(prepared).await?;
        let committed = TurnCommitter::new(deps.clone())
            .commit(accepted, &mut session_meta, now)
            .await?;
        let attached = StreamIngestionAttacher::new(deps.clone())
            .attach(committed)
            .await;

        Ok(attached.turn_id)
    }
}

#[cfg(test)]
mod tests {
    use crate::session::baseline_capabilities::build_session_baseline_capabilities;

    #[test]
    fn baseline_capabilities_built_from_skills() {
        let caps = build_session_baseline_capabilities(&[agentdash_spi::SkillRef {
            name: "my-skill".to_string(),
            description: "test".to_string(),
            file_path: "/ws/SKILL.md".into(),
            base_dir: "/ws".into(),
            disable_model_invocation: false,
        }]);
        assert_eq!(caps.skills.len(), 1);
        assert_eq!(caps.skills[0].name, "my-skill");
    }
}
