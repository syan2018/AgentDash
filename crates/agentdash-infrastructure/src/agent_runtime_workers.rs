use std::sync::Arc;

use crate::{PostgresAgentRuntimeCompositionRepository, PostgresRuntimeRepository};
use agentdash_agent_runtime::{
    ActivationObservation, CompactionPreparation, ContextPreparationStatus, HookEffect,
    HookRunStatus, ManagedAgentRuntime, RuntimeRepository, RuntimeWorkClaim,
    RuntimeWorkClaimRequest, RuntimeWorkKind, RuntimeWorkPayload, RuntimeWorkQueue,
    RuntimeWorkerId, failure_completion,
};
use agentdash_agent_runtime_contract::{
    ContextActivationId, ContextBlock, ContextCandidateId, ContextCheckpointId, ContextDigest,
    ContextFidelity, ContextProvenance, ContextRecipe, ContextRecipeRevision,
    DriverCommandEnvelope, DriverEventEnvelope, DriverEventSink, DriverInspection,
    DriverInspectionQuery, DriverRequestId, RuntimeCommand, RuntimeEvent, RuntimeItemContent,
};
use agentdash_agent_runtime_host::{IntegrationDriverHost, RouteDriverCommand};
use agentdash_diagnostics::{Subsystem, diag};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum RuntimeDurableWorkerError {
    #[error("Runtime durable worker store failed: {0}")]
    Store(String),
    #[error("Runtime durable worker claim is invalid: {0}")]
    InvalidClaim(String),
    #[error("Runtime durable worker Host operation failed: {0}")]
    Host(String),
    #[error("Runtime durable worker processing failed: {0}")]
    Processing(String),
}

#[async_trait]
pub trait RuntimeHookEffectDispatcher: Send + Sync {
    async fn dispatch(&self, effect: &HookEffect) -> Result<(), String>;
}

/// Production default for the currently declared diagnostic effect authority.
///
/// Unknown authorities remain leased/retried instead of being acknowledged as if their side
/// effect had happened. Enterprise composition can inject its own dispatcher.
pub struct DiagnosticRuntimeHookEffectDispatcher;

#[async_trait]
impl RuntimeHookEffectDispatcher for DiagnosticRuntimeHookEffectDispatcher {
    async fn dispatch(&self, effect: &HookEffect) -> Result<(), String> {
        if effect.descriptor.target_authority != "agentdash_hook_effect_dispatcher"
            || !effect.descriptor.effect_type.starts_with("diagnostic:")
        {
            return Err(format!(
                "no Runtime hook effect executor for authority `{}` and type `{}`",
                effect.descriptor.target_authority, effect.descriptor.effect_type
            ));
        }
        diag!(
            Info,
            Subsystem::AgentRun,
            hook_effect_id = effect.effect_id.to_string(),
            hook_run_id = effect.hook_run_id.to_string(),
            effect_type = effect.descriptor.effect_type.clone(),
            payload = effect.payload.to_string(),
            "Runtime durable diagnostic hook effect"
        );
        Ok(())
    }
}

pub struct RuntimeDurableWorkers {
    store: Arc<PostgresRuntimeRepository>,
    runtime: Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>,
    composition: Arc<PostgresAgentRuntimeCompositionRepository>,
    host: Arc<IntegrationDriverHost>,
    hook_effects: Arc<dyn RuntimeHookEffectDispatcher>,
    node_id: String,
}

impl RuntimeDurableWorkers {
    pub fn new(
        store: Arc<PostgresRuntimeRepository>,
        runtime: Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>,
        composition: Arc<PostgresAgentRuntimeCompositionRepository>,
        host: Arc<IntegrationDriverHost>,
        hook_effects: Arc<dyn RuntimeHookEffectDispatcher>,
        node_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            runtime,
            composition,
            host,
            hook_effects,
            node_id: node_id.into(),
        }
    }

    pub async fn run_once(
        &self,
        kind: RuntimeWorkKind,
        limit: u32,
    ) -> Result<usize, RuntimeDurableWorkerError> {
        if kind == RuntimeWorkKind::RuntimeOutbox {
            return Err(RuntimeDurableWorkerError::InvalidClaim(
                "RuntimeOutbox is owned by RuntimeOutboxWorker".to_string(),
            ));
        }
        let claims = self
            .store
            .claim(RuntimeWorkClaimRequest {
                kind,
                owner: RuntimeWorkerId(format!("{}-{kind:?}", self.node_id)),
                lease_duration_ms: 5 * 60 * 1_000,
                limit,
            })
            .await
            .map_err(|error| RuntimeDurableWorkerError::Store(error.to_string()))?;
        let count = claims.len();
        let mut first_error = None;
        for claim in claims {
            match self.process(&claim).await {
                Ok(()) => {
                    if let Err(error) = self.store.ack(&claim).await {
                        let error = RuntimeDurableWorkerError::Store(error.to_string());
                        let _ = self.store.release(&claim, error.to_string()).await;
                        first_error.get_or_insert(error);
                    }
                }
                Err(error) => {
                    if let Err(release) = self.store.release(&claim, error.to_string()).await {
                        first_error.get_or_insert_with(|| {
                            RuntimeDurableWorkerError::Store(release.to_string())
                        });
                    } else {
                        first_error.get_or_insert(error);
                    }
                }
            }
        }
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(count)
        }
    }

    async fn process(&self, claim: &RuntimeWorkClaim) -> Result<(), RuntimeDurableWorkerError> {
        match &claim.payload {
            RuntimeWorkPayload::ContextPreparation(work) => self.prepare_context(work).await,
            RuntimeWorkPayload::ContextActivationDispatch(entry) => {
                self.dispatch_context_activation(entry).await
            }
            RuntimeWorkPayload::ContextActivationRecovery(activation) => {
                self.recover_context_activation(activation).await
            }
            RuntimeWorkPayload::HookRunRecovery(run) => self.recover_hook_run(run).await,
            RuntimeWorkPayload::HookEffect(effect) => self
                .hook_effects
                .dispatch(effect)
                .await
                .map_err(RuntimeDurableWorkerError::Processing),
            RuntimeWorkPayload::RuntimeOutbox(_) => Err(RuntimeDurableWorkerError::InvalidClaim(
                "RuntimeOutbox payload reached a durable side-effect worker".to_string(),
            )),
        }
    }

    async fn prepare_context(
        &self,
        work: &agentdash_agent_runtime::ContextPreparationWorkItem,
    ) -> Result<(), RuntimeDurableWorkerError> {
        if !matches!(work.status, ContextPreparationStatus::Pending) {
            return Ok(());
        }
        let thread = self
            .store
            .load_thread(&work.thread_id)
            .await
            .map_err(|error| RuntimeDurableWorkerError::Store(error.to_string()))?
            .ok_or_else(|| {
                RuntimeDurableWorkerError::InvalidClaim("Runtime thread does not exist".to_string())
            })?;
        let surface = self
            .composition
            .load_bound_surface(&thread.binding_id)
            .await
            .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?
            .ok_or_else(|| {
                RuntimeDurableWorkerError::InvalidClaim(
                    "bound Runtime surface does not exist".to_string(),
                )
            })?;
        let mut blocks = surface
            .context
            .instructions
            .into_iter()
            .flat_map(|set| set.entries)
            .map(|text| ContextBlock::Instruction { text })
            .chain(surface.context.blocks)
            .collect::<Vec<_>>();
        let source_item_ids = thread.item_order.clone();
        let mut item_contents = thread
            .items
            .iter()
            .map(|(id, item)| (id.clone(), item.initial_content.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let event_batch = self
            .store
            .events_after(&work.thread_id, None)
            .await
            .map_err(|error| RuntimeDurableWorkerError::Store(error.to_string()))?;
        for event in event_batch.events {
            if let RuntimeEvent::ItemDelta { item_id, delta, .. } = event.event
                && let Some(content) = item_contents.get_mut(&item_id)
            {
                match content {
                    RuntimeItemContent::AgentMessage { text }
                    | RuntimeItemContent::Reasoning { text } => text.push_str(&delta),
                    _ => {}
                }
            }
        }
        blocks.extend(source_item_ids.iter().filter_map(|item_id| {
            item_contents
                .get(item_id)
                .cloned()
                .map(|content| ContextBlock::RuntimeItem { content })
        }));
        let recipe = ContextRecipe {
            revision: ContextRecipeRevision(work.expected_base_revision.0 + 1),
            provenance: ContextProvenance {
                settings_revision: thread.settings_revision,
                tool_set_revision: thread.tool_set_revision,
            },
            source_item_ids,
        };
        let digest = ContextDigest::new(digest_json(&(&recipe, &blocks))?)
            .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?;
        self.runtime
            .prepare_compaction(CompactionPreparation {
                candidate_id: ContextCandidateId::new(format!("candidate-{}", work.compaction_id))
                    .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?,
                compaction_id: work.compaction_id.clone(),
                activation_id: ContextActivationId::new(format!(
                    "activation-{}",
                    work.compaction_id
                ))
                .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?,
                operation_id: work.operation_id.clone(),
                thread_id: work.thread_id.clone(),
                trigger: work.trigger,
                expected_base_checkpoint_id: work.expected_base_checkpoint_id.clone(),
                expected_base_revision: work.expected_base_revision,
                checkpoint_id: ContextCheckpointId::new(format!(
                    "checkpoint-{}",
                    work.compaction_id
                ))
                .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?,
                materialized: agentdash_agent_runtime_contract::MaterializedContext {
                    recipe,
                    blocks,
                    digest,
                    fidelity: ContextFidelity::PlatformExact,
                },
            })
            .await
            .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))
    }

    async fn dispatch_context_activation(
        &self,
        entry: &agentdash_agent_runtime::ContextActivationOutboxEntry,
    ) -> Result<(), RuntimeDurableWorkerError> {
        let thread = self
            .store
            .load_thread(&entry.thread_id)
            .await
            .map_err(|error| RuntimeDurableWorkerError::Store(error.to_string()))?
            .ok_or_else(|| RuntimeDurableWorkerError::InvalidClaim("thread missing".to_string()))?;
        if thread.binding_id != entry.binding_id || thread.driver_generation != entry.generation {
            return Err(RuntimeDurableWorkerError::InvalidClaim(
                "context activation generation is stale".to_string(),
            ));
        }
        let preparation = self
            .store
            .load_context_preparation(&entry.compaction_id)
            .await
            .map_err(|error| RuntimeDurableWorkerError::Store(error.to_string()))?
            .ok_or_else(|| {
                RuntimeDurableWorkerError::InvalidClaim("context preparation missing".to_string())
            })?;
        let lease = self
            .host
            .acquire_driver_lease(&entry.binding_id)
            .await
            .map_err(|error| RuntimeDurableWorkerError::Host(error.to_string()))?;
        let result = self
            .host
            .dispatch(
                RouteDriverCommand {
                    envelope: DriverCommandEnvelope {
                        request_id: DriverRequestId::new(format!(
                            "context-activation-{}",
                            entry.activation_id
                        ))
                        .map_err(|error| {
                            RuntimeDurableWorkerError::InvalidClaim(error.to_string())
                        })?,
                        binding_id: entry.binding_id.clone(),
                        generation: entry.generation,
                        source_thread_id: thread.source_thread_id,
                        command: RuntimeCommand::ContextCompact {
                            thread_id: entry.thread_id.clone(),
                            compaction_id: entry.compaction_id.clone(),
                            trigger: preparation.trigger,
                            base_checkpoint_id: preparation.expected_base_checkpoint_id,
                            expected_context_revision: preparation.expected_base_revision,
                        },
                    },
                    lease_owner: lease.owner.clone(),
                    lease_token: lease.token.clone(),
                },
                Arc::new(DurableWorkerEventSink {
                    runtime: self.runtime.clone(),
                }),
            )
            .await
            .map_err(|error| RuntimeDurableWorkerError::Host(error.to_string()));
        let release = self.host.release_driver_lease(&lease).await;
        result?;
        release.map_err(|error| RuntimeDurableWorkerError::Host(error.to_string()))?;
        Ok(())
    }

    async fn recover_context_activation(
        &self,
        activation: &agentdash_agent_runtime::ContextActivation,
    ) -> Result<(), RuntimeDurableWorkerError> {
        let thread = self
            .store
            .load_thread(&activation.thread_id)
            .await
            .map_err(|error| RuntimeDurableWorkerError::Store(error.to_string()))?
            .ok_or_else(|| RuntimeDurableWorkerError::InvalidClaim("thread missing".to_string()))?;
        let observation = match self
            .host
            .inspect_binding_driver(
                &thread.binding_id,
                DriverInspectionQuery::CompactionActivation {
                    candidate_id: activation.candidate_id.clone(),
                },
            )
            .await
            .map_err(|error| RuntimeDurableWorkerError::Host(error.to_string()))?
        {
            DriverInspection::CompactionActivation { applied: false, .. } => {
                ActivationObservation::NotApplied
            }
            DriverInspection::CompactionActivation {
                applied: true,
                digest: Some(digest),
                driver_context_revision: Some(driver_context_revision),
            } => ActivationObservation::Applied {
                digest: ContextDigest::new(digest)
                    .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?,
                driver_context_revision,
            },
            DriverInspection::CompactionActivation { .. } => ActivationObservation::Unverifiable {
                reason: "driver reported incomplete compaction activation evidence".to_string(),
            },
            _ => ActivationObservation::Unverifiable {
                reason: "driver returned the wrong inspection variant".to_string(),
            },
        };
        self.runtime
            .recover_compaction(&activation.compaction_id, observation)
            .await
            .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))
    }

    async fn recover_hook_run(
        &self,
        run: &agentdash_agent_runtime::HookRun,
    ) -> Result<(), RuntimeDurableWorkerError> {
        if run.status.is_terminal() {
            return Ok(());
        }
        if run.status == HookRunStatus::Accepted {
            self.runtime
                .start_hook(&run.hook_run_id)
                .await
                .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?;
        }
        self.runtime
            .complete_hook(
                &run.hook_run_id,
                failure_completion(
                    run.failure_policy,
                    "hook execution was recovered after its owner stopped".to_string(),
                ),
                Vec::new(),
            )
            .await
            .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?;
        Ok(())
    }

    pub fn spawn(
        self: Arc<Self>,
        cancellation: CancellationToken,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        [
            RuntimeWorkKind::ContextPreparation,
            RuntimeWorkKind::ContextActivationDispatch,
            RuntimeWorkKind::ContextActivationRecovery,
            RuntimeWorkKind::HookRunRecovery,
            RuntimeWorkKind::HookEffect,
        ]
        .into_iter()
        .map(|kind| {
            let worker = self.clone();
            let cancellation = cancellation.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    tokio::select! {
                        _ = cancellation.cancelled() => break,
                        _ = interval.tick() => {
                            if let Err(error) = worker.run_once(kind, 32).await {
                                diag!(
                                    Error,
                                    Subsystem::AgentRun,
                                    work_kind = format!("{kind:?}"),
                                    error = error.to_string(),
                                    "Runtime durable worker iteration failed"
                                );
                            }
                        }
                    }
                }
            })
        })
        .collect()
    }
}

struct DurableWorkerEventSink {
    runtime: Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>,
}

#[async_trait]
impl DriverEventSink for DurableWorkerEventSink {
    async fn emit(
        &self,
        event: DriverEventEnvelope,
    ) -> Result<(), agentdash_agent_runtime_contract::DriverError> {
        self.runtime
            .ingest_driver_event(event)
            .await
            .map(|_| ())
            .map_err(
                |error| agentdash_agent_runtime_contract::DriverError::Lost {
                    reason: error.to_string(),
                    retryable: true,
                },
            )
    }
}

fn digest_json(value: &impl serde::Serialize) -> Result<String, RuntimeDurableWorkerError> {
    let value = serde_json::to_value(value)
        .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?;
    let bytes = agentdash_agent_runtime_host::canonical_json(&value);
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}
