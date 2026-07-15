use std::sync::Arc;

use crate::{PostgresAgentRuntimeCompositionRepository, PostgresRuntimeRepository};
use agentdash_agent::compaction::execute_compaction;
use agentdash_agent::{
    AgentMessage, CompactionParams, ContentPart, MessageRef, StopReason, ToolCallInfo,
};
use agentdash_agent_runtime::CompactionPresentationFacts;
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
    DriverInspectionQuery, DriverRequestId, RuntimeCommand,
};
use agentdash_agent_runtime_host::AgentRuntimeHostRepository;
use agentdash_agent_runtime_host::{IntegrationDriverHost, RouteDriverCommand};
use agentdash_agent_types::{
    CompactionImplementation, CompactionMetadata, CompactionPhase, CompactionReason,
    CompactionStrategy, CompactionTrigger, CompactionTriggerStats,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::llm_provider::{
    LlmProviderCredentialRepository, LlmProviderRepository, LlmSecretCodec,
};
use agentdash_integration_native_agent::{NativeAgentServiceConfig, NativeCredentialScope};
use agentdash_llm_provider::{
    ProviderCredentialScope, resolve_effective_bridge_with_model_for_scope,
};
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
    #[error("Runtime durable worker compaction has no eligible messages")]
    NoEligibleCompaction,
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

pub struct NativeManagedCompactionEngine {
    provider_repository: Arc<dyn LlmProviderRepository>,
    credential_repository: Arc<dyn LlmProviderCredentialRepository>,
    secret_codec: Arc<dyn LlmSecretCodec>,
    policy: ManagedCompactionPolicy,
}

#[async_trait]
pub trait ManagedCompactionPreparationEngine: Send + Sync {
    async fn compact(
        &self,
        thread: &agentdash_agent_runtime::RuntimeThreadState,
        surface: &agentdash_integration_api::MaterializedDriverSurface,
        instance: &agentdash_agent_runtime_host::AgentServiceInstance,
        work: &agentdash_agent_runtime::ContextPreparationWorkItem,
    ) -> Result<ManagedCompactionOutput, RuntimeDurableWorkerError>;
}

#[derive(Debug, Clone, Copy)]
pub struct ManagedCompactionPolicy {
    pub keep_last_n: u32,
    pub reserve_tokens: u64,
}

impl NativeManagedCompactionEngine {
    pub fn new(
        provider_repository: Arc<dyn LlmProviderRepository>,
        credential_repository: Arc<dyn LlmProviderCredentialRepository>,
        secret_codec: Arc<dyn LlmSecretCodec>,
        policy: ManagedCompactionPolicy,
    ) -> Self {
        Self {
            provider_repository,
            credential_repository,
            secret_codec,
            policy,
        }
    }
}

#[async_trait]
impl ManagedCompactionPreparationEngine for NativeManagedCompactionEngine {
    async fn compact(
        &self,
        thread: &agentdash_agent_runtime::RuntimeThreadState,
        surface: &agentdash_integration_api::MaterializedDriverSurface,
        instance: &agentdash_agent_runtime_host::AgentServiceInstance,
        work: &agentdash_agent_runtime::ContextPreparationWorkItem,
    ) -> Result<ManagedCompactionOutput, RuntimeDurableWorkerError> {
        let config: NativeAgentServiceConfig = serde_json::from_value(instance.config.clone())
            .map_err(|error| {
                RuntimeDurableWorkerError::Processing(format!(
                    "managed compaction requires a Native provider binding: {error}"
                ))
            })?;
        let scope = match config.credential_scope {
            NativeCredentialScope::Platform => ProviderCredentialScope::Platform,
            NativeCredentialScope::User { user_id } => ProviderCredentialScope::User { user_id },
        };
        let resolved = resolve_effective_bridge_with_model_for_scope(
            self.provider_repository.as_ref(),
            Some(self.credential_repository.as_ref()),
            self.secret_codec.as_ref(),
            &scope,
            config.provider.trim(),
            Some(config.model.trim()),
        )
        .await
        .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?;

        let transcript = completed_compaction_messages(thread)?;
        let messages = transcript
            .iter()
            .map(|entry| entry.message.clone())
            .collect::<Vec<_>>();
        let refs = transcript
            .iter()
            .map(|entry| Some(entry.reference.clone()))
            .collect::<Vec<_>>();
        let tokens_before = messages
            .iter()
            .map(agentdash_agent_types::estimate_message_tokens)
            .sum();
        let metadata = CompactionMetadata {
            trigger: match work.trigger {
                agentdash_agent_runtime_contract::ContextCompactionTrigger::Automatic => {
                    CompactionTrigger::Auto
                }
                agentdash_agent_runtime_contract::ContextCompactionTrigger::Manual => {
                    CompactionTrigger::Manual
                }
            },
            reason: match work.trigger {
                agentdash_agent_runtime_contract::ContextCompactionTrigger::Automatic => {
                    CompactionReason::TokenPressure
                }
                agentdash_agent_runtime_contract::ContextCompactionTrigger::Manual => {
                    CompactionReason::UserRequested
                }
            },
            phase: CompactionPhase::StandaloneCompactTurn,
            strategy: CompactionStrategy::SummaryPrefix,
            implementation: CompactionImplementation::LocalSummary,
            request_id: Some(work.compaction_id.to_string()),
        };
        let result = execute_compaction(
            &messages,
            &refs,
            &CompactionParams {
                keep_last_n: self.policy.keep_last_n,
                reserve_tokens: self.policy.reserve_tokens,
                custom_summary: None,
                custom_prompt: None,
                trigger_stats: CompactionTriggerStats {
                    input_tokens: tokens_before,
                    context_window: resolved.model.context_window,
                    reserve_tokens: self.policy.reserve_tokens,
                },
                metadata: metadata.clone(),
            },
            metadata,
            resolved.bridge.as_ref(),
            &CancellationToken::new(),
        )
        .await
        .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?
        .ok_or(RuntimeDurableWorkerError::NoEligibleCompaction)?;
        let (summary, messages_compacted, compacted_until_ref, timestamp_ms) =
            match &result.summary_message {
                AgentMessage::CompactionSummary {
                    summary,
                    messages_compacted,
                    compacted_until_ref,
                    timestamp,
                    ..
                } => (
                    summary.clone(),
                    *messages_compacted,
                    compacted_until_ref.clone(),
                    *timestamp,
                ),
                _ => {
                    return Err(RuntimeDurableWorkerError::Processing(
                        "managed compaction did not return a summary message".to_string(),
                    ));
                }
            };
        let mut source_item_ids = kept_source_item_ids(
            &thread.item_order,
            &transcript,
            result.first_kept_ref.as_ref(),
        )?;
        let mut blocks = surface
            .context
            .instructions
            .iter()
            .flat_map(|set| set.entries.iter())
            .cloned()
            .map(|text| ContextBlock::Instruction { text })
            .collect::<Vec<_>>();
        blocks.extend(surface.context.blocks.clone());
        blocks.push(ContextBlock::CompactionSummary {
            summary: summary.clone(),
        });
        blocks.extend(
            source_item_ids
                .iter()
                .filter_map(|id| thread.items.get(id))
                .map(|item| {
                    let content = match &item.phase {
                        agentdash_agent_runtime::EntityPhase::Terminal(
                            agentdash_agent_runtime_contract::RuntimeItemTerminal::Completed {
                                final_content,
                            },
                        ) => final_content.clone(),
                        _ => item.initial_content.clone(),
                    };
                    ContextBlock::RuntimeItem { content }
                }),
        );
        let compacted_until_value = compacted_until_ref.as_ref().map(|reference| {
            serde_json::json!({
                "turn_id": reference.turn_id,
                "entry_index": reference.entry_index,
            })
        });
        Ok(ManagedCompactionOutput {
            blocks,
            source_item_ids: std::mem::take(&mut source_item_ids),
            presentation: CompactionPresentationFacts {
                summary,
                tokens_before,
                messages_compacted,
                compaction_id: Some(work.compaction_id.to_string()),
                projection_version: None,
                strategy: Some("summary_prefix".to_string()),
                trigger: Some(
                    match work.trigger {
                        agentdash_agent_runtime_contract::ContextCompactionTrigger::Automatic => {
                            "auto"
                        }
                        agentdash_agent_runtime_contract::ContextCompactionTrigger::Manual => {
                            "manual"
                        }
                    }
                    .to_string(),
                ),
                phase: Some("standalone_compact_turn".to_string()),
                source_start_event_seq: None,
                source_end_event_seq: None,
                first_kept_event_seq: None,
                compacted_until_ref: compacted_until_value,
                timestamp_ms,
            },
        })
    }
}

pub struct ManagedCompactionOutput {
    pub blocks: Vec<ContextBlock>,
    pub source_item_ids: Vec<agentdash_agent_runtime_contract::RuntimeItemId>,
    pub presentation: CompactionPresentationFacts,
}

struct CompactionTranscriptEntry {
    message: AgentMessage,
    reference: MessageRef,
    runtime_item_index: usize,
}

fn kept_source_item_ids(
    item_order: &[agentdash_agent_runtime_contract::RuntimeItemId],
    transcript: &[CompactionTranscriptEntry],
    first_kept_ref: Option<&MessageRef>,
) -> Result<Vec<agentdash_agent_runtime_contract::RuntimeItemId>, RuntimeDurableWorkerError> {
    let Some(first_kept_ref) = first_kept_ref else {
        return Ok(Vec::new());
    };
    let runtime_item_index = transcript
        .iter()
        .find(|entry| &entry.reference == first_kept_ref)
        .map(|entry| entry.runtime_item_index)
        .ok_or_else(|| {
            RuntimeDurableWorkerError::Processing(
                "managed compaction first-kept reference is not backed by a canonical Runtime item"
                    .to_string(),
            )
        })?;
    Ok(item_order
        .iter()
        .skip(runtime_item_index)
        .cloned()
        .collect())
}

fn completed_compaction_messages(
    thread: &agentdash_agent_runtime::RuntimeThreadState,
) -> Result<Vec<CompactionTranscriptEntry>, RuntimeDurableWorkerError> {
    let mut entries = Vec::new();
    for (index, id) in thread.item_order.iter().enumerate() {
        let Some(item) = thread.items.get(id) else {
            continue;
        };
        let agentdash_agent_runtime::EntityPhase::Terminal(
            agentdash_agent_runtime_contract::RuntimeItemTerminal::Completed { final_content },
        ) = &item.phase
        else {
            continue;
        };
        let reference = MessageRef {
            turn_id: item.turn_id.to_string(),
            entry_index: index as u32,
        };
        entries.extend(
            compaction_messages(final_content)?
                .into_iter()
                .map(|message| CompactionTranscriptEntry {
                    message,
                    reference: reference.clone(),
                    runtime_item_index: index,
                }),
        );
    }
    Ok(entries)
}

fn compaction_messages(
    content: &agentdash_agent_runtime_contract::RuntimeItemContent,
) -> Result<Vec<AgentMessage>, RuntimeDurableWorkerError> {
    use agentdash_agent_protocol::{AgentDashThreadItem, CodexThreadItem};
    match content.item() {
        AgentDashThreadItem::Codex(CodexThreadItem::UserMessage { content, .. }) => {
            Ok(vec![AgentMessage::User {
                content: content
                    .iter()
                    .filter_map(|part| match part {
                        agentdash_agent_protocol::codex_app_server_protocol::UserInput::Text {
                            text,
                            ..
                        } => Some(ContentPart::text(text.clone())),
                        agentdash_agent_protocol::codex_app_server_protocol::UserInput::Image {
                            url,
                            ..
                        } => Some(ContentPart::image("image/*", url.clone())),
                        _ => None,
                    })
                    .collect(),
                timestamp: None,
            }])
        }
        AgentDashThreadItem::Codex(CodexThreadItem::AgentMessage { text, .. }) => {
            Ok(vec![AgentMessage::Assistant {
                content: vec![ContentPart::text(text)],
                tool_calls: Vec::new(),
                stop_reason: None,
                error_message: None,
                usage: None,
                timestamp: None,
            }])
        }
        AgentDashThreadItem::Codex(CodexThreadItem::Reasoning {
            summary, content, ..
        }) => Ok(vec![AgentMessage::Assistant {
            content: vec![ContentPart::reasoning(
                summary
                    .iter()
                    .chain(content)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n"),
                None,
                None,
            )],
            tool_calls: Vec::new(),
            stop_reason: None,
            error_message: None,
            usage: None,
            timestamp: None,
        }]),
        item => tool_compaction_messages(item).ok_or_else(|| {
            RuntimeDurableWorkerError::Processing(format!(
                "managed compaction cannot losslessly project canonical item {}",
                item.id()
            ))
        }),
    }
}

fn tool_compaction_messages(
    item: &agentdash_agent_protocol::AgentDashThreadItem,
) -> Option<Vec<AgentMessage>> {
    use agentdash_agent_protocol::{AgentDashThreadItem, CodexThreadItem};
    let (id, name, arguments, output, details, is_error) = match item {
        AgentDashThreadItem::Codex(CodexThreadItem::CommandExecution {
            id,
            command,
            cwd,
            aggregated_output,
            exit_code,
            ..
        }) => (
            id.clone(),
            "command_execution".to_string(),
            serde_json::json!({"command":command,"cwd":cwd}),
            aggregated_output.as_ref()?.as_ref()?.clone(),
            Some(serde_json::json!({"exit_code":exit_code})),
            exit_code.as_ref().and_then(|v| *v).is_some_and(|v| v != 0),
        ),
        AgentDashThreadItem::Codex(CodexThreadItem::McpToolCall {
            id,
            tool,
            arguments,
            result,
            error,
            ..
        }) => {
            let details = result
                .as_ref()?
                .as_ref()
                .and_then(|value| serde_json::to_value(value).ok())
                .or_else(|| {
                    error
                        .as_ref()?
                        .as_ref()
                        .map(|value| serde_json::Value::String(value.message.clone()))
                })?;
            (
                id.clone(),
                tool.clone(),
                arguments.clone(),
                details.to_string(),
                Some(details),
                error.as_ref().and_then(Option::as_ref).is_some(),
            )
        }
        AgentDashThreadItem::Codex(CodexThreadItem::DynamicToolCall {
            id,
            tool,
            arguments,
            content_items,
            success,
            ..
        }) => {
            let items = content_items.as_ref()?.as_ref()?;
            let details = serde_json::to_value(items).ok()?;
            let output = items
                .iter()
                .map(|item| match item {
                    agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                        text,
                    } => text.clone(),
                    agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputImage {
                        image_url,
                    } => image_url.clone(),
                })
                .collect::<Vec<_>>()
                .join("\n");
            (
                id.clone(),
                tool.clone(),
                arguments.clone(),
                output,
                Some(details),
                success.as_ref().and_then(|v| *v) == Some(false),
            )
        }
        AgentDashThreadItem::AgentDash(native) => {
            let (output, details) = if let Some(output) = native.shell_output() {
                (
                    output.to_string(),
                    serde_json::Value::String(output.to_string()),
                )
            } else {
                let items = native.content_items()?;
                let details = serde_json::to_value(items).ok()?;
                let output = items.iter().map(|item| match item {
                    agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText { text } => text.clone(),
                    agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputImage { image_url } => image_url.clone(),
                }).collect::<Vec<_>>().join("\n");
                (output, details)
            };
            (
                native.id().to_string(),
                native.tool_name().to_string(),
                native.arguments()?.clone(),
                output,
                Some(details),
                native.success() == Some(false),
            )
        }
        _ => return None,
    };
    Some(vec![
        AgentMessage::Assistant {
            content: Vec::new(),
            tool_calls: vec![ToolCallInfo {
                id: id.clone(),
                call_id: Some(id.clone()),
                name: name.clone(),
                arguments,
            }],
            stop_reason: Some(StopReason::ToolUse),
            error_message: None,
            usage: None,
            timestamp: None,
        },
        AgentMessage::ToolResult {
            tool_call_id: id.clone(),
            call_id: Some(id),
            tool_name: Some(name),
            content: vec![ContentPart::text(output)],
            details,
            is_error,
            timestamp: None,
        },
    ])
}

pub struct RuntimeDurableWorkers {
    store: Arc<PostgresRuntimeRepository>,
    runtime: Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>,
    composition: Arc<PostgresAgentRuntimeCompositionRepository>,
    host: Arc<IntegrationDriverHost>,
    host_repository: Arc<dyn AgentRuntimeHostRepository>,
    managed_compaction: Option<Arc<dyn ManagedCompactionPreparationEngine>>,
    hook_effects: Arc<dyn RuntimeHookEffectDispatcher>,
    node_id: String,
}

impl RuntimeDurableWorkers {
    pub fn new(
        store: Arc<PostgresRuntimeRepository>,
        runtime: Arc<ManagedAgentRuntime<PostgresRuntimeRepository>>,
        composition: Arc<PostgresAgentRuntimeCompositionRepository>,
        host: Arc<IntegrationDriverHost>,
        host_repository: Arc<dyn AgentRuntimeHostRepository>,
        managed_compaction: Option<Arc<dyn ManagedCompactionPreparationEngine>>,
        hook_effects: Arc<dyn RuntimeHookEffectDispatcher>,
        node_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            runtime,
            composition,
            host,
            host_repository,
            managed_compaction,
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
        let engine = self.managed_compaction.as_ref().ok_or_else(|| {
            RuntimeDurableWorkerError::Processing(
                "bound Runtime does not support platform-managed compaction".to_string(),
            )
        })?;
        let binding = self
            .host_repository
            .load_binding(&thread.binding_id)
            .await
            .map_err(|error| RuntimeDurableWorkerError::Store(error.to_string()))?
            .ok_or_else(|| {
                RuntimeDurableWorkerError::InvalidClaim(
                    "Runtime binding does not exist".to_string(),
                )
            })?;
        let instance = self
            .host_repository
            .load_activation_instance(&binding.service_instance_id, binding.driver_generation)
            .await
            .map_err(|error| RuntimeDurableWorkerError::Store(error.to_string()))?
            .ok_or_else(|| {
                RuntimeDurableWorkerError::InvalidClaim(
                    "Runtime activation instance does not exist".to_string(),
                )
            })?;
        let prepared = match engine.compact(&thread, &surface, &instance, work).await {
            Ok(prepared) => prepared,
            Err(RuntimeDurableWorkerError::NoEligibleCompaction) => {
                self.runtime
                    .complete_compaction_without_changes(&work.compaction_id)
                    .await
                    .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let blocks = prepared.blocks;
        let source_item_ids = prepared.source_item_ids;
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
                presentation: Some(prepared.presentation),
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
                        presentation_thread_id: thread.presentation_thread_id.clone(),
                        binding_id: entry.binding_id.clone(),
                        generation: entry.generation,
                        source_thread_id: thread.source_thread_id,
                        runtime_turn_id: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn content(value: serde_json::Value) -> agentdash_agent_runtime_contract::RuntimeItemContent {
        agentdash_agent_runtime_contract::RuntimeItemContent::new(
            serde_json::from_value(value).expect("canonical item"),
        )
    }

    #[test]
    fn managed_compaction_projects_user_tool_result_and_agent_without_ui_flattening() {
        let user = compaction_messages(&content(serde_json::json!({
            "type":"userMessage", "id":"user-1", "content":[{"type":"text","text":"inspect"}]
        })))
        .expect("user");
        let tool = compaction_messages(&content(serde_json::json!({
            "type":"dynamicToolCall", "id":"tool-1", "tool":"workspace_read",
            "arguments":{"path":"README.md"}, "status":"completed",
            "contentItems":[{"type":"inputText","text":"real tool output"}], "success":true
        })))
        .expect("tool");
        let agent = compaction_messages(&content(serde_json::json!({
            "type":"agentMessage", "id":"agent-1", "text":"done"
        })))
        .expect("agent");

        assert!(matches!(user.as_slice(), [AgentMessage::User { .. }]));
        assert!(matches!(tool.as_slice(), [
            AgentMessage::Assistant { tool_calls, .. },
            AgentMessage::ToolResult { content, .. }
        ] if tool_calls[0].name == "workspace_read" && content[0].extract_text() == Some("real tool output")));
        assert!(matches!(agent.as_slice(), [AgentMessage::Assistant { .. }]));
    }

    #[test]
    fn kept_boundary_uses_runtime_item_coordinate_when_tool_expands_to_a_pair() {
        let ids = ["user", "presentation-only", "tool", "agent"]
            .into_iter()
            .map(|id| agentdash_agent_runtime_contract::RuntimeItemId::new(id).expect("item id"))
            .collect::<Vec<_>>();
        let reference = |entry_index| MessageRef {
            turn_id: "turn-1".to_string(),
            entry_index,
        };
        let transcript = vec![
            CompactionTranscriptEntry {
                message: AgentMessage::user("user"),
                reference: reference(0),
                runtime_item_index: 0,
            },
            // item_order[1] is intentionally absent: it is not model-visible.
            CompactionTranscriptEntry {
                message: AgentMessage::assistant("tool call"),
                reference: reference(2),
                runtime_item_index: 2,
            },
            CompactionTranscriptEntry {
                message: AgentMessage::tool_result("tool", "result", false),
                reference: reference(2),
                runtime_item_index: 2,
            },
            CompactionTranscriptEntry {
                message: AgentMessage::assistant("agent"),
                reference: reference(3),
                runtime_item_index: 3,
            },
        ];

        assert_eq!(
            kept_source_item_ids(&ids, &transcript, Some(&reference(2))).expect("boundary"),
            ids[2..].to_vec()
        );
    }
}
