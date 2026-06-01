use crate::backend_execution_placement::ExecutionPlacementPlan;
use crate::session::construction_provider::SessionConstructionProviderInput;
use crate::session::launch::{
    ConnectorStarter, LaunchCommand, LaunchCommandOutcome, LaunchPlanner, LaunchPlannerInput,
    SessionLaunchDeps, StreamIngestionAttacher, TurnCommitter, TurnPreparationInput, TurnPreparer,
};
use crate::session::runtime_commands::RuntimeCommandRecord;
use crate::session::types::*;
use crate::workflow::AgentFrameBuilder;
use crate::workflow::runtime_launch::RuntimeLaunchRequest;
use agentdash_spi::ConnectorError;

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
        let launch_request = match provider
            .build_frame_construction(SessionConstructionProviderInput {
                session_id: sid.clone(),
                command: command.clone(),
                session_meta: session_meta.clone(),
                had_existing_runtime,
                requested_runtime_commands: requested_runtime_commands.clone(),
            })
            .await
        {
            Ok(req) => req,
            Err(error) => {
                self.deps
                    .turn_supervisor
                    .clear_turn_and_hook(session_id)
                    .await;
                return Err(error);
            }
        };
        let context_sources = launch_request
            .context_bundle
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
            .launch_with_request(session_id, &command, launch_request, facts)
            .await?;
        Ok(LaunchCommandOutcome {
            turn_id,
            context_sources,
        })
    }

    #[cfg(test)]
    pub(crate) async fn launch_with_request_for_test(
        &self,
        session_id: &str,
        launch_request: RuntimeLaunchRequest,
    ) -> Result<String, ConnectorError> {
        let user_input = UserPromptInput {
            prompt_blocks: launch_request.prompt_blocks.clone(),
            env: launch_request.environment_variables.clone(),
            executor_config: launch_request.executor_config.clone(),
            backend_selection: None,
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
        let launch_request =
            Self::finalize_request_for_test(launch_request, &requested_runtime_commands);
        let facts = LaunchRuntimeFacts {
            turn_id,
            had_existing_runtime,
            session_meta,
            requested_runtime_commands,
            context_sources: Vec::new(),
        };
        self.launch_with_request(session_id, &command, launch_request, facts)
            .await
    }

    #[cfg(test)]
    fn finalize_request_for_test(
        mut req: RuntimeLaunchRequest,
        requested_runtime_commands: &[crate::session::runtime_commands::RuntimeCommandRecord],
    ) -> RuntimeLaunchRequest {
        use crate::workflow::runtime_launch::LaunchResolutionTrace;

        let mut base_capability_state = req.typed_capability_state.clone().unwrap_or_default();
        if let Some(ref vfs) = req.typed_vfs {
            base_capability_state.vfs.active = Some(vfs.clone());
        }
        base_capability_state.tool.mcp_servers = req.typed_mcp_servers.clone();

        let requested_transitions = requested_runtime_commands
            .iter()
            .map(|command| command.pending_capability_state_transition())
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
        if let Some(base_vfs) = req.typed_vfs.clone() {
            let effective_vfs = replay
                .as_ref()
                .and_then(|replay| replay.effective_vfs.clone())
                .unwrap_or(base_vfs);
            req.working_directory = effective_vfs
                .default_mount()
                .map(|mount| std::path::PathBuf::from(mount.root_ref.trim()))
                .filter(|path| !path.as_os_str().is_empty())
                .or(req.working_directory);
            final_capability_state.vfs.active = Some(effective_vfs.clone());
            req.typed_vfs = Some(effective_vfs);
        }
        let effective_mcp_servers = replay
            .as_ref()
            .and_then(|replay| replay.effective_mcp_servers.clone())
            .unwrap_or_else(|| req.typed_mcp_servers.clone());
        req.typed_mcp_servers = effective_mcp_servers.clone();
        final_capability_state.tool.mcp_servers = effective_mcp_servers;
        req.typed_capability_state = Some(final_capability_state);
        req.base_capability_state = Some(base_capability_state);
        if requested_runtime_commands.is_empty() {
            req.resolution_trace.pending_overlay_applied = false;
        } else {
            req.resolution_trace = LaunchResolutionTrace {
                vfs_source: Some("test.pending_runtime_command".to_string()),
                mcp_source: Some("test.pending_runtime_command".to_string()),
                capability_source: Some("test.pending_runtime_command".to_string()),
                pending_overlay_applied: true,
            };
        }
        req
    }

    /// 已完成 frame construction 后的内部 stage runner。生产入口只能从 `launch` 进入。
    async fn launch_with_request(
        &self,
        session_id: &str,
        command: &LaunchCommand,
        launch_request: RuntimeLaunchRequest,
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
        let launch_plan = match LaunchPlanner::new(deps.planning())
            .plan(LaunchPlannerInput {
                session_id,
                turn_id: &turn_id,
                command,
                had_existing_runtime,
                session_meta: &session_meta,
                requested_runtime_commands,
                launch_request,
            })
            .await
        {
            Ok(launch_plan) => launch_plan,
            Err(error) => {
                deps.turn_supervisor.clear_turn_and_hook(session_id).await;
                return Err(error);
            }
        };
        // === 初始 capability state 写入 AgentFrame revision ===
        if let Some(ref frame_repo) = deps.agent_frame_repo {
            let initial_cap_state = &launch_plan.context.turn.capability_state;
            match frame_repo.find_by_runtime_session(session_id).await {
                Ok(Some(current_frame)) => {
                    let mut builder = AgentFrameBuilder::new(current_frame.agent_id)
                        .with_capability_state(initial_cap_state)
                        .with_created_by("session_launch", Some(session_id.to_string()));
                    if let Some(ctx) = current_frame.context_slice_json {
                        builder = builder.with_context(ctx);
                    }
                    if let Some(profile) = current_frame.execution_profile_json {
                        builder = builder.with_execution_profile_raw(profile);
                    }
                    match builder.build(frame_repo.as_ref()).await {
                        Ok(frame) => {
                            tracing::debug!(
                                session_id,
                                agent_id = %frame.agent_id,
                                revision = frame.revision,
                                "初始 capability state 已写入 AgentFrame"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(
                                session_id,
                                "初始 AgentFrame revision 写入失败: {error}"
                            );
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(session_id, "查找 session 关联的 AgentFrame 失败: {error}");
                }
            }
        }

        let backend_execution = launch_plan.backend_execution.clone();
        let prepared = match TurnPreparer::new(deps.preparation())
            .prepare(TurnPreparationInput {
                launch_plan,
                session_id: sid.clone(),
                turn_id: turn_id.clone(),
                had_existing_runtime,
            })
            .await
        {
            Ok(prepared) => prepared,
            Err(error) => {
                fail_claimed_backend_execution(
                    deps,
                    backend_execution.as_ref(),
                    format!("turn preparation failed: {error}"),
                )
                .await;
                return Err(error);
            }
        };
        let accepted = match ConnectorStarter::new(deps.connector_start())
            .start(prepared)
            .await
        {
            Ok(accepted) => accepted,
            Err(error) => {
                fail_claimed_backend_execution(
                    deps,
                    backend_execution.as_ref(),
                    format!("connector start failed: {error}"),
                )
                .await;
                return Err(error);
            }
        };
        let committed = TurnCommitter::new(deps.commit())
            .commit(accepted, &mut session_meta, now)
            .await?;
        let attached = StreamIngestionAttacher::new(deps.ingestion())
            .attach(committed)
            .await;

        Ok(attached.turn_id)
    }
}

async fn fail_claimed_backend_execution(
    deps: &SessionLaunchDeps,
    placement: Option<&ExecutionPlacementPlan>,
    reason: String,
) {
    let Some(lease_id) = placement.and_then(|placement| placement.lease_id) else {
        return;
    };
    let Some(repo) = deps.backend_execution_lease_repo.as_ref() else {
        return;
    };
    if let Err(error) = repo.fail(lease_id, Some(reason), chrono::Utc::now()).await {
        tracing::warn!(
            lease_id = %lease_id,
            error = %error,
            "标记 backend execution lease failed 失败"
        );
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
