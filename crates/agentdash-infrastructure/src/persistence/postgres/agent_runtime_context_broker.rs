use std::sync::Arc;

use agentdash_agent_runtime::{RuntimeRepository, RuntimeStoreError};
use agentdash_agent_runtime_contract::{
    ContextCandidateId, RuntimeBindingId, RuntimeEvent, RuntimeJournalFact,
};
use agentdash_application_ports::agent_run_runtime::{
    AgentRunRuntimeBinding, AgentRunRuntimeBindingError,
};
use agentdash_integration_api::{
    AgentRuntimeContextBroker, DriverCompactionActivationRequest, DriverContextActivation,
    DriverContextCheckpointRequest, DriverContextError, DriverTranscript, DriverTranscriptRequest,
};
use async_trait::async_trait;

use super::{PostgresAgentRuntimeCompositionRepository, PostgresRuntimeRepository};

#[derive(Clone)]
pub struct PostgresAgentRuntimeContextBroker {
    runtime: Arc<PostgresRuntimeRepository>,
    composition: Arc<PostgresAgentRuntimeCompositionRepository>,
}

impl PostgresAgentRuntimeContextBroker {
    pub fn new(
        runtime: Arc<PostgresRuntimeRepository>,
        composition: Arc<PostgresAgentRuntimeCompositionRepository>,
    ) -> Self {
        Self {
            runtime,
            composition,
        }
    }

    async fn validated_binding(
        &self,
        binding_id: &RuntimeBindingId,
        generation: agentdash_agent_runtime_contract::RuntimeDriverGeneration,
    ) -> Result<AgentRunRuntimeBinding, DriverContextError> {
        let binding = self
            .composition
            .load_by_runtime_binding(binding_id)
            .await
            .map_err(map_binding_error)?
            .ok_or(DriverContextError::NotFound)?;

        if binding.binding_id != *binding_id || binding.driver_generation != generation {
            return Err(DriverContextError::Stale);
        }

        let thread = self
            .runtime
            .load_thread(&binding.thread_id)
            .await
            .map_err(map_runtime_error)?
            .ok_or(DriverContextError::NotFound)?;
        if thread.thread_id != binding.thread_id
            || thread.binding_id != binding.binding_id
            || thread.driver_generation != binding.driver_generation
        {
            return Err(DriverContextError::Stale);
        }

        Ok(binding)
    }
}

#[async_trait]
impl AgentRuntimeContextBroker for PostgresAgentRuntimeContextBroker {
    async fn load_transcript(
        &self,
        request: DriverTranscriptRequest,
    ) -> Result<DriverTranscript, DriverContextError> {
        let binding = self
            .validated_binding(&request.binding_id, request.generation)
            .await?;
        if binding.thread_id != request.runtime_thread_id {
            return Err(DriverContextError::Stale);
        }
        let batch = self
            .runtime
            .journal_records_after(&binding.thread_id, None)
            .await
            .map_err(map_runtime_error)?;
        let projection = self
            .runtime
            .load_thread(&binding.thread_id)
            .await
            .map_err(map_runtime_error)?
            .ok_or(DriverContextError::NotFound)?;
        if projection.thread_id != binding.thread_id
            || projection.binding_id != binding.binding_id
            || projection.driver_generation != binding.driver_generation
            || projection.source_thread_id != binding.source_thread_id
        {
            return Err(DriverContextError::Stale);
        }
        let current_thread_name = projection.thread_name.clone();
        let completed_presentation_item_ids = projection
            .presentation_transcript
            .into_iter()
            .map(|item| item.source_item_id)
            .collect();
        let active_compaction_source_end = match self
            .runtime
            .load_context_head(&binding.thread_id)
            .await
            .map_err(map_runtime_error)?
        {
            None => None,
            Some(head) => {
                let activation = batch.records.iter().rev().find_map(|record| {
                    let RuntimeJournalFact::Internal(RuntimeEvent::ContextCheckpointActivated {
                        checkpoint_id,
                        candidate_id,
                        compaction_id,
                        digest,
                        ..
                    }) = record.fact()
                    else {
                        return None;
                    };
                    (checkpoint_id == &head.checkpoint_id)
                        .then(|| (candidate_id.clone(), compaction_id.clone(), digest.clone()))
                });
                let Some((candidate_id, compaction_id, digest)) = activation else {
                    return Err(DriverContextError::InvalidMaterialization {
                        reason: format!(
                            "active context checkpoint `{}` has no retained activation record",
                            head.checkpoint_id.as_str()
                        ),
                    });
                };
                let candidate = self
                    .runtime
                    .load_context_candidate(&compaction_id)
                    .await
                    .map_err(map_runtime_error)?
                    .ok_or(DriverContextError::NotFound)?;
                if candidate.thread_id != binding.thread_id
                    || candidate.candidate_id != candidate_id
                    || candidate.checkpoint.checkpoint_id != head.checkpoint_id
                    || candidate.checkpoint.materialized.digest != digest
                    || head.digest != digest
                {
                    return Err(DriverContextError::Stale);
                }
                Some(candidate.source_end_event_sequence)
            }
        };
        Ok(DriverTranscript {
            earliest_available: batch.earliest_available,
            latest_available: batch.latest_available,
            current_thread_name,
            active_compaction_source_end,
            completed_presentation_item_ids,
            records: batch.records,
        })
    }

    async fn load_checkpoint(
        &self,
        request: DriverContextCheckpointRequest,
    ) -> Result<DriverContextActivation, DriverContextError> {
        let binding = self
            .validated_binding(&request.binding_id, request.generation)
            .await?;
        let checkpoint = self
            .runtime
            .load_context_checkpoint(&request.checkpoint_id)
            .await
            .map_err(map_runtime_error)?
            .ok_or(DriverContextError::NotFound)?;
        if checkpoint.thread_id != binding.thread_id
            || checkpoint.checkpoint_id != request.checkpoint_id
        {
            return Err(DriverContextError::Stale);
        }

        let candidate_id = ContextCandidateId::new(format!(
            "checkpoint-import:{}",
            checkpoint.checkpoint_id.as_str()
        ))
        .map_err(|error| DriverContextError::InvalidMaterialization {
            reason: error.to_string(),
        })?;
        Ok(DriverContextActivation {
            candidate_id,
            checkpoint_id: checkpoint.checkpoint_id,
            context_revision: checkpoint.revision,
            materialized: checkpoint.materialized,
        })
    }

    async fn compaction_activation(
        &self,
        request: DriverCompactionActivationRequest,
    ) -> Result<DriverContextActivation, DriverContextError> {
        let binding = self
            .validated_binding(&request.binding_id, request.generation)
            .await?;
        let candidate = self
            .runtime
            .load_context_candidate(&request.compaction_id)
            .await
            .map_err(map_runtime_error)?
            .ok_or(DriverContextError::NotFound)?;
        if candidate.thread_id != binding.thread_id
            || candidate.compaction_id != request.compaction_id
            || candidate.checkpoint.thread_id != binding.thread_id
        {
            return Err(DriverContextError::Stale);
        }

        Ok(DriverContextActivation {
            candidate_id: candidate.candidate_id,
            checkpoint_id: candidate.checkpoint.checkpoint_id,
            context_revision: candidate.checkpoint.revision,
            materialized: candidate.checkpoint.materialized,
        })
    }
}

fn map_binding_error(error: AgentRunRuntimeBindingError) -> DriverContextError {
    match error {
        AgentRunRuntimeBindingError::NotFound => DriverContextError::NotFound,
        AgentRunRuntimeBindingError::Conflict => DriverContextError::InvalidMaterialization {
            reason: "AgentRun runtime binding coordinates conflict".to_string(),
        },
        AgentRunRuntimeBindingError::Unavailable { reason, retryable } => {
            DriverContextError::Unavailable { reason, retryable }
        }
        AgentRunRuntimeBindingError::Persistence { reason } => DriverContextError::Unavailable {
            reason,
            retryable: true,
        },
    }
}

fn map_runtime_error(error: RuntimeStoreError) -> DriverContextError {
    match error {
        RuntimeStoreError::NotFound => DriverContextError::NotFound,
        RuntimeStoreError::Unavailable(reason) => DriverContextError::Unavailable {
            reason,
            retryable: true,
        },
        other => DriverContextError::InvalidMaterialization {
            reason: other.to_string(),
        },
    }
}
