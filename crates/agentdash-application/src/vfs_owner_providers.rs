use std::sync::{Arc, OnceLock};

use agentdash_agent_protocol::{BackboneEnvelope, SourceInfo, TraceInfo};
use agentdash_application_agentrun::agent_run::{
    AgentRunJournalQuery, AgentRunJournalService, agent_run_journal_session_id,
};
use agentdash_application_lifecycle::lifecycle::LifecycleMountProvider;
use agentdash_application_lifecycle::lifecycle::surface::journey::{
    AgentRunCompactionArchiveReader, AgentRunJournalProjection, AgentRunJournalReader,
    AgentRunJournalRef, JourneyResult, LifecycleJourneyError, SessionCompactionArchive,
    SessionCompactionArchiveStatus,
};
use agentdash_application_vfs::MountProviderRegistryBuilder;
use agentdash_platform_spi::PersistedSessionEvent;
use async_trait::async_trait;

use crate::canvas::CanvasFsMountProvider;
use crate::repository_set::RepositorySet;
use crate::vfs::{InlineFsMountProvider, RoutineMountProvider, SkillAssetFsMountProvider};

#[derive(Clone, Default)]
pub struct SharedAgentRunJournalReaderHandle {
    service: Arc<OnceLock<Arc<AgentRunJournalService>>>,
}

impl SharedAgentRunJournalReaderHandle {
    pub fn bind(&self, service: Arc<AgentRunJournalService>) -> Result<(), &'static str> {
        self.service
            .set(service)
            .map_err(|_| "AgentRun journal reader composition handle was already bound")
    }

    fn service(&self) -> JourneyResult<&Arc<AgentRunJournalService>> {
        self.service.get().ok_or_else(|| {
            LifecycleJourneyError::OperationFailed(
                "AgentRun journal reader composition handle is not bound".to_string(),
            )
        })
    }
}

pub trait MountProviderRegistryBuilderOwnerExt {
    fn with_application_builtins(
        self,
        repos: &RepositorySet,
        agent_run_journal_reader: SharedAgentRunJournalReaderHandle,
    ) -> Self;
}

impl MountProviderRegistryBuilderOwnerExt for MountProviderRegistryBuilder {
    fn with_application_builtins(
        self,
        repos: &RepositorySet,
        agent_run_journal_reader: SharedAgentRunJournalReaderHandle,
    ) -> Self {
        self.register(Arc::new(InlineFsMountProvider::new(
            repos.inline_file_repo.clone(),
        )))
        .register(Arc::new(LifecycleMountProvider::new(
            repos.lifecycle_run_repo.clone(),
            repos.lifecycle_agent_repo.clone(),
            repos.inline_file_repo.clone(),
            repos.skill_asset_repo.clone(),
            Arc::new(agent_run_journal_reader.clone()),
            Arc::new(agent_run_journal_reader),
        )))
        .register(Arc::new(RoutineMountProvider::new(
            repos.routine_execution_repo.clone(),
            repos.inline_file_repo.clone(),
        )))
        .register(Arc::new(CanvasFsMountProvider::new(
            repos.canvas_repo.clone(),
        )))
        .register(Arc::new(SkillAssetFsMountProvider::new(
            repos.skill_asset_repo.clone(),
        )))
    }
}

#[async_trait]
impl AgentRunJournalReader for SharedAgentRunJournalReaderHandle {
    async fn visible_journal(
        &self,
        reference: AgentRunJournalRef,
    ) -> JourneyResult<AgentRunJournalProjection> {
        let page = self
            .service()?
            .load_visible_journal_page(
                AgentRunJournalQuery {
                    run_id: reference.run_id,
                    agent_id: reference.agent_id,
                },
                0,
                u32::MAX,
            )
            .await
            .map_err(|error| {
                LifecycleJourneyError::OperationFailed(format!(
                    "读取 AgentRun journal 失败: {error}"
                ))
            })?;
        let session_id = agent_run_journal_session_id(reference.run_id, reference.agent_id);
        let delivery_runtime_session_id = page.delivery_runtime_thread_id.as_str().to_string();
        let events = page
            .events
            .into_iter()
            .map(|event| project_journal_event(&session_id, event))
            .collect::<JourneyResult<Vec<_>>>()?;
        Ok(AgentRunJournalProjection {
            delivery_runtime_session_id,
            events,
        })
    }
}

#[async_trait]
impl AgentRunCompactionArchiveReader for SharedAgentRunJournalReaderHandle {
    async fn list_archives(
        &self,
        reference: AgentRunJournalRef,
    ) -> JourneyResult<Vec<SessionCompactionArchive>> {
        self.service()?
            .list_context_compaction_archives(AgentRunJournalQuery {
                run_id: reference.run_id,
                agent_id: reference.agent_id,
            })
            .await
            .map_err(|error| {
                LifecycleJourneyError::OperationFailed(format!(
                    "读取 AgentRun compaction archive 失败: {error}"
                ))
            })
            .map(|archives| {
                archives
                    .into_iter()
                    .map(|archive| SessionCompactionArchive {
                        id: archive.compaction_id,
                        lifecycle_item_id: archive.lifecycle_item_id,
                        projection_version: archive.projection_version,
                        completed_event_seq: archive.completed_event_seq,
                        source_start_event_seq: archive.source_start_event_seq,
                        source_end_event_seq: archive.source_end_event_seq,
                        summary: archive.summary,
                        trigger: archive.trigger,
                        phase: archive.phase,
                        strategy: archive.strategy,
                        token_stats_json: serde_json::json!({
                            "tokens_before": archive.tokens_before,
                            "messages_compacted": archive.messages_compacted,
                        }),
                        diagnostics_json: serde_json::Value::Null,
                        turn_id: archive.turn_id,
                        entry_index: archive.entry_index,
                        status: SessionCompactionArchiveStatus::ProjectionCommitted,
                    })
                    .collect()
            })
    }
}

fn project_journal_event(
    session_id: &str,
    event: agentdash_application_agentrun::agent_run::AgentRunJournalEvent,
) -> JourneyResult<PersistedSessionEvent> {
    let carrier = event.record.carrier();
    let presentation = event.record.as_presentation().ok_or_else(|| {
        LifecycleJourneyError::OperationFailed(
            "AgentRun visible journal contained an internal Runtime fact".to_string(),
        )
    })?;
    let event_value = serde_json::to_value(&presentation.event)
        .map_err(|error| LifecycleJourneyError::OperationFailed(error.to_string()))?;
    let session_update_type = event_value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            LifecycleJourneyError::OperationFailed(
                "AgentRun presentation event is missing its protocol type".to_string(),
            )
        })?
        .to_string();
    let recorded_at_ms = i64::try_from(carrier.recorded_at_ms).map_err(|_| {
        LifecycleJourneyError::OperationFailed(
            "AgentRun journal timestamp exceeds the supported range".to_string(),
        )
    })?;
    let observed_at = chrono::DateTime::from_timestamp_millis(recorded_at_ms).ok_or_else(|| {
        LifecycleJourneyError::OperationFailed("AgentRun journal timestamp is invalid".to_string())
    })?;
    let notification = BackboneEnvelope {
        event: presentation.event.clone(),
        session_id: session_id.to_string(),
        source: SourceInfo {
            connector_id: "agent_run_journal".to_string(),
            connector_type: "managed_runtime".to_string(),
            executor_id: None,
        },
        trace: TraceInfo {
            turn_id: carrier.coordinate.source_turn_id.clone(),
            entry_index: carrier.coordinate.source_entry_index,
        },
        observed_at,
    };
    Ok(PersistedSessionEvent {
        session_id: session_id.to_string(),
        event_seq: event.journal_seq,
        occurred_at_ms: recorded_at_ms,
        committed_at_ms: recorded_at_ms,
        session_update_type,
        turn_id: carrier.coordinate.source_turn_id.clone(),
        entry_index: carrier.coordinate.source_entry_index,
        tool_call_id: None,
        ephemeral: presentation.durability
            == agentdash_agent_runtime_contract::PresentationDurability::Ephemeral,
        notification: serde_json::to_value(notification)
            .map_err(|error| LifecycleJourneyError::OperationFailed(error.to_string()))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn unbound_agent_run_journal_handle_fails_explicitly() {
        let handle = SharedAgentRunJournalReaderHandle::default();
        let error = handle
            .visible_journal(AgentRunJournalRef::new(Uuid::new_v4(), Uuid::new_v4()))
            .await
            .expect_err("unbound composition handle must fail");

        assert!(matches!(
            error,
            LifecycleJourneyError::OperationFailed(message)
                if message.contains("is not bound")
        ));
    }
}
