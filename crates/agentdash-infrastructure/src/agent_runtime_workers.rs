use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    PostgresAgentRuntimeCompositionRepository, PostgresAgentRuntimeContextBroker,
    PostgresRuntimeRepository,
};
use agentdash_agent::compaction::execute_compaction;
use agentdash_agent::{
    AgentMessage, CompactionParams, ContentPart, MessageRef, StopReason, ToolCallInfo,
};
use agentdash_agent_protocol::{BackboneEvent, TranscriptProjectionEvent, project_transcript};
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
    CompactionStrategy, CompactionTrigger, CompactionTriggerStats, ProjectedTranscript,
};
use agentdash_diagnostics::{Subsystem, diag};
use agentdash_domain::llm_provider::{
    LlmProviderCredentialRepository, LlmProviderRepository, LlmSecretCodec,
};
use agentdash_integration_api::{
    AgentRuntimeContextBroker, DriverTranscript, DriverTranscriptRequest,
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
        _thread: &agentdash_agent_runtime::RuntimeThreadState,
        surface: &agentdash_integration_api::MaterializedDriverSurface,
        instance: &agentdash_agent_runtime_host::AgentServiceInstance,
        input: &ManagedCompactionInput,
        work: &agentdash_agent_runtime::ContextPreparationWorkItem,
    ) -> Result<ManagedCompactionOutput, RuntimeDurableWorkerError>;
}

#[derive(Debug, Clone)]
pub struct ManagedCompactionInputEntry {
    pub message: AgentMessage,
    pub reference: MessageRef,
    pub kept_source: Option<ManagedCompactionKeptSource>,
}

#[derive(Debug, Clone)]
pub struct ManagedCompactionKeptSource {
    pub key: String,
    pub block: ContextBlock,
    pub runtime_item_id: Option<agentdash_agent_runtime_contract::RuntimeItemId>,
}

#[derive(Debug, Clone, Default)]
pub struct ManagedCompactionInput {
    pub entries: Vec<ManagedCompactionInputEntry>,
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
        _thread: &agentdash_agent_runtime::RuntimeThreadState,
        surface: &agentdash_integration_api::MaterializedDriverSurface,
        instance: &agentdash_agent_runtime_host::AgentServiceInstance,
        input: &ManagedCompactionInput,
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

        let messages = input
            .entries
            .iter()
            .map(|entry| entry.message.clone())
            .collect::<Vec<_>>();
        let refs = input
            .entries
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
        let (kept_blocks, mut source_item_ids) =
            kept_context(input, result.first_kept_ref.as_ref())?;
        let mut blocks = surface
            .context
            .instructions
            .iter()
            .flat_map(|set| set.entries.iter())
            .cloned()
            .map(|text| ContextBlock::Instruction { text })
            .collect::<Vec<_>>();
        blocks.extend(
            surface
                .context
                .blocks
                .iter()
                .filter(|block| matches!(block, ContextBlock::Instruction { .. }))
                .cloned(),
        );
        blocks.push(ContextBlock::CompactionSummary {
            summary: summary.clone(),
        });
        blocks.extend(kept_blocks);
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
                source_end_event_seq: Some(work.source_end_event_sequence.0),
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

fn kept_context(
    input: &ManagedCompactionInput,
    first_kept_ref: Option<&MessageRef>,
) -> Result<
    (
        Vec<ContextBlock>,
        Vec<agentdash_agent_runtime_contract::RuntimeItemId>,
    ),
    RuntimeDurableWorkerError,
> {
    let Some(first_kept_ref) = first_kept_ref else {
        return Ok((Vec::new(), Vec::new()));
    };
    let first_kept_index = input
        .entries
        .iter()
        .position(|entry| &entry.reference == first_kept_ref)
        .ok_or_else(|| {
            RuntimeDurableWorkerError::Processing(
                "managed compaction first-kept reference is outside the frozen source transcript"
                    .to_string(),
            )
        })?;
    let mut seen = BTreeSet::new();
    let mut blocks = Vec::new();
    let mut source_item_ids = Vec::new();
    for source in input
        .entries
        .iter()
        .skip(first_kept_index)
        .filter_map(|entry| entry.kept_source.as_ref())
    {
        if !seen.insert(source.key.clone()) {
            continue;
        }
        blocks.push(source.block.clone());
        if let Some(runtime_item_id) = &source.runtime_item_id {
            source_item_ids.push(runtime_item_id.clone());
        }
    }
    Ok((blocks, source_item_ids))
}

fn managed_compaction_input(
    thread: &agentdash_agent_runtime::RuntimeThreadState,
    surface: &agentdash_integration_api::MaterializedDriverSurface,
    transcript: &DriverTranscript,
    work: &agentdash_agent_runtime::ContextPreparationWorkItem,
) -> Result<ManagedCompactionInput, RuntimeDurableWorkerError> {
    let replay_after = transcript
        .active_compaction_source_end
        .unwrap_or(agentdash_agent_runtime_contract::EventSequence(0));
    if replay_after.0 > work.source_end_event_sequence.0 {
        return Err(RuntimeDurableWorkerError::Processing(format!(
            "active compaction boundary {} is newer than admitted source boundary {}",
            replay_after.0, work.source_end_event_sequence.0
        )));
    }
    if transcript.latest_available.0 < work.source_end_event_sequence.0 {
        return Err(RuntimeDurableWorkerError::Processing(format!(
            "admitted compaction source boundary {} is not durable; latest_available={}",
            work.source_end_event_sequence.0, transcript.latest_available.0
        )));
    }
    if work.source_end_event_sequence.0 > replay_after.0
        && transcript.earliest_available.0 > replay_after.0.saturating_add(1)
    {
        return Err(RuntimeDurableWorkerError::Processing(format!(
            "compaction source transcript has a retention gap after {}: earliest_available={}",
            replay_after.0, transcript.earliest_available.0
        )));
    }
    let has_compaction_base = surface
        .context
        .blocks
        .iter()
        .any(|block| matches!(block, ContextBlock::CompactionSummary { .. }));
    if has_compaction_base != transcript.active_compaction_source_end.is_some() {
        return Err(RuntimeDurableWorkerError::Processing(
            "materialized context and durable transcript disagree about the active compaction base"
                .to_string(),
        ));
    }

    let mut entries = Vec::new();
    if transcript.active_compaction_source_end.is_some() {
        let base_turn_id = format!("compaction-base:{}", work.compaction_id);
        let mut canonical_source_item_ids = surface.context.recipe.source_item_ids.clone();
        for (block_index, block) in surface.context.blocks.iter().enumerate() {
            let (messages, kept_source) = match block {
                ContextBlock::Instruction { .. } => continue,
                ContextBlock::Input { input } => (
                    vec![compaction_input_message(input)],
                    Some(ManagedCompactionKeptSource {
                        key: format!("base:block:{block_index}"),
                        block: block.clone(),
                        runtime_item_id: None,
                    }),
                ),
                ContextBlock::CompactionSummary { summary } => (
                    vec![AgentMessage::compaction_summary(summary.clone(), 0, 0)],
                    None,
                ),
                ContextBlock::RuntimeItem { content } => {
                    let runtime_item_id = canonical_source_item_ids
                        .iter()
                        .position(|runtime_item_id| {
                            thread.items.get(runtime_item_id).is_some_and(|item| {
                                let canonical_content = match &item.phase {
                                    agentdash_agent_runtime::EntityPhase::Terminal(
                                        agentdash_agent_runtime_contract::RuntimeItemTerminal::Completed {
                                            final_content,
                                        },
                                    ) => final_content,
                                    _ => &item.initial_content,
                                };
                                canonical_content == content
                            })
                        })
                        .map(|index| canonical_source_item_ids.remove(index));
                    (
                        compaction_messages(content)?,
                        Some(ManagedCompactionKeptSource {
                            key: runtime_item_id.as_ref().map_or_else(
                                || format!("base:block:{block_index}"),
                                |item_id| format!("base:item:{item_id}"),
                            ),
                            block: block.clone(),
                            runtime_item_id,
                        }),
                    )
                }
            };
            for message in messages {
                entries.push(ManagedCompactionInputEntry {
                    message,
                    reference: MessageRef {
                        turn_id: base_turn_id.clone(),
                        entry_index: u32::try_from(entries.len()).unwrap_or(u32::MAX),
                    },
                    kept_source: kept_source.clone(),
                });
            }
        }
        if !canonical_source_item_ids.is_empty() {
            return Err(RuntimeDurableWorkerError::Processing(format!(
                "active context recipe references {} canonical Runtime items that are absent from its materialized blocks",
                canonical_source_item_ids.len()
            )));
        }
    }

    entries.extend(project_durable_compaction_tail(
        transcript,
        replay_after.0,
        work.source_end_event_sequence.0,
    )?);
    Ok(ManagedCompactionInput { entries })
}

fn project_durable_compaction_tail(
    transcript: &DriverTranscript,
    replay_after: u64,
    source_end_event_sequence: u64,
) -> Result<Vec<ManagedCompactionInputEntry>, RuntimeDurableWorkerError> {
    let mut last_sequence = 0;
    let mut projected_events = Vec::new();
    let mut event_source_keys = BTreeMap::new();
    let mut kept_sources = BTreeMap::new();
    for record in &transcript.records {
        let sequence = record
            .carrier()
            .sequence
            .ok_or_else(|| {
                RuntimeDurableWorkerError::Processing(
                    "durable compaction transcript record is missing its sequence".to_string(),
                )
            })?
            .0;
        if sequence <= last_sequence || sequence > transcript.latest_available.0 {
            return Err(RuntimeDurableWorkerError::Processing(format!(
                "durable compaction transcript sequence {sequence} is outside its ordered window"
            )));
        }
        last_sequence = sequence;
        if sequence <= replay_after || sequence > source_end_event_sequence {
            continue;
        }
        let Some(presentation) = record.as_presentation() else {
            continue;
        };
        if presentation.durability
            != agentdash_agent_runtime_contract::PresentationDurability::Durable
        {
            continue;
        }
        if let Some(source_item_id) = presentation_source_item_id(&presentation.event) {
            let source_key = format!("journal:item:{source_item_id}");
            event_source_keys.insert(sequence, source_key.clone());
            if let Some(block) = presentation_source_block(&presentation.event) {
                kept_sources.insert(
                    source_key.clone(),
                    ManagedCompactionKeptSource {
                        key: source_key,
                        block,
                        // Journal presentation IDs are vendor/session identities. They are not a
                        // canonical Runtime item coordinate and therefore must never enter the
                        // checkpoint recipe's source_item_ids.
                        runtime_item_id: None,
                    },
                );
            }
        }
        projected_events.push(TranscriptProjectionEvent {
            event_seq: sequence,
            turn_id: record
                .carrier()
                .coordinate
                .presentation_turn_id
                .as_ref()
                .map(|turn_id| turn_id.as_str()),
            entry_index: record.carrier().coordinate.source_entry_index,
            event: &presentation.event,
        });
    }
    let projected: ProjectedTranscript = project_transcript(projected_events);
    Ok(projected
        .entries
        .into_iter()
        .map(|entry| {
            let kept_source = entry
                .source_event_seq
                .and_then(|sequence| event_source_keys.get(&sequence))
                .and_then(|source_key| kept_sources.get(source_key))
                .cloned();
            ManagedCompactionInputEntry {
                message: entry.message,
                reference: entry.message_ref,
                kept_source,
            }
        })
        .collect())
}

fn compaction_input_message(
    input: &[agentdash_agent_runtime_contract::RuntimeInput],
) -> AgentMessage {
    AgentMessage::user_parts(
        input
            .iter()
            .map(|input| match input {
                agentdash_agent_runtime_contract::RuntimeInput::Text { text } => {
                    ContentPart::text(text.clone())
                }
                agentdash_agent_runtime_contract::RuntimeInput::Image {
                    data_url,
                    mime_type,
                } => ContentPart::image(mime_type.clone(), data_url.clone()),
                agentdash_agent_runtime_contract::RuntimeInput::FileReference { uri, .. } => {
                    ContentPart::text(uri.clone())
                }
                agentdash_agent_runtime_contract::RuntimeInput::Structured { schema, value } => {
                    ContentPart::text(
                        serde_json::json!({"schema":schema,"value":value}).to_string(),
                    )
                }
            })
            .collect(),
    )
}

fn presentation_source_item_id(event: &BackboneEvent) -> Option<&str> {
    match event {
        BackboneEvent::AgentMessageDelta(delta) => Some(delta.item_id.as_str()),
        BackboneEvent::ReasoningTextDelta(delta) => Some(delta.item_id.as_str()),
        BackboneEvent::ReasoningSummaryDelta(delta) => Some(delta.item_id.as_str()),
        BackboneEvent::UserInputSubmitted(input) => Some(input.item_id.as_str()),
        BackboneEvent::ItemStarted(item) => Some(item.item.id()),
        BackboneEvent::ItemUpdated(item) => Some(item.item.id()),
        BackboneEvent::ItemCompleted(item) => Some(item.item.id()),
        _ => None,
    }
}

fn presentation_source_block(event: &BackboneEvent) -> Option<ContextBlock> {
    use agentdash_agent_protocol::{AgentDashThreadItem, CodexThreadItem};

    let content = match event {
        BackboneEvent::UserInputSubmitted(input) => {
            agentdash_agent_runtime_contract::RuntimeItemContent::new(AgentDashThreadItem::Codex(
                CodexThreadItem::UserMessage {
                    client_id: None,
                    content: input.content.clone(),
                    id: input.item_id.clone(),
                },
            ))
        }
        BackboneEvent::ItemStarted(item) => {
            agentdash_agent_runtime_contract::RuntimeItemContent::new(item.item.clone())
        }
        BackboneEvent::ItemUpdated(item) => {
            agentdash_agent_runtime_contract::RuntimeItemContent::new(item.item.clone())
        }
        BackboneEvent::ItemCompleted(item) => {
            agentdash_agent_runtime_contract::RuntimeItemContent::new(item.item.clone())
        }
        _ => return None,
    };
    Some(ContextBlock::RuntimeItem { content })
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
        let transcript =
            PostgresAgentRuntimeContextBroker::new(self.store.clone(), self.composition.clone())
                .load_transcript(DriverTranscriptRequest {
                    binding_id: thread.binding_id.clone(),
                    generation: thread.driver_generation,
                    runtime_thread_id: thread.thread_id.clone(),
                })
                .await
                .map_err(|error| RuntimeDurableWorkerError::Processing(error.to_string()))?;
        let input = managed_compaction_input(&thread, &surface, &transcript, work)?;
        let prepared = match engine
            .compact(&thread, &surface, &instance, &input, work)
            .await
        {
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
                source_end_event_sequence: work.source_end_event_sequence,
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
                        operation_id: preparation.operation_id.clone(),
                        presentation_thread_id: thread.presentation_thread_id.clone(),
                        binding_id: entry.binding_id.clone(),
                        generation: entry.generation,
                        source_thread_id: thread.source_thread_id,
                        runtime_turn_id: None,
                        presentation_turn_id: None,
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

    fn transcript_record(
        sequence: u64,
        presentation_turn_id: &str,
        entry_index: u32,
        event: BackboneEvent,
    ) -> agentdash_agent_runtime_contract::RuntimeJournalRecord {
        agentdash_agent_runtime_contract::RuntimeJournalRecord::new(
            agentdash_agent_runtime_contract::RuntimeCarrierMetadata {
                thread_id: "runtime-thread-compaction-test"
                    .parse()
                    .expect("runtime thread id"),
                recorded_at_ms: sequence,
                sequence: Some(agentdash_agent_runtime_contract::EventSequence(sequence)),
                transient: None,
                revision: agentdash_agent_runtime_contract::RuntimeRevision(sequence),
                operation_id: None,
                append_idempotency_key: None,
                binding_id: None,
                coordinate: agentdash_agent_runtime_contract::RuntimePresentationCoordinate {
                    runtime_turn_id: None,
                    presentation_turn_id: Some(
                        presentation_turn_id.parse().expect("presentation turn id"),
                    ),
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("source-thread-compaction-test".to_string()),
                    source_turn_id: Some(presentation_turn_id.to_string()),
                    source_item_id: None,
                    source_request_id: None,
                    source_entry_index: Some(entry_index),
                },
            },
            agentdash_agent_runtime_contract::RuntimeJournalFact::Presentation(
                agentdash_agent_runtime_contract::ImmutablePresentationEvent::new(
                    agentdash_agent_runtime_contract::PresentationDurability::Durable,
                    event,
                ),
            ),
        )
        .expect("transcript record")
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
    fn admitted_compaction_projects_only_durable_records_at_or_before_frozen_boundary() {
        use agentdash_agent_protocol::codex_app_server_protocol as codex;
        use agentdash_agent_protocol::{
            ItemCompletedNotification, UserInputSource, UserInputSubmissionKind,
            UserInputSubmittedNotification,
        };

        let user = |turn: &str, item: &str, text: &str| {
            BackboneEvent::UserInputSubmitted(UserInputSubmittedNotification::new(
                "presentation-thread",
                turn,
                item,
                UserInputSubmissionKind::Prompt,
                UserInputSource::core_composer(),
                vec![codex::UserInput::Text {
                    text: text.to_string(),
                    text_elements: Vec::new(),
                }],
            ))
        };
        let tool = BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
            codex::ThreadItem::DynamicToolCall {
                id: "turn_005:tool_009".to_string(),
                tool: "fs_glob".to_string(),
                arguments: serde_json::json!({"pattern":"**/*.rs"}),
                status: codex::DynamicToolCallStatus::Completed,
                content_items: Some(Some(vec![
                    codex::DynamicToolCallOutputContentItem::InputText {
                        text: "src/lib.rs".to_string(),
                    },
                ])),
                duration_ms: None,
                namespace: None,
                success: Some(Some(true)),
            },
            "presentation-thread",
            "turn-before-admission",
        ));
        let transcript = DriverTranscript {
            earliest_available: agentdash_agent_runtime_contract::EventSequence(1),
            latest_available: agentdash_agent_runtime_contract::EventSequence(4),
            active_compaction_source_end: None,
            completed_presentation_item_ids: vec!["turn_005:tool_009".to_string()],
            records: vec![
                transcript_record(
                    1,
                    "turn-before-admission",
                    0,
                    user("turn-before-admission", "user-before", "before admission"),
                ),
                transcript_record(2, "turn-before-admission", 1, tool),
                transcript_record(
                    3,
                    "turn-after-admission",
                    0,
                    user("turn-after-admission", "user-after", "after admission"),
                ),
                transcript_record(
                    4,
                    "turn-after-admission",
                    1,
                    BackboneEvent::ItemCompleted(ItemCompletedNotification::new(
                        codex::ThreadItem::AgentMessage {
                            id: "agent-after".to_string(),
                            text: "after answer".to_string(),
                            phase: None,
                            memory_citation: None,
                        },
                        "presentation-thread",
                        "turn-after-admission",
                    )),
                ),
            ],
        };

        let entries = project_durable_compaction_tail(&transcript, 0, 2)
            .expect("project frozen source transcript");
        assert_eq!(entries.len(), 3, "user + tool call/result only");
        assert!(entries.iter().all(|entry| {
            entry.message.first_text() != Some("after admission")
                && entry.message.first_text() != Some("after answer")
        }));
        assert!(matches!(
            entries.as_slice(),
            [
                ManagedCompactionInputEntry {
                    message: AgentMessage::User { .. },
                    ..
                },
                ManagedCompactionInputEntry {
                    message: AgentMessage::Assistant { tool_calls, .. },
                    kept_source: Some(call_source),
                    ..
                },
                ManagedCompactionInputEntry {
                    message: AgentMessage::ToolResult { tool_call_id, .. },
                    kept_source: Some(result_source),
                    ..
                }
            ] if tool_calls[0].id == "turn_005:tool_009"
                && tool_call_id == "turn_005:tool_009"
                && call_source.key == result_source.key
        ));
    }

    #[test]
    fn kept_boundary_preserves_completed_tool_source_when_projection_expands_to_a_pair() {
        let ids = ["user", "agent"]
            .into_iter()
            .map(|id| agentdash_agent_runtime_contract::RuntimeItemId::new(id).expect("item id"))
            .collect::<Vec<_>>();
        let reference = |entry_index| MessageRef {
            turn_id: "turn-1".to_string(),
            entry_index,
        };
        let user_block = ContextBlock::RuntimeItem {
            content: content(serde_json::json!({
                "type":"userMessage", "id":"user-readable", "content":[{"type":"text","text":"inspect"}]
            })),
        };
        let tool_block = ContextBlock::RuntimeItem {
            content: content(serde_json::json!({
                "type":"dynamicToolCall", "id":"turn_005:tool_009", "tool":"workspace_read",
                "arguments":{"path":"README.md"}, "status":"completed",
                "contentItems":[{"type":"inputText","text":"real tool output"}], "success":true
            })),
        };
        let agent_block = ContextBlock::RuntimeItem {
            content: content(serde_json::json!({
                "type":"agentMessage", "id":"agent-readable", "text":"done"
            })),
        };
        let source =
            |key: &str,
             block: &ContextBlock,
             runtime_item_id: Option<agentdash_agent_runtime_contract::RuntimeItemId>| {
                ManagedCompactionKeptSource {
                    key: key.to_string(),
                    block: block.clone(),
                    runtime_item_id,
                }
            };
        let tool_messages = match &tool_block {
            ContextBlock::RuntimeItem { content } => compaction_messages(content).expect("tool"),
            _ => unreachable!(),
        };
        let input = ManagedCompactionInput {
            entries: vec![
                ManagedCompactionInputEntry {
                    message: AgentMessage::user("user"),
                    reference: reference(0),
                    kept_source: Some(source("base:item:user", &user_block, Some(ids[0].clone()))),
                },
                ManagedCompactionInputEntry {
                    message: tool_messages[0].clone(),
                    reference: reference(2),
                    kept_source: Some(source("journal:item:turn_005:tool_009", &tool_block, None)),
                },
                ManagedCompactionInputEntry {
                    message: tool_messages[1].clone(),
                    reference: reference(2),
                    kept_source: Some(source("journal:item:turn_005:tool_009", &tool_block, None)),
                },
                ManagedCompactionInputEntry {
                    message: AgentMessage::assistant("agent"),
                    reference: reference(3),
                    kept_source: Some(source(
                        "base:item:agent",
                        &agent_block,
                        Some(ids[1].clone()),
                    )),
                },
            ],
        };

        let (blocks, source_item_ids) =
            kept_context(&input, Some(&reference(2))).expect("boundary");
        assert_eq!(blocks, vec![tool_block, agent_block]);
        assert_eq!(source_item_ids, vec![ids[1].clone()]);

        let ContextBlock::RuntimeItem { content } = &blocks[0] else {
            panic!("kept tool source must remain a Runtime item");
        };
        let restored = compaction_messages(content).expect("restore kept tool pair");
        assert!(matches!(restored.as_slice(), [
            AgentMessage::Assistant { tool_calls, .. },
            AgentMessage::ToolResult { tool_call_id, .. }
        ] if tool_calls.len() == 1
            && tool_calls[0].id == "turn_005:tool_009"
            && tool_call_id == "turn_005:tool_009"));
    }
}
