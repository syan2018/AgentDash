use std::path::Path;
use std::sync::Arc;

use agentdash_agent_protocol::SourceInfo;
use agentdash_spi::connector::RuntimeToolProvider;
use agentdash_spi::hooks::{
    ContextFrame, ContextFrameSection, HookTrigger, HookTurnStartNotice, SessionHookSnapshot,
    SessionHookSnapshotQuery, SharedHookSessionRuntime,
};
use agentdash_spi::{AgentConnector, ConnectorError, McpRelayProvider};

use super::assignment_context_frame::build_assignment_context_frame;
use super::capability_service::SessionCapabilityService;
use super::construction::SessionConstructionPlan;
use super::construction_provider::{
    SessionConstructionProviderInput, SharedSessionConstructionProvider,
};
use super::core::SessionCoreService;
use super::effects_service::SessionEffectsService;
use super::eventing::SessionEventingService;
use super::hook_runtime::HookSessionRuntime;
use super::hooks_service::SessionHookService;
use super::hub::SessionRuntimeInner;
use super::hub::{HookTriggerInput, build_initial_capability_state_frame};
use super::hub_support::*;
use super::identity_context_frame::{IdentityFrameInput, build_identity_context_frame};
use super::launch::{LaunchCommand, LaunchCommandOutcome, LaunchStrictness};
use super::launch_planner::{SessionLaunchPlanner, SessionLaunchPlannerInput};
use super::pending_action_context_frame::build_pending_action_context_frame;
use super::persistence::{SessionRuntimeCommandStore, SessionStoreSet};
use super::post_turn_handler::DynTerminalHookEffectHandlerRegistry;
use super::runtime_registry::SessionRuntimeRegistry;
use super::title_generator::SessionTitleGenerator;
use super::turn_supervisor::TurnSupervisor;
pub use super::types::*;
use crate::context::SharedContextAuditBus;

#[derive(Clone)]
pub(super) struct SessionLaunchDeps {
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

pub(super) struct SessionLaunchExecutor {
    deps: SessionLaunchDeps,
}

impl SessionLaunchExecutor {
    pub fn new(deps: SessionLaunchDeps) -> Self {
        Self { deps }
    }

    pub async fn execute_command(
        &self,
        session_id: &str,
        command: LaunchCommand,
    ) -> Result<LaunchCommandOutcome, ConnectorError> {
        let reason = command.reason_tag();
        let provider = match command.strictness() {
            LaunchStrictness::Strict => {
                let Some(provider) = self.deps.current_session_construction_provider().await else {
                    return Err(ConnectorError::Runtime(format!(
                        "session_construction_provider 未注入，拒绝 strict launch: {reason}"
                    )));
                };
                provider
            }
            LaunchStrictness::Relaxed => {
                let Some(provider) = self.deps.current_session_construction_provider().await else {
                    return Err(ConnectorError::Runtime(format!(
                        "session_construction_provider 未注入，拒绝 relaxed launch: {reason}"
                    )));
                };
                provider
            }
        };
        let turn_id = format!("t{}", chrono::Utc::now().timestamp_millis());
        let had_existing_runtime = self.deps.connector.has_live_session(session_id).await;
        let cached_continuation = self.deps.turn_supervisor.claim_prompt(session_id).await?;
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
                cached_capability_state: cached_continuation
                    .as_ref()
                    .map(|profile| profile.capability_state.clone()),
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
        let turn_id = self
            .execute_constructed_launch(
                session_id,
                &command,
                construction,
                turn_id,
                had_existing_runtime,
                session_meta,
                requested_runtime_commands,
            )
            .await?;
        Ok(LaunchCommandOutcome {
            turn_id,
            context_sources,
        })
    }

    #[cfg(test)]
    pub(crate) async fn execute_constructed_launch_for_test(
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
        self.execute_constructed_launch(
            session_id,
            &command,
            construction,
            turn_id,
            had_existing_runtime,
            session_meta,
            requested_runtime_commands,
        )
        .await
    }

    #[cfg(test)]
    fn finalize_construction_for_test(
        mut construction: SessionConstructionPlan,
        requested_runtime_commands: &[super::runtime_commands::RuntimeCommandRecord],
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

        let mut final_capability_state = requested_runtime_commands
            .last()
            .map(|command| command.transition.state.clone())
            .unwrap_or_else(|| base_capability_state.clone());
        if let Some(base_vfs) = construction.surface.vfs.clone() {
            let effective_vfs = requested_runtime_commands
                .last()
                .and_then(|command| command.transition.state.vfs.active.as_ref())
                .map(|pending_vfs| {
                    super::capability_state::merge_vfs_overlay(base_vfs.clone(), pending_vfs)
                })
                .unwrap_or(base_vfs);
            construction.workspace.working_directory = effective_vfs
                .default_mount()
                .map(|mount| std::path::PathBuf::from(mount.root_ref.trim()))
                .filter(|path| !path.as_os_str().is_empty())
                .or(construction.workspace.working_directory);
            construction.surface.vfs = Some(effective_vfs.clone());
            final_capability_state.vfs.active = Some(effective_vfs);
        }
        if let Some(pending_mcp) = requested_runtime_commands
            .last()
            .map(|command| command.transition.state.tool.mcp_servers.clone())
        {
            construction.projections.mcp_servers = pending_mcp.clone();
            final_capability_state.tool.mcp_servers = pending_mcp;
        } else {
            final_capability_state.tool.mcp_servers = construction.projections.mcp_servers.clone();
        }
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

    /// 已完成 construction plan 准备后的执行段。生产入口只能从 `execute_command` 进入。
    async fn execute_constructed_launch(
        &self,
        session_id: &str,
        command: &LaunchCommand,
        construction: SessionConstructionPlan,
        turn_id: String,
        had_existing_runtime: bool,
        mut session_meta: SessionMeta,
        requested_runtime_commands: Vec<super::runtime_commands::RuntimeCommandRecord>,
    ) -> Result<String, ConnectorError> {
        let deps = &self.deps;
        let sid = session_id.to_string();
        let meta_store = deps.stores.meta.clone();
        let runtime_command_store = deps.stores.runtime_commands.clone();
        let now = chrono::Utc::now().timestamp_millis();
        let launch_execution = match SessionLaunchPlanner::new(deps.clone())
            .plan(SessionLaunchPlannerInput {
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
            Ok(launch_execution) => launch_execution,
            Err(error) => {
                deps.turn_supervisor.clear_turn_and_hook(session_id).await;
                return Err(error);
            }
        };
        let resolved_payload = launch_execution.resolved_payload.clone();
        let title_hint = launch_execution.title_hint.clone();
        let resolved_follow_up_session_id = launch_execution.summary.follow_up_session_id.clone();
        let post_turn_handler = launch_execution.terminal_effects.post_turn_handler.clone();
        let hook_session = launch_execution.context.turn.hook_session.clone();
        let hook_snapshot_contribution = launch_execution.hooks.snapshot_contribution.clone();
        let context_bundle = launch_execution.construction.context.bundle.clone();
        let discovered_guidelines = launch_execution.discovered_guidelines.clone();
        let base_capability_state = launch_execution
            .runtime_commands
            .base_capability_state
            .clone();
        let capability_state = launch_execution.context.turn.capability_state.clone();
        let capability_keys = capability_state.capability_keys();
        let is_owner_bootstrap =
            launch_execution.summary.hook_snapshot_reload == HookSnapshotReloadTrigger::Reload;
        tracing::debug!(
            session_id = %launch_execution.summary.session_id,
            turn_id = %launch_execution.summary.turn_id,
            lifecycle = ?launch_execution.summary.lifecycle,
            restore_mode = ?launch_execution.summary.restore_mode,
            follow_up_source = ?launch_execution.summary.follow_up_source,
            pending_transition_count = launch_execution.summary.pending_transition_count,
            vfs_source = ?launch_execution.summary.vfs_source,
            pending_vfs_overlay_applied = launch_execution.summary.pending_vfs_overlay_applied,
            mcp_source = ?launch_execution.summary.mcp_source,
            capability_source = ?launch_execution.summary.capability_source,
            mcp_server_count = launch_execution.summary.mcp_server_count,
            has_vfs = launch_execution.summary.has_vfs,
            "prepared session launch execution"
        );
        let mut context = launch_execution.context;

        // pipeline 层预构建工具列表：runtime + direct MCP + relay MCP
        context.turn.assembled_tools = deps
            .build_tools_for_execution_context(
                session_id,
                &context,
                &capability_state.tool.mcp_servers,
            )
            .await;

        let identity_frame = build_identity_context_frame(&IdentityFrameInput {
            base_system_prompt: &deps.base_system_prompt,
            agent_system_prompt: context.session.executor_config.system_prompt.as_deref(),
            agent_system_prompt_mode: context.session.executor_config.system_prompt_mode,
            user_preferences: &deps.user_preferences,
            discovered_guidelines: &discovered_guidelines,
        });

        let compose_fragments = context_bundle
            .as_ref()
            .map(|bundle| bundle.bootstrap_fragments.clone())
            .or_else(|| hook_snapshot_contribution.clone())
            .unwrap_or_default();
        let (audit_bundle_id, audit_session_id) = context_bundle
            .as_ref()
            .map(|bundle| (bundle.bundle_id, bundle.session_id))
            .unwrap_or_else(|| {
                let session_uuid = uuid::Uuid::parse_str(session_id).unwrap_or_else(|_| {
                    tracing::debug!(
                        session_id = %session_id,
                        "session_id 不是 UUID，使用临时审计 session_id"
                    );
                    uuid::Uuid::new_v4()
                });
                (uuid::Uuid::new_v4(), session_uuid)
            });

        {
            deps.turn_supervisor
                .activate_turn(
                    session_id,
                    super::hub_support::SessionProfile {
                        capability_state: capability_state.clone(),
                    },
                    TurnExecution::new(
                        turn_id.clone(),
                        context.session.clone(),
                        capability_state.clone(),
                        audit_bundle_id,
                        audit_session_id,
                    ),
                )
                .await;
        }

        let pending_command_ids = launch_execution
            .runtime_commands
            .requested_commands
            .iter()
            .map(|command| command.id)
            .collect::<Vec<_>>();
        let pending_transition_application = if !launch_execution
            .runtime_commands
            .pending_capability_transitions
            .is_empty()
        {
            deps.capability
                .apply_pending_runtime_context_transitions_on_turn(
                    &sid,
                    &turn_id,
                    hook_session.as_ref(),
                    base_capability_state,
                    &launch_execution
                        .runtime_commands
                        .pending_capability_transitions,
                    &context.turn.assembled_tools,
                )
                .await
        } else {
            Default::default()
        };

        let connector_type = match deps.connector.connector_type() {
            agentdash_spi::ConnectorType::LocalExecutor => "local_executor",
            agentdash_spi::ConnectorType::RemoteAcpBackend => "remote_acp_backend",
        };
        let source = SourceInfo {
            connector_id: deps.connector.connector_id().to_string(),
            connector_type: connector_type.to_string(),
            executor_id: Some(context.session.executor_config.executor.to_string()),
        };

        // PR 4（04-30-session-pipeline-architecture-refactor）删除 `session-capabilities://`
        // resource block 注入路径。动态能力面由独立 ContextFrame 投递，不再混入
        // user_blocks 或 core system prompt。
        let user_envelopes = build_user_message_envelopes(
            session_id,
            &source,
            &turn_id,
            &resolved_payload.user_blocks,
        );
        for envelope in user_envelopes {
            let _ = deps.eventing.persist_notification(&sid, envelope).await;
        }

        let started = build_turn_started_envelope(session_id, &source, &turn_id);
        let _ = deps.eventing.persist_notification(&sid, started).await;

        // SessionStart 只代表 owner 首轮 bootstrap，不再与“进程内第几轮”绑定。
        if is_owner_bootstrap {
            if let Some(hook_session) = hook_session.as_ref() {
                let initial_caps = capability_keys.clone();
                if !initial_caps.is_empty() {
                    let _ = hook_session.update_capabilities(initial_caps.clone());
                }

                let _start_effects = deps
                    .hooks
                    .emit_session_hook_trigger(
                        hook_session.as_ref(),
                        &HookTriggerInput {
                            session_id: &sid,
                            turn_id: Some(&turn_id),
                            trigger: HookTrigger::SessionStart,
                            payload: Some(serde_json::json!({
                                "text_prompt": resolved_payload.text_prompt,
                                "user_block_count": resolved_payload.user_blocks.len(),
                                "tool_capabilities": {
                                    "current": initial_caps.iter().collect::<Vec<_>>(),
                                },
                            })),
                            refresh_reason: "trigger:session_start",
                            source: source.clone(),
                        },
                    )
                    .await;
            }
        }

        let mut accepted_context_frames_to_emit = Vec::new();
        let mut owner_bootstrap_frames = Vec::new();
        if is_owner_bootstrap {
            let frame = build_initial_capability_state_frame(
                &capability_state,
                &capability_keys,
                &context.turn.assembled_tools,
            );
            accepted_context_frames_to_emit.push(frame.clone());
            owner_bootstrap_frames.push(frame);

            if let Some(frame) = build_assignment_context_frame(
                context_bundle
                    .as_ref()
                    .map(|bundle| bundle.phase_tag.as_str()),
                &compose_fragments,
            ) {
                accepted_context_frames_to_emit.push(frame.clone());
                owner_bootstrap_frames.push(frame);
            }
        }

        let mut turn_context_frames: Vec<ContextFrame> = Vec::new();
        if let Some(frame) = identity_frame {
            accepted_context_frames_to_emit.push(frame.clone());
            turn_context_frames.push(frame);
        }
        if let Some(frame) = launch_execution
            .construction
            .context
            .continuation_context_frame
            .clone()
        {
            accepted_context_frames_to_emit.push(frame.clone());
            turn_context_frames.push(frame);
        }
        turn_context_frames.extend(owner_bootstrap_frames);
        turn_context_frames.extend(pending_transition_application.context_frames.clone());

        if let Some(hook_session_runtime) = hook_session.as_ref() {
            turn_context_frames.extend(collect_queued_turn_start_frames(
                hook_session_runtime.as_ref(),
            ));

            let snapshot = hook_session_runtime.snapshot();
            let runtime = hook_session_runtime.runtime_snapshot();
            let pending_action_frames = hook_session_runtime
                .unresolved_pending_actions()
                .into_iter()
                .filter_map(|action| {
                    build_pending_action_context_frame(&snapshot, &action, &runtime)
                })
                .collect::<Vec<_>>();
            for frame in &pending_action_frames {
                accepted_context_frames_to_emit.push(frame.clone());
            }
            turn_context_frames.extend(pending_action_frames);
        }
        context.turn.context_frames = dedupe_context_frames(turn_context_frames);

        enqueue_context_frames_for_transform_context(
            hook_session.as_ref(),
            &context.turn.context_frames,
        );
        let executor_config_for_meta = context.session.executor_config.clone();

        let mut stream = match deps
            .connector
            .prompt(
                session_id,
                resolved_follow_up_session_id.as_deref(),
                &resolved_payload.prompt_payload,
                context,
            )
            .await
        {
            Ok(stream) => stream,
            Err(error) => {
                deps.turn_supervisor.clear_turn_and_hook(session_id).await;
                let failed = build_turn_terminal_envelope(
                    &sid,
                    &source,
                    &turn_id,
                    TurnTerminalKind::Failed,
                    Some(error.to_string()),
                );
                let _ = deps.eventing.persist_notification(&sid, failed).await;
                return Err(error);
            }
        };

        for payload in &pending_transition_application.capability_events {
            let _ = deps
                .eventing
                .emit_capability_state_changed(&sid, Some(&turn_id), payload.clone())
                .await;
        }
        for frame in &accepted_context_frames_to_emit {
            let _ = deps
                .eventing
                .emit_context_frame(&sid, Some(&turn_id), frame)
                .await;
        }

        Self::apply_turn_start_meta(
            &mut session_meta,
            now,
            &turn_id,
            &executor_config_for_meta,
            is_owner_bootstrap,
            &title_hint,
        );
        let _ = meta_store.save_session_meta(&session_meta).await;

        if let Err(error) =
            commit_runtime_commands_applied(&*runtime_command_store, &pending_command_ids, &sid)
                .await
        {
            deps.turn_supervisor.clear_turn_and_hook(session_id).await;
            let failed = build_turn_terminal_envelope(
                &sid,
                &source,
                &turn_id,
                TurnTerminalKind::Failed,
                Some(error.to_string()),
            );
            let _ = deps.eventing.persist_notification(&sid, failed).await;
            return Err(error);
        }

        // 首轮 prompt 且 title_source 非 User 时，异步触发标题生成。
        // connector.prompt 已接受后再触发，避免启动失败时产生额外副作用。
        let is_first_turn = session_meta.last_event_seq <= 1;
        if is_first_turn
            && session_meta.title_source != super::types::TitleSource::User
            && deps.title_generator.is_some()
        {
            deps.spawn_title_generation(
                session_id.to_string(),
                resolved_payload.text_prompt.clone(),
            );
        }
        let session_id = session_id.to_string();

        // 创建 SessionTurnProcessor — cloud-native 和 relay 共用的事件处理核心
        let processor = super::turn_processor::SessionTurnProcessor::spawn(
            super::turn_processor::SessionTurnProcessorDeps {
                turn_supervisor: deps.turn_supervisor.clone(),
                eventing: deps.eventing.clone(),
                effects: deps.effects.clone(),
            },
            super::turn_processor::SessionTurnProcessorConfig {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                source: source.clone(),
                hook_session,
                post_turn_handler,
            },
        );

        let processor_tx = processor.tx();

        // 注册 processor_tx 到 SessionRuntime.current_turn，供 relay / cancel 路径使用
        deps.turn_supervisor
            .register_processor_tx(&session_id, processor_tx.clone())
            .await;

        let stream_adapter = Self::spawn_stream_adapter(
            deps.turn_supervisor.clone(),
            session_id.to_string(),
            turn_id.clone(),
            &mut stream,
            processor_tx,
        );
        deps.turn_supervisor
            .register_stream_adapter_handle(&session_id, stream_adapter.abort_handle())
            .await;

        Ok(turn_id)
    }

    /// 将 connector stream 桥接到 processor channel 的后台任务。
    fn spawn_stream_adapter(
        turn_supervisor: super::turn_supervisor::TurnSupervisor,
        session_id: String,
        turn_id: String,
        stream: &mut agentdash_spi::ExecutionStream,
        processor_tx: tokio::sync::mpsc::UnboundedSender<super::turn_processor::TurnEvent>,
    ) -> tokio::task::JoinHandle<()> {
        use futures::StreamExt;
        let mut stream = std::mem::replace(stream, Box::pin(futures::stream::empty()));
        tokio::spawn(async move {
            while let Some(next) = stream.next().await {
                match next {
                    Ok(notification) => {
                        let _ = processor_tx.send(super::turn_processor::TurnEvent::Notification(
                            Box::new(notification),
                        ));
                    }
                    Err(e) => {
                        tracing::error!("执行流错误 session_id={}: {}", session_id, e);
                        let (kind, message) = Self::resolve_stream_terminal(
                            &turn_supervisor,
                            &session_id,
                            &turn_id,
                            Some(e),
                        )
                        .await;
                        let _ = processor_tx
                            .send(super::turn_processor::TurnEvent::Terminal { kind, message });
                        return;
                    }
                }
            }
            let (kind, message) =
                Self::resolve_stream_terminal(&turn_supervisor, &session_id, &turn_id, None).await;
            let _ = processor_tx.send(super::turn_processor::TurnEvent::Terminal { kind, message });
        })
    }

    /// 根据 cancel 状态和错误信息决定 stream 结束时的 terminal kind。
    async fn resolve_stream_terminal(
        turn_supervisor: &super::turn_supervisor::TurnSupervisor,
        session_id: &str,
        turn_id: &str,
        error: Option<agentdash_spi::ConnectorError>,
    ) -> (TurnTerminalKind, Option<String>) {
        if let Some(terminal) = turn_supervisor
            .cancel_interrupted_terminal(session_id, turn_id)
            .await
        {
            terminal
        } else if let Some(e) = error {
            (TurnTerminalKind::Failed, Some(e.to_string()))
        } else {
            (TurnTerminalKind::Completed, None)
        }
    }

    /// Turn 开始时更新 SessionMeta 的快照字段。
    ///
    /// 包含 bootstrap state 的 `Pending → Bootstrapped` 转移。
    fn apply_turn_start_meta(
        meta: &mut SessionMeta,
        now: i64,
        turn_id: &str,
        executor_config: &agentdash_domain::common::AgentConfig,
        is_owner_bootstrap: bool,
        title_hint: &str,
    ) {
        meta.updated_at = now;
        meta.last_execution_status = ExecutionStatus::Running;
        meta.last_turn_id = Some(turn_id.to_string());
        meta.last_terminal_message = None;
        meta.executor_config = Some(executor_config.clone());
        if is_owner_bootstrap {
            meta.bootstrap_state = SessionBootstrapState::Bootstrapped;
        }
        if meta.title.trim().is_empty() {
            meta.title = title_hint.to_string();
        }
    }
}

impl SessionRuntimeInner {
    /// 解析 hook runtime：决定 reload / refresh / skip。
    ///
    /// 三条路径：
    /// - **reload**：owner bootstrap 首轮 或 进程内没有 runtime（冷启动恢复）
    /// - **refresh**：已有 hook_session 且非 bootstrap → 只 refresh snapshot
    /// - **skip**：无 hook provider → 返回 None
    pub(super) async fn resolve_hook_session(
        &self,
        session_id: &str,
        turn_id: &str,
        executor_config: &agentdash_domain::common::AgentConfig,
        working_directory: &Path,
        is_owner_bootstrap: bool,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        let existing = self.runtime_registry.hook_session_runtime(session_id).await;

        if is_owner_bootstrap || existing.is_none() {
            return self
                .reload_session_hook_runtime(
                    session_id,
                    turn_id,
                    executor_config.executor.as_str(),
                    executor_config.permission_policy.as_deref(),
                    working_directory,
                )
                .await;
        }

        if let Some(ref hs) = existing {
            let _ = hs
                .refresh(agentdash_spi::hooks::SessionHookRefreshQuery {
                    session_id: session_id.to_string(),
                    turn_id: Some(turn_id.to_string()),
                    reason: Some("subsequent_turn_refresh".to_string()),
                })
                .await;
        }
        Ok(existing)
    }

    /// 重载 session hook runtime 并写入 `SessionRuntime.hook_session` 单一权威字段。
    ///
    /// PR 7c：只有本函数（owner bootstrap 入口）以及 `ensure_hook_session_runtime`
    /// （冷启动恢复入口）可以写 `SessionRuntime.hook_session`；其他调用方（包括
    /// `SessionLaunchExecutor` 的 happy path）只读取不回写。
    pub async fn reload_session_hook_runtime(
        &self,
        session_id: &str,
        turn_id: &str,
        executor: &str,
        permission_policy: Option<&str>,
        working_directory: &Path,
    ) -> Result<Option<SharedHookSessionRuntime>, ConnectorError> {
        let Some(provider) = self.hook_provider.as_ref() else {
            // 无 hook provider 场景下清空 runtime 记录，保证"单一权威"不滞留旧值。
            self.runtime_registry
                .with_runtime_mut(session_id, |runtime| {
                    if let Some(runtime) = runtime {
                        runtime.hook_session = None;
                    }
                })
                .await;
            return Ok(None);
        };

        let mut snapshot = provider
            .load_session_snapshot(SessionHookSnapshotQuery {
                session_id: session_id.to_string(),
                turn_id: Some(turn_id.to_string()),
            })
            .await
            .map_err(|error| {
                ConnectorError::Runtime(format!("加载会话 Hook snapshot 失败: {error}"))
            })?;
        enrich_hook_snapshot_runtime_metadata(
            &mut snapshot,
            turn_id,
            self.connector.connector_id(),
            executor,
            permission_policy,
            working_directory,
        );

        let runtime = Arc::new(HookSessionRuntime::new(
            session_id.to_string(),
            provider.clone(),
            snapshot,
        ));

        // 写回 SessionRuntime.hook_session —— 单一权威字段仅在此处写入。
        self.runtime_registry
            .with_runtime_mut(session_id, |session_runtime| {
                if let Some(session_runtime) = session_runtime {
                    session_runtime.hook_session = Some(runtime.clone());
                }
            })
            .await;

        Ok(Some(runtime))
    }
}

fn enrich_hook_snapshot_runtime_metadata(
    snapshot: &mut SessionHookSnapshot,
    turn_id: &str,
    connector_id: &str,
    executor: &str,
    permission_policy: Option<&str>,
    working_directory: &Path,
) {
    let metadata = snapshot
        .metadata
        .get_or_insert_with(agentdash_spi::SessionSnapshotMetadata::default);
    metadata.turn_id = Some(turn_id.to_string());
    metadata.connector_id = Some(connector_id.to_string());
    metadata.executor = Some(executor.to_string());
    metadata.permission_policy = permission_policy.map(ToString::to_string);
    metadata.working_directory = Some(working_directory.to_string_lossy().replace('\\', "/"));
}

fn collect_queued_turn_start_frames(
    hook_session: &dyn agentdash_spi::hooks::HookSessionRuntimeAccess,
) -> Vec<ContextFrame> {
    hook_session
        .collect_turn_start_notices_for_injection()
        .into_iter()
        .filter_map(notice_to_context_frame)
        .collect()
}

fn notice_to_context_frame(notice: HookTurnStartNotice) -> Option<ContextFrame> {
    if let Some(frame) = notice.context_frame {
        return Some(frame);
    }
    let content = notice.content.trim();
    if content.is_empty() {
        return None;
    }
    Some(ContextFrame {
        id: notice.id,
        kind: "system_notice".to_string(),
        source: notice.source,
        phase_node: None,
        apply_mode: None,
        delivery_status: "queued_for_transform_context".to_string(),
        delivery_channel: "turn_start".to_string(),
        message_role: "user".to_string(),
        rendered_text: content.to_string(),
        sections: vec![ContextFrameSection::SystemNotice {
            title: "TurnStart Notice".to_string(),
            summary: "TurnStart notice 已桥接为 ContextFrame。".to_string(),
            body: Some(content.to_string()),
        }],
        created_at_ms: notice.created_at_ms,
    })
}

fn dedupe_context_frames(frames: Vec<ContextFrame>) -> Vec<ContextFrame> {
    let mut ids = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for frame in frames {
        if frame.rendered_text.trim().is_empty() {
            continue;
        }
        if ids.insert(frame.id.clone()) {
            deduped.push(frame);
        }
    }
    deduped
}

/// 统一将已组装的 ContextFrame 投递到 Hook session transform_context 队列。
///
/// 排除规则（与原 executor 层行为一致）：
/// - `identity` frame 走 connector 的 set_system_prompt，不进入 transform_context；
/// - `pending_action` frame 不参与 transform_context（由独立通道投递）。
fn enqueue_context_frames_for_transform_context(
    hook_session: Option<&SharedHookSessionRuntime>,
    frames: &[ContextFrame],
) {
    let Some(hook_session) = hook_session else {
        return;
    };
    for frame in frames {
        if frame.kind == "identity" || frame.kind == "pending_action" {
            continue;
        }
        if frame.rendered_text.trim().is_empty() {
            continue;
        }
        hook_session.enqueue_turn_start_notice(HookTurnStartNotice {
            id: frame.id.clone(),
            created_at_ms: frame.created_at_ms,
            source: frame.source.clone(),
            content: frame.rendered_text.clone(),
            context_frame: Some(frame.clone()),
        });
    }
}

async fn commit_runtime_commands_applied(
    runtime_command_store: &dyn SessionRuntimeCommandStore,
    pending_command_ids: &[uuid::Uuid],
    session_id: &str,
) -> Result<(), ConnectorError> {
    if pending_command_ids.is_empty() {
        return Ok(());
    }
    if let Err(error) = runtime_command_store
        .mark_runtime_commands_applied(pending_command_ids)
        .await
    {
        let error_message = error.to_string();
        tracing::error!(
            session_id = %session_id,
            error = %error_message,
            "标记 requested runtime commands applied 失败，改写为 failed 以避免下一轮重复应用"
        );
        if let Err(failed_error) = runtime_command_store
            .mark_runtime_commands_failed(pending_command_ids, error_message.clone())
            .await
        {
            return Err(ConnectorError::Runtime(format!(
                "runtime command applied/failed 状态提交均失败: applied_error={error_message}; failed_error={failed_error}"
            )));
        }
        return Err(ConnectorError::Runtime(format!(
            "runtime command applied 状态提交失败，已标记 failed: {error_message}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::baseline_capabilities::build_session_baseline_capabilities;
    use super::super::runtime_commands::{RuntimeCommandRecord, RuntimeCommandStatus};
    use super::super::types::PendingCapabilityStateTransition;
    use super::*;
    use async_trait::async_trait;
    use std::io;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

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

    #[tokio::test]
    async fn runtime_command_apply_commit_failure_marks_failed_and_returns_error() {
        let store = FailingApplyRuntimeCommandStore::default();
        let command_id = Uuid::new_v4();

        let err =
            commit_runtime_commands_applied(&store, &[command_id], "sess-runtime-command-fails")
                .await
                .expect_err("applied failure should fail launch");

        assert!(matches!(err, ConnectorError::Runtime(_)));
        assert_eq!(store.apply_calls.load(Ordering::SeqCst), 1);
        assert_eq!(store.failed_calls.load(Ordering::SeqCst), 1);
    }

    #[derive(Default)]
    struct FailingApplyRuntimeCommandStore {
        apply_calls: AtomicUsize,
        failed_calls: AtomicUsize,
    }

    #[async_trait]
    impl SessionRuntimeCommandStore for FailingApplyRuntimeCommandStore {
        async fn upsert_runtime_command_request(
            &self,
            _session_id: &str,
            _transition: PendingCapabilityStateTransition,
        ) -> io::Result<RuntimeCommandRecord> {
            Err(io::Error::other("not used"))
        }

        async fn list_requested_runtime_commands(
            &self,
            _session_id: &str,
        ) -> io::Result<Vec<RuntimeCommandRecord>> {
            Ok(Vec::new())
        }

        async fn mark_runtime_commands_applied(&self, _command_ids: &[Uuid]) -> io::Result<()> {
            self.apply_calls.fetch_add(1, Ordering::SeqCst);
            Err(io::Error::other("forced applied commit failure"))
        }

        async fn mark_runtime_commands_failed(
            &self,
            _command_ids: &[Uuid],
            _error: String,
        ) -> io::Result<()> {
            self.failed_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn list_runtime_commands_by_status(
            &self,
            _statuses: &[RuntimeCommandStatus],
            _limit: u32,
        ) -> io::Result<Vec<RuntimeCommandRecord>> {
            Ok(Vec::new())
        }
    }
}
