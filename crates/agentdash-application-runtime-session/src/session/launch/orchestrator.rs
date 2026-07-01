use agentdash_application_ports::frame_launch_envelope::{
    FrameLaunchEnvelope, FrameLaunchEnvelopeRequest, RuntimeTraceLaunchStateRef,
};
use agentdash_application_ports::launch::{LaunchCommand, LaunchPlanningInput};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_spi::ConnectorError;

use crate::backend_execution_placement::ExecutionPlacementPlan;
use crate::session::launch::{
    ConnectorStarter, LaunchCommandOutcome, LaunchPlanner, LaunchPlannerInput, SessionLaunchDeps,
    StreamIngestionAttacher, TurnCommitter, TurnPreparationInput, TurnPreparer,
};
use crate::session::runtime_commands::RuntimeCommandRecord;
use crate::session::types::*;
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
        planning_input: LaunchPlanningInput,
    ) -> Result<LaunchCommandOutcome, ConnectorError> {
        let reason = command.reason_tag();
        let Some(provider) = self.deps.current_frame_launch_envelope_provider().await else {
            return Err(ConnectorError::Runtime(format!(
                "session_launch_envelope_provider 未注入，拒绝 session launch: {reason}"
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
        let accepted_launch_commit = self.deps.current_accepted_launch_commit_port().await;
        let agent_needs_bootstrap_early = accepted_launch_commit.agent_needs_bootstrap(&sid).await;
        let runtime_trace_state = RuntimeTraceLaunchState::from(&session_meta);
        let launch_envelope = match provider
            .build_launch_envelope(FrameLaunchEnvelopeRequest {
                runtime_session_id: sid.clone(),
                command: command.clone(),
                runtime_trace_state: RuntimeTraceLaunchStateRef {
                    executor_session_id: runtime_trace_state.executor_session_id.clone(),
                    last_event_seq: runtime_trace_state.last_event_seq,
                },
                had_existing_runtime,
                requested_runtime_commands: requested_runtime_commands.clone(),
                agent_needs_bootstrap: agent_needs_bootstrap_early,
            })
            .await
        {
            Ok(envelope) => envelope,
            Err(error) => {
                self.deps
                    .turn_supervisor
                    .clear_turn_and_hook(session_id)
                    .await;
                return Err(error);
            }
        };
        let context_sources = launch_envelope
            .context
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
            .launch_with_envelope(
                session_id,
                &command,
                &planning_input,
                launch_envelope,
                facts,
            )
            .await?;
        Ok(LaunchCommandOutcome {
            turn_id,
            context_sources,
        })
    }

    /// 已完成 frame construction 后的内部 stage runner。生产入口只能从 `launch` 进入。
    async fn launch_with_envelope(
        &self,
        session_id: &str,
        command: &LaunchCommand,
        planning_input: &LaunchPlanningInput,
        launch_envelope: FrameLaunchEnvelope,
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
        let accepted_launch_commit = deps.current_accepted_launch_commit_port().await;
        let agent_needs_bootstrap = accepted_launch_commit
            .agent_needs_bootstrap(session_id)
            .await;
        let launch_plan = match LaunchPlanner::new(deps.planning())
            .plan(LaunchPlannerInput {
                session_id,
                turn_id: &turn_id,
                command,
                planning_input: planning_input.clone(),
                had_existing_runtime,
                runtime_trace_state: RuntimeTraceLaunchState::from(&session_meta),
                requested_runtime_commands,
                launch_envelope,
                agent_needs_bootstrap,
            })
            .await
        {
            Ok(launch_plan) => launch_plan,
            Err(error) => {
                deps.turn_supervisor.clear_turn_and_hook(session_id).await;
                return Err(error);
            }
        };
        let backend_execution = launch_plan.backend_execution.clone();
        diag!(
            Debug,
            Subsystem::SessionLaunch,
            session_id,
            turn_id,
            "session launch preparing turn"
        );
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
        diag!(
            Debug,
            Subsystem::SessionLaunch,
            session_id,
            turn_id,
            "session launch starting connector"
        );
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
        diag!(
            Debug,
            Subsystem::SessionLaunch,
            session_id,
            turn_id,
            "session launch connector accepted"
        );
        let committed = TurnCommitter::new(deps.commit(accepted_launch_commit.clone()))
            .commit(accepted, &mut session_meta, now)
            .await?;
        diag!(
            Debug,
            Subsystem::SessionLaunch,
            session_id,
            turn_id,
            "session launch committed turn"
        );

        if committed.accepted.prepared.is_owner_bootstrap {
            accepted_launch_commit
                .mark_agent_bootstrapped(session_id)
                .await;
        }

        let attached = StreamIngestionAttacher::new(deps.ingestion())
            .attach(committed)
            .await;
        diag!(Debug, Subsystem::SessionLaunch,

            session_id,
            turn_id = %attached.turn_id,
            "session launch stream ingestion attached"
        );

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
        diag!(Warn, Subsystem::SessionLaunch,

            lease_id = %lease_id,
            error = %error,
            "标记 backend execution lease failed 失败"
        );
    }
}
