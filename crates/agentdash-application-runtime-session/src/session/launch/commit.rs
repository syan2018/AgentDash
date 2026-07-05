use agentdash_agent_protocol::{BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo};
use agentdash_application_ports::frame_launch_envelope::AcceptedLaunchCommitInput;
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag, diag_error};
use agentdash_spi::{ConnectorError, McpServerReadinessSummary};
use std::fmt::Display;

use super::connector_start::ConnectorAcceptedTurn;
use super::deps::TurnCommitDeps;
use super::preparation::PreparedTurn;
use crate::session::hub_support::{
    TurnTerminalKind, build_turn_started_envelope, build_user_input_submitted_envelope,
};
use crate::session::persistence::SessionRuntimeCommandStore;
use crate::session::turn_processor::{
    SessionTurnProcessorDeps, TurnTerminalDispatch, process_turn_terminal,
};
use crate::session::types::{ExecutionStatus, ResolvedPromptPayload, SessionMeta, TitleSource};

/// Accepted-after-commit boundary: connector accepted 后的 user/start/context/runtime
/// facts 与 AgentRun accepted 控制面提交均已成功。
pub(in crate::session) struct CommittedTurn {
    pub accepted: ConnectorAcceptedTurn,
}

pub(in crate::session) struct TurnCommitter {
    deps: TurnCommitDeps,
}

impl TurnCommitter {
    pub(super) fn new(deps: TurnCommitDeps) -> Self {
        Self { deps }
    }

    pub async fn commit(
        &self,
        accepted: ConnectorAcceptedTurn,
        session_meta: &mut SessionMeta,
        now: i64,
    ) -> Result<CommittedTurn, ConnectorError> {
        let prepared = &accepted.prepared;
        let session_id = prepared.session_id.as_str();
        let turn_id = prepared.turn_id.as_str();

        if let Err(error) = self
            .commit_accepted_launch_events(
                session_id,
                &prepared.source,
                turn_id,
                &prepared.resolved_payload,
                prepared.started_at_ms,
            )
            .await
        {
            return Err(self
                .fail_accepted_boundary(prepared, "accepted launch events commit", error)
                .await);
        }

        self.commit_mcp_readiness_notice(
            session_id,
            &prepared.source,
            &prepared.mcp_readiness_notice,
        )
        .await;

        if let Some(record) = prepared.context_delivery_record.as_ref()
            && let Err(error) = self
                .deps
                .eventing
                .emit_context_delivery_record(session_id, Some(turn_id), record)
                .await
                .map(|_| ())
                .map_err(|error| connector_commit_error("ContextDeliveryRecord 提交失败", error))
        {
            return Err(self
                .fail_accepted_boundary(prepared, "context delivery record commit", error)
                .await);
        }

        for frame in &prepared.pending_transition_application.context_frames {
            if let Err(error) = self
                .deps
                .eventing
                .emit_context_frame(session_id, Some(turn_id), frame)
                .await
                .map(|_| ())
                .map_err(|error| connector_commit_error("pending context_frame 提交失败", error))
            {
                return Err(self
                    .fail_accepted_boundary(prepared, "pending context frame commit", error)
                    .await);
            }
        }
        for frame in &prepared.accepted_context_frames_to_emit {
            if let Err(error) = self
                .deps
                .eventing
                .emit_context_frame(session_id, Some(turn_id), frame)
                .await
                .map(|_| ())
                .map_err(|error| connector_commit_error("accepted context_frame 提交失败", error))
            {
                return Err(self
                    .fail_accepted_boundary(prepared, "accepted context frame commit", error)
                    .await);
            }
        }

        apply_turn_start_meta(
            session_meta,
            now,
            turn_id,
            prepared.is_owner_bootstrap,
            &prepared.title_hint,
        );
        if let Err(error) = self
            .deps
            .stores
            .meta
            .save_session_meta(session_meta)
            .await
            .map_err(|error| connector_commit_error("session meta accepted 状态提交失败", error))
        {
            return Err(self
                .fail_accepted_boundary(prepared, "session meta commit", error)
                .await);
        }

        if let Err(error) = commit_runtime_commands_applied(
            &*self.deps.stores.runtime_commands,
            &prepared.pending_command_ids,
            session_id,
        )
        .await
        {
            return Err(self
                .fail_accepted_boundary(prepared, "runtime commands applied commit", error)
                .await);
        }

        let is_first_turn = session_meta.last_event_seq <= 1;
        if is_first_turn
            && session_meta.title_source == TitleSource::Auto
            && !self.deps.eventing.supports_source_session_title()
        {
            self.deps
                .apply_auto_title(session_id, &prepared.resolved_payload.text_prompt)
                .await;
        }

        let outcome = match self
            .deps
            .accepted_launch_commit
            .commit_accepted_launch(AcceptedLaunchCommitInput {
                runtime_session_id: session_id.to_string(),
                turn_id: prepared.turn_id.clone(),
                pending_frame: prepared.pending_frame.clone(),
                accepted_capability_state: prepared.accepted_capability_state.clone(),
            })
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(self
                    .fail_accepted_boundary(prepared, "AgentRun accepted launch commit", error)
                    .await);
            }
        };
        for diagnostic in outcome.diagnostics {
            diag!(
                Warn,
                Subsystem::SessionLaunch,
                operation = "session.launch.commit",
                stage = "accepted_launch_control_plane_diagnostic",
                session_id = %session_id,
                turn_id = %turn_id,
                diagnostic = %diagnostic,
                "accepted launch control-plane commit diagnostic"
            );
        }

        Ok(CommittedTurn { accepted })
    }

    async fn commit_accepted_launch_events(
        &self,
        session_id: &str,
        source: &SourceInfo,
        turn_id: &str,
        resolved_payload: &ResolvedPromptPayload,
        started_at_ms: i64,
    ) -> Result<(), ConnectorError> {
        // 直接使用 resolve 阶段已转换好的 canonical 输入，不再二次 round-trip ContentBlock。
        if !resolved_payload.input.is_empty() {
            let envelope = build_user_input_submitted_envelope(
                session_id,
                source,
                turn_id,
                &format!("{turn_id}:user-input:0"),
                agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                resolved_payload.input.clone(),
            );
            self.deps
                .eventing
                .persist_notification(session_id, envelope)
                .await
                .map_err(|error| connector_commit_error("user_input_submitted 提交失败", error))?;
        }

        let started = build_turn_started_envelope(session_id, source, turn_id, started_at_ms);
        self.deps
            .eventing
            .persist_notification(session_id, started)
            .await
            .map_err(|error| connector_commit_error("turn_started 提交失败", error))?;
        Ok(())
    }

    async fn commit_mcp_readiness_notice(
        &self,
        session_id: &str,
        source: &SourceInfo,
        unavailable_sources: &[McpServerReadinessSummary],
    ) {
        if unavailable_sources.is_empty() {
            return;
        }
        let source_names = unavailable_sources
            .iter()
            .map(|source| format!("{} ({})", source.name, source.reason_code))
            .collect::<Vec<_>>()
            .join("；");
        let envelope = BackboneEnvelope::new(
            BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                key: "system_message".to_string(),
                value: serde_json::json!({
                    "message": format!("MCP 工具源不可用，相关工具本轮不会暴露：{source_names}"),
                    "kind": "mcp_source_readiness",
                    "sources": unavailable_sources,
                }),
            }),
            session_id,
            source.clone(),
        );
        let _ = self
            .deps
            .eventing
            .persist_notification(session_id, envelope)
            .await;
    }

    async fn fail_accepted_boundary(
        &self,
        prepared: &PreparedTurn,
        stage: &'static str,
        error: ConnectorError,
    ) -> ConnectorError {
        let error_message = error.to_string();
        process_turn_terminal(
            &SessionTurnProcessorDeps {
                turn_supervisor: self.deps.turn_supervisor.clone(),
                eventing: self.deps.eventing.clone(),
                effects: self.deps.effects.clone(),
            },
            TurnTerminalDispatch {
                session_id: prepared.session_id.clone(),
                turn_id: prepared.turn_id.clone(),
                source: prepared.source.clone(),
                terminal_kind: TurnTerminalKind::Failed,
                terminal_message: Some(error_message.clone()),
                hook_runtime: prepared.hook_runtime.clone(),
                post_turn_handler: prepared.post_turn_handler.clone(),
            },
        )
        .await;
        diag!(
            Warn,
            Subsystem::SessionLaunch,
            session_id = %prepared.session_id,
            turn_id = %prepared.turn_id,
            stage = stage,
            accepted_boundary_error = %error_message,
            "accepted boundary 失败后已走统一 terminal 收口"
        );
        error
    }
}

fn connector_commit_error(stage: &str, error: impl Display) -> ConnectorError {
    ConnectorError::Runtime(format!("{stage}: {error}"))
}

fn apply_turn_start_meta(
    meta: &mut SessionMeta,
    now: i64,
    turn_id: &str,
    _is_owner_bootstrap: bool,
    title_hint: &str,
) {
    meta.updated_at = now;
    meta.last_delivery_status = ExecutionStatus::Running;
    meta.last_turn_id = Some(turn_id.to_string());
    meta.last_terminal_message = None;
    if meta.title.trim().is_empty() {
        meta.title = title_hint.to_string();
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
        let context =
            DiagnosticErrorContext::new("session.launch.commit_runtime_commands", "mark_applied");
        diag_error!(
            Error,
            Subsystem::SessionLaunch,
            context = &context,
            error = &error,
            session_id = %session_id,
            pending_command_count = pending_command_ids.len(),
            "标记 requested runtime commands applied 失败，改写为 failed 以避免下一轮重复应用"
        );
        if let Err(failed_error) = runtime_command_store
            .mark_runtime_commands_failed(pending_command_ids, error_message.clone())
            .await
        {
            let context = DiagnosticErrorContext::new(
                "session.launch.commit_runtime_commands",
                "mark_failed_after_applied_error",
            );
            diag_error!(
                Error,
                Subsystem::SessionLaunch,
                context = &context,
                error = &failed_error,
                session_id = %session_id,
                pending_command_count = pending_command_ids.len(),
                applied_error = %error_message,
                "标记 requested runtime commands failed 也失败"
            );
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
    use super::*;
    use crate::session::persistence::{SessionStoreError, SessionStoreResult};
    use crate::session::runtime_commands::{
        AgentFrameTransitionRecord, RuntimeCommandRecord, RuntimeCommandStatus,
        RuntimeDeliveryCommand,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

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
        async fn upsert_runtime_delivery_command(
            &self,
            _delivery_runtime_session_id: &str,
            _delivery: RuntimeDeliveryCommand,
            _frame_transition: AgentFrameTransitionRecord,
        ) -> SessionStoreResult<RuntimeCommandRecord> {
            Err(SessionStoreError::Internal("not used".to_string()))
        }

        async fn list_requested_runtime_commands(
            &self,
            _session_id: &str,
        ) -> SessionStoreResult<Vec<RuntimeCommandRecord>> {
            Ok(Vec::new())
        }

        async fn mark_runtime_commands_applied(
            &self,
            _command_ids: &[Uuid],
        ) -> SessionStoreResult<()> {
            self.apply_calls.fetch_add(1, Ordering::SeqCst);
            Err(SessionStoreError::Internal(
                "forced applied commit failure".to_string(),
            ))
        }

        async fn mark_runtime_commands_failed(
            &self,
            _command_ids: &[Uuid],
            _error: String,
        ) -> SessionStoreResult<()> {
            self.failed_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn list_runtime_commands_by_status(
            &self,
            _statuses: &[RuntimeCommandStatus],
            _limit: u32,
        ) -> SessionStoreResult<Vec<RuntimeCommandRecord>> {
            Ok(Vec::new())
        }
    }
}
