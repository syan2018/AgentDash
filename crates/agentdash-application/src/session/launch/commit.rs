use agentdash_spi::ConnectorError;

use super::SessionLaunchDeps;
use super::connector_start::ConnectorAcceptedTurn;
use crate::session::hub_support::{
    TurnTerminalKind, build_turn_started_envelope, build_turn_terminal_envelope,
    build_user_message_envelopes,
};
use crate::session::persistence::SessionRuntimeCommandStore;
use crate::session::types::{
    ExecutionStatus, ResolvedPromptPayload, SessionBootstrapState, SessionMeta, TitleSource,
};

pub(in crate::session) struct CommittedTurn {
    pub accepted: ConnectorAcceptedTurn,
}

pub(in crate::session) struct TurnCommitter {
    deps: SessionLaunchDeps,
}

impl TurnCommitter {
    pub fn new(deps: SessionLaunchDeps) -> Self {
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

        self.commit_accepted_launch_events(
            session_id,
            &prepared.source,
            turn_id,
            &prepared.resolved_payload,
        )
        .await;

        for payload in &prepared.pending_transition_application.capability_events {
            let _ = self
                .deps
                .eventing
                .emit_capability_state_changed(session_id, Some(turn_id), payload.clone())
                .await;
        }
        for frame in &prepared.accepted_context_frames_to_emit {
            let _ = self
                .deps
                .eventing
                .emit_context_frame(session_id, Some(turn_id), frame)
                .await;
        }

        apply_turn_start_meta(
            session_meta,
            now,
            turn_id,
            &prepared.executor_config_for_meta,
            prepared.is_owner_bootstrap,
            &prepared.title_hint,
        );
        let _ = self.deps.stores.meta.save_session_meta(session_meta).await;

        if let Err(error) = commit_runtime_commands_applied(
            &*self.deps.stores.runtime_commands,
            &prepared.pending_command_ids,
            session_id,
        )
        .await
        {
            self.deps
                .turn_supervisor
                .clear_turn_and_hook(session_id)
                .await;
            let failed = build_turn_terminal_envelope(
                session_id,
                &prepared.source,
                turn_id,
                TurnTerminalKind::Failed,
                Some(error.to_string()),
            );
            let _ = self
                .deps
                .eventing
                .persist_notification(session_id, failed)
                .await;
            return Err(error);
        }

        let is_first_turn = session_meta.last_event_seq <= 1;
        if is_first_turn
            && session_meta.title_source != TitleSource::User
            && self.deps.title_generator.is_some()
        {
            self.deps.spawn_title_generation(
                session_id.to_string(),
                prepared.resolved_payload.text_prompt.clone(),
            );
        }

        Ok(CommittedTurn { accepted })
    }

    async fn commit_accepted_launch_events(
        &self,
        session_id: &str,
        source: &agentdash_agent_protocol::SourceInfo,
        turn_id: &str,
        resolved_payload: &ResolvedPromptPayload,
    ) {
        let user_envelopes = build_user_message_envelopes(
            session_id,
            source,
            turn_id,
            &resolved_payload.user_blocks,
        );
        for envelope in user_envelopes {
            let _ = self
                .deps
                .eventing
                .persist_notification(session_id, envelope)
                .await;
        }

        let started = build_turn_started_envelope(session_id, source, turn_id);
        let _ = self
            .deps
            .eventing
            .persist_notification(session_id, started)
            .await;
    }
}

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
    use super::*;
    use crate::session::runtime_commands::{RuntimeCommandRecord, RuntimeCommandStatus};
    use crate::session::types::PendingCapabilityStateTransition;
    use async_trait::async_trait;
    use std::io;
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
