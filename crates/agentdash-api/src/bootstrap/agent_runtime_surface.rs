use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

use agentdash_agent_runtime_contract::*;
use agentdash_application_agentrun::agent_run::{
    AgentFrameHookRuntime, AgentFrameSurfaceExt, BusinessFrameSurfaceQuery,
    RuntimeSurfaceQueryPurpose,
};
use agentdash_application_ports::agent_frame_hook_plan::AgentFrameHookPlan;
use agentdash_application_ports::agent_run_surface::{
    AgentRunAdmissionRequest, AgentRunEffectiveCapabilityPort,
};
use agentdash_application_ports::runtime_surface_adoption::{
    AgentFrameRuntimeTarget, RuntimeSurfaceAdoptionError, RuntimeSurfaceAdoptionPort,
};
use agentdash_domain::common::AgentConfig;
use agentdash_domain::workflow::AgentFrameRepository;
use agentdash_infrastructure::persistence::postgres::PostgresToolBrokerRepository;
use agentdash_integration_api::*;
use agentdash_spi::{
    AgentFrameHookSnapshotQuery, DynAgentTool, ExecutionContext, ExecutionHookProvider,
    ExecutionSessionFrame, ExecutionTurnFrame, HookControlTarget, HookRuntimeEvaluationQuery,
    HookRuntimeRefreshQuery, HookTrigger, RuntimeAdapterProvenance, SharedHookRuntime,
    build_hook_trace_envelope, connector::RuntimeToolProvider,
    hook_trace_entry_storage_disposition,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use super::agent_runtime::{
    AgentRunPlatformToolBrokerResolver, AgentRunRuntimeSurfaceSourceError,
    AppliedNativeAgentRunSurface, NativeAgentRunSurfaceCompiler, NativeAgentRunSurfacePlan,
    NativeAgentRunSurfacePublication, NativeAgentRunSurfacePublicationReservation,
};

#[derive(Clone)]
pub struct CompiledAgentRunToolBinding {
    pub applied: AppliedNativeAgentRunSurface,
    pub runtime_session_id: String,
    pub run_id: uuid::Uuid,
    pub agent_id: uuid::Uuid,
    pub frame_id: uuid::Uuid,
    pub hook_runtime: SharedHookRuntime,
    pub catalog: agentdash_agent_runtime::ToolCatalogRevision,
    pub tools: BTreeMap<String, DynAgentTool>,
    pub terminal_hook_effect_binding: Option<RuntimeTerminalHookEffectBinding>,
}

#[derive(Clone)]
pub(crate) struct PendingCompiledAgentRunToolBinding {
    pub(crate) registry: Arc<CompiledAgentRunToolRegistry>,
    pub(crate) runtime_session_id: String,
    pub(crate) run_id: uuid::Uuid,
    pub(crate) agent_id: uuid::Uuid,
    pub(crate) frame_id: uuid::Uuid,
    pub(crate) hook_runtime: SharedHookRuntime,
    pub(crate) catalog: agentdash_agent_runtime::ToolCatalogRevision,
    pub(crate) tools: BTreeMap<String, DynAgentTool>,
}

impl PendingCompiledAgentRunToolBinding {
    fn applied_binding(
        &self,
        applied: AppliedNativeAgentRunSurface,
    ) -> Result<CompiledAgentRunToolBinding, AgentRunRuntimeSurfaceSourceError> {
        if self.catalog.revision != applied.tool_set_revision {
            return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "compiled tool catalog revision does not match the adopted surface"
                    .to_string(),
            });
        }
        let terminal_hook_effect_binding = applied.terminal_hook_effect_binding.clone();
        Ok(CompiledAgentRunToolBinding {
            applied,
            runtime_session_id: self.runtime_session_id.clone(),
            run_id: self.run_id,
            agent_id: self.agent_id,
            frame_id: self.frame_id,
            hook_runtime: self.hook_runtime.clone(),
            catalog: self.catalog.clone(),
            tools: self.tools.clone(),
            terminal_hook_effect_binding,
        })
    }
}

#[async_trait]
impl NativeAgentRunSurfacePublication for PendingCompiledAgentRunToolBinding {
    async fn reserve(
        &self,
        applied: AppliedNativeAgentRunSurface,
    ) -> Result<
        Box<dyn NativeAgentRunSurfacePublicationReservation>,
        AgentRunRuntimeSurfaceSourceError,
    > {
        let binding = self.applied_binding(applied)?;
        self.registry.reserve(binding).await
    }
}

pub struct CanonicalAgentRuntimeHookCallback {
    runtime: Arc<
        agentdash_agent_runtime::ManagedAgentRuntime<
            agentdash_infrastructure::PostgresRuntimeRepository,
        >,
    >,
    registry: Arc<CompiledAgentRunToolRegistry>,
}

impl CanonicalAgentRuntimeHookCallback {
    pub fn new(
        runtime: Arc<
            agentdash_agent_runtime::ManagedAgentRuntime<
                agentdash_infrastructure::PostgresRuntimeRepository,
            >,
        >,
        registry: Arc<CompiledAgentRunToolRegistry>,
    ) -> Self {
        Self { runtime, registry }
    }

    async fn append_hook_trace(
        &self,
        binding: &CompiledAgentRunToolBinding,
        request: &DriverHookInvocation,
        hook_run_id: &HookRunId,
        entry: &agentdash_spi::HookTraceEntry,
    ) -> Result<(), DriverHookCallbackError> {
        let snapshot = self
            .runtime
            .snapshot(RuntimeSnapshotQuery::Thread {
                thread_id: request.thread_id.clone(),
                at_revision: None,
            })
            .await
            .map_err(hook_callback_error)?;
        let RuntimeSnapshotResult::Thread { snapshot } = snapshot else {
            return Err(DriverHookCallbackError::ProtocolViolation {
                reason: "hook trace projection requires a thread snapshot".into(),
            });
        };
        let Some(durability) = hook_trace_presentation_durability(entry) else {
            return Ok(());
        };
        let Some(envelope) = build_hook_trace_envelope(
            &binding.runtime_session_id,
            request.turn_id.as_ref().map(ToString::to_string).as_deref(),
            agentdash_agent_protocol::SourceInfo {
                connector_id: "agentdash.hook".into(),
                connector_type: "application_hook".into(),
                executor_id: None,
            },
            entry,
        ) else {
            return Ok(());
        };
        let input = RuntimePresentationInput {
            coordinate: RuntimePresentationCoordinate {
                runtime_turn_id: request.turn_id.clone(),
                runtime_item_id: request.item_id.clone(),
                interaction_id: None,
                source_thread_id: Some(binding.runtime_session_id.clone()),
                source_turn_id: snapshot
                    .active_presentation_turn_id
                    .map(|turn_id| turn_id.to_string()),
                source_item_id: request.item_id.as_ref().map(ToString::to_string),
                source_request_id: Some(hook_run_id.to_string()),
                source_entry_index: None,
            },
            event: ImmutablePresentationEvent::new(durability, envelope.event),
        };
        match durability {
            PresentationDurability::Durable => self
                .runtime
                .append_presentation(RuntimePresentationAppendRequest {
                    runtime_thread_id: request.thread_id.clone(),
                    producer: "application.hook_trace".into(),
                    idempotency_key: IdempotencyKey::new(format!("hook-trace:{hook_run_id}"))
                        .map_err(|error| DriverHookCallbackError::ProtocolViolation {
                            reason: error.to_string(),
                        })?,
                    events: vec![input],
                })
                .await
                .map(|_| ())
                .map_err(hook_callback_error),
            PresentationDurability::Ephemeral => self
                .runtime
                .append_transient_presentation(RuntimeTransientPresentationAppendRequest {
                    runtime_thread_id: request.thread_id.clone(),
                    producer: "application.hook_trace".into(),
                    events: vec![input],
                })
                .await
                .map_err(hook_callback_error),
        }
    }
}

fn hook_trace_presentation_durability(
    entry: &agentdash_spi::HookTraceEntry,
) -> Option<PresentationDurability> {
    match hook_trace_entry_storage_disposition(entry) {
        agentdash_spi::HookTraceStorageDisposition::Durable => {
            Some(PresentationDurability::Durable)
        }
        agentdash_spi::HookTraceStorageDisposition::Ephemeral => {
            Some(PresentationDurability::Ephemeral)
        }
        agentdash_spi::HookTraceStorageDisposition::Drop => None,
    }
}

fn canonical_hook_trace_entry(
    hook_runtime: &dyn agentdash_spi::HookRuntimeAccess,
    request: &DriverHookInvocation,
    trigger: agentdash_spi::HookTraceTrigger,
    resolution: &agentdash_spi::HookResolution,
) -> agentdash_spi::HookTraceEntry {
    let decision = match trigger {
        agentdash_spi::HookTraceTrigger::BeforeTool => {
            if resolution.block_reason.is_some() {
                "deny"
            } else if resolution.approval_request.is_some() {
                "ask"
            } else if resolution.rewritten_tool_input.is_some() {
                "rewrite"
            } else {
                "allow"
            }
        }
        agentdash_spi::HookTraceTrigger::AfterTool => {
            if resolution.refresh_snapshot {
                "refresh_requested"
            } else if !resolution.effects.is_empty() {
                "effects_applied"
            } else {
                "noop"
            }
        }
        _ => "noop",
    };
    agentdash_spi::HookTraceEntry {
        sequence: hook_runtime.next_trace_sequence(),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        revision: hook_runtime.revision(),
        trigger,
        decision: decision.into(),
        tool_name: request
            .payload
            .get("tool_name")
            .or_else(|| request.payload.get("toolName"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        tool_call_id: request.item_id.as_ref().map(ToString::to_string),
        subagent_type: None,
        matched_rule_keys: resolution.matched_rule_keys.clone(),
        refresh_snapshot: resolution.refresh_snapshot,
        effects_applied: !resolution.effects.is_empty(),
        block_reason: resolution.block_reason.clone(),
        completion: resolution.completion.clone(),
        diagnostics: resolution.diagnostics.clone(),
        injections: Vec::new(),
    }
}

fn hook_invocation_coordinates_are_current(
    request: &DriverHookInvocation,
    active_turn_id: Option<&RuntimeTurnId>,
) -> bool {
    request.turn_id.as_ref() == active_turn_id
        && request.turn_id.is_some() == request.source_turn_id.is_some()
        && request.item_id.is_some() == request.source_item_id.is_some()
        && request.item_id.as_ref().map(ToString::to_string)
            == request.source_item_id.as_ref().map(ToString::to_string)
}

#[async_trait]
impl AgentRuntimeHookCallback for CanonicalAgentRuntimeHookCallback {
    async fn execute(
        &self,
        request: DriverHookInvocation,
    ) -> Result<DriverHookDecision, DriverHookCallbackError> {
        let binding = self
            .registry
            .get_current_hook(&request)
            .await
            .ok_or(DriverHookCallbackError::Stale)?;
        let snapshot = self
            .runtime
            .snapshot(RuntimeSnapshotQuery::Thread {
                thread_id: request.thread_id.clone(),
                at_revision: None,
            })
            .await
            .map_err(hook_callback_error)?;
        let RuntimeSnapshotResult::Thread { snapshot } = snapshot else {
            return Err(DriverHookCallbackError::Stale);
        };
        if snapshot.binding_id != request.binding_id
            || !hook_invocation_coordinates_are_current(&request, snapshot.active_turn_id.as_ref())
            || snapshot.surface.hook_plan.revision != request.hook_plan_revision
            || snapshot.surface.hook_plan.digest != request.hook_plan_digest
        {
            return Err(DriverHookCallbackError::Stale);
        }
        let trigger = match request.point {
            HookPoint::BeforeTool => HookTrigger::BeforeTool,
            HookPoint::AfterTool => HookTrigger::AfterTool,
            _ => {
                return Err(DriverHookCallbackError::ProtocolViolation {
                    reason: format!("canonical hook route does not support {:?}", request.point),
                });
            }
        };
        let correlation_key = request
            .item_id
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| request.turn_id.as_ref().map(ToString::to_string))
            .unwrap_or_else(|| request.thread_id.to_string());
        let hook_run_id = HookRunId::new(format!(
            "hook-{}-{}",
            request.definition_id, correlation_key
        ))
        .map_err(|error| DriverHookCallbackError::ProtocolViolation {
            reason: error.to_string(),
        })?;
        let admission = self
            .runtime
            .accept_hook(agentdash_agent_runtime::RuntimeHookInvocation {
                hook_run_id: hook_run_id.clone(),
                thread_id: request.thread_id.clone(),
                definition_id: request.definition_id.clone(),
                point: request.point,
                correlation: agentdash_agent_runtime::HookCorrelation {
                    operation_id: None,
                    turn_id: request.turn_id.clone(),
                    item_id: request.item_id.clone(),
                    interaction_id: None,
                },
                input: request.payload.clone(),
            })
            .await
            .map_err(hook_callback_error)?;
        if matches!(
            admission,
            agentdash_agent_runtime::HookAdmission::SilentObserver
        ) {
            return Ok(DriverHookDecision::Continue {
                payload: request.payload,
            });
        }
        self.runtime
            .start_hook(&hook_run_id)
            .await
            .map_err(hook_callback_error)?;
        let resolution = binding
            .hook_runtime
            .evaluate_from_provenance(HookRuntimeEvaluationQuery {
                provenance: RuntimeAdapterProvenance::runtime_session(
                    binding.runtime_session_id.clone(),
                    request.turn_id.as_ref().map(ToString::to_string),
                    "canonical_agent_runtime_hook",
                ),
                trigger,
                tool_name: request
                    .payload
                    .get("tool_name")
                    .or_else(|| request.payload.get("toolName"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                tool_call_id: request.item_id.as_ref().map(ToString::to_string),
                subagent_type: None,
                snapshot: None,
                payload: Some(request.payload.clone()),
                token_stats: None,
            })
            .await
            .map_err(|error| DriverHookCallbackError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?;
        if resolution.refresh_snapshot {
            binding
                .hook_runtime
                .refresh_from_provenance(HookRuntimeRefreshQuery {
                    provenance: RuntimeAdapterProvenance::runtime_session(
                        binding.runtime_session_id.clone(),
                        request.turn_id.as_ref().map(ToString::to_string),
                        "canonical_agent_runtime_hook_refresh",
                    ),
                    reason: Some(format!("trigger:{trigger:?}")),
                })
                .await
                .map_err(|error| DriverHookCallbackError::Unavailable {
                    reason: error.to_string(),
                    retryable: true,
                })?;
        }
        let trace_trigger =
            trigger
                .trace_trigger()
                .ok_or_else(|| DriverHookCallbackError::ProtocolViolation {
                    reason: format!("canonical hook trigger {trigger:?} has no trace projection"),
                })?;
        let trace = canonical_hook_trace_entry(
            binding.hook_runtime.as_ref(),
            &request,
            trace_trigger,
            &resolution,
        );
        binding.hook_runtime.append_trace(trace.clone());
        self.append_hook_trace(&binding, &request, &hook_run_id, &trace)
            .await?;
        if let Some(approval) = resolution.approval_request {
            let interaction_id = RuntimeInteractionId::new(format!("interaction-{hook_run_id}"))
                .map_err(|error| DriverHookCallbackError::ProtocolViolation {
                    reason: error.to_string(),
                })?;
            self.runtime
                .request_hook_interaction(
                    &hook_run_id,
                    interaction_id.clone(),
                    approval.reason.clone(),
                )
                .await
                .map_err(hook_callback_error)?;
            return Ok(DriverHookDecision::InteractionRequired {
                interaction_id,
                reason: approval.reason,
            });
        }
        let effects = resolution
            .effects
            .into_iter()
            .enumerate()
            .map(|(index, effect)| {
                let effect_id = HookEffectId::new(format!("effect-{hook_run_id}-{index}"))
                    .expect("derived hook effect id");
                let payload_digest =
                    agentdash_agent_runtime::hook_effect_payload_digest(&effect.payload);
                agentdash_agent_runtime::HookEffect {
                    effect_id,
                    hook_run_id: hook_run_id.clone(),
                    thread_id: request.thread_id.clone(),
                    idempotency_key: format!("{hook_run_id}:{index}"),
                    descriptor: agentdash_agent_runtime::HookEffectDescriptor {
                        effect_type: effect.kind,
                        schema_version: 1,
                        target_authority: "agentdash_hook_effect_dispatcher".to_string(),
                        retry_limit: 3,
                        payload_digest,
                    },
                    payload: effect.payload,
                }
            })
            .collect::<Vec<_>>();
        if let Some(reason) = resolution.block_reason {
            self.runtime
                .complete_hook(
                    &hook_run_id,
                    agentdash_agent_runtime::HookCompletion {
                        status: agentdash_agent_runtime::HookRunStatus::Blocked,
                        decision: HookRunDecision::Block,
                        message: Some(reason.clone()),
                    },
                    effects,
                )
                .await
                .map_err(hook_callback_error)?;
            return Ok(DriverHookDecision::Block { reason });
        }
        self.runtime
            .complete_hook(
                &hook_run_id,
                agentdash_agent_runtime::HookCompletion {
                    status: agentdash_agent_runtime::HookRunStatus::Completed,
                    decision: HookRunDecision::Continue,
                    message: None,
                },
                effects,
            )
            .await
            .map_err(hook_callback_error)?;
        Ok(DriverHookDecision::Continue {
            payload: resolution.rewritten_tool_input.unwrap_or(request.payload),
        })
    }
}

fn hook_callback_error(error: impl ToString) -> DriverHookCallbackError {
    DriverHookCallbackError::ProtocolViolation {
        reason: error.to_string(),
    }
}

pub struct RegistryToolBrokerPolicy {
    registry: Arc<CompiledAgentRunToolRegistry>,
    capabilities: Arc<dyn AgentRunEffectiveCapabilityPort>,
}

impl RegistryToolBrokerPolicy {
    pub fn new(
        registry: Arc<CompiledAgentRunToolRegistry>,
        capabilities: Arc<dyn AgentRunEffectiveCapabilityPort>,
    ) -> Self {
        Self {
            registry,
            capabilities,
        }
    }
}

#[async_trait]
impl agentdash_agent_runtime::ToolBrokerPolicyPort for RegistryToolBrokerPolicy {
    async fn validate_binding(
        &self,
        invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
    ) -> Result<agentdash_agent_runtime::ToolGuardDecision, agentdash_agent_runtime::ToolBrokerError>
    {
        self.registry
            .get(&invocation.coordinates.binding_id)
            .await
            .ok_or(agentdash_agent_runtime::ToolBrokerError::StaleCoordinates)?;
        Ok(agentdash_agent_runtime::ToolGuardDecision::Allowed(
            agentdash_agent_runtime::ToolPolicyCheck { revision: 1 },
        ))
    }

    async fn authorize_capability(
        &self,
        invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
        tool: &agentdash_agent_runtime::ToolContribution,
    ) -> Result<agentdash_agent_runtime::ToolGuardDecision, agentdash_agent_runtime::ToolBrokerError>
    {
        let binding = self
            .registry
            .get(&invocation.coordinates.binding_id)
            .await
            .ok_or(agentdash_agent_runtime::ToolBrokerError::StaleCoordinates)?;
        let decision = self
            .capabilities
            .admit_tool(AgentRunAdmissionRequest::tool(
                binding.runtime_session_id,
                tool.capability_key.clone(),
                tool.runtime_name.clone(),
                None,
            ))
            .await
            .map_err(|error| {
                agentdash_agent_runtime::ToolBrokerError::Execution(error.to_string())
            })?;
        Ok(if decision.allowed {
            agentdash_agent_runtime::ToolGuardDecision::Allowed(
                agentdash_agent_runtime::ToolPolicyCheck { revision: 1 },
            )
        } else {
            agentdash_agent_runtime::ToolGuardDecision::Denied {
                reason: decision
                    .reason
                    .unwrap_or_else(|| "AgentFrame capability denied the tool".to_string()),
            }
        })
    }

    async fn authorize_permission(
        &self,
        _invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
        _tool: &agentdash_agent_runtime::ToolContribution,
    ) -> Result<
        agentdash_agent_runtime::ToolPermissionDecision,
        agentdash_agent_runtime::ToolBrokerError,
    > {
        Ok(agentdash_agent_runtime::ToolPermissionDecision::Allowed(
            agentdash_agent_runtime::ToolPolicyCheck { revision: 1 },
        ))
    }

    async fn authorize_vfs(
        &self,
        _invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
        _tool: &agentdash_agent_runtime::ToolContribution,
    ) -> Result<agentdash_agent_runtime::ToolGuardDecision, agentdash_agent_runtime::ToolBrokerError>
    {
        Ok(agentdash_agent_runtime::ToolGuardDecision::Allowed(
            agentdash_agent_runtime::ToolPolicyCheck { revision: 1 },
        ))
    }
}

struct EmbeddedToolCredentialResolver;

#[async_trait]
impl agentdash_agent_runtime::ToolCredentialResolver for EmbeddedToolCredentialResolver {
    async fn resolve(
        &self,
        credential_refs: &[String],
    ) -> Result<agentdash_agent_runtime::CredentialMaterial, agentdash_agent_runtime::ToolBrokerError>
    {
        if !credential_refs.is_empty() {
            return Err(agentdash_agent_runtime::ToolBrokerError::Credential(
                "embedded platform tools cannot receive broker credential material".to_string(),
            ));
        }
        Ok(agentdash_agent_runtime::CredentialMaterial::new(
            BTreeMap::new(),
        ))
    }
}

struct RegistryToolExecutor {
    registry: Arc<CompiledAgentRunToolRegistry>,
}

fn project_agent_tool_content(
    content: &[agentdash_agent::ContentPart],
) -> Result<
    Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>,
    agentdash_agent_runtime::ToolBrokerError,
> {
    content
        .iter()
        .map(|part| match part {
            agentdash_agent::ContentPart::Text { text } => Ok(
                agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                    text: text.clone(),
                },
            ),
            agentdash_agent::ContentPart::Image { data, .. } => Ok(
                agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputImage {
                    image_url: data.clone(),
                },
            ),
            agentdash_agent::ContentPart::Reasoning { .. } => {
                Err(agentdash_agent_runtime::ToolBrokerError::Execution(
                    "tool result contains unsupported reasoning content for DynamicToolCall output"
                        .to_string(),
                ))
            }
        })
        .collect()
}

#[async_trait]
impl agentdash_agent_runtime::ToolExecutionPort for RegistryToolExecutor {
    async fn execute(
        &self,
        request: agentdash_agent_runtime::ToolExecutionRequest,
    ) -> Result<agentdash_agent_runtime::ToolBrokerResult, agentdash_agent_runtime::ToolBrokerError>
    {
        let binding = self
            .registry
            .get(&request.invocation.coordinates.binding_id)
            .await
            .ok_or(agentdash_agent_runtime::ToolBrokerError::StaleCoordinates)?;
        let tool = binding
            .tools
            .get(&request.invocation.tool_name)
            .ok_or_else(|| {
                agentdash_agent_runtime::ToolBrokerError::UnknownTool(
                    request.invocation.tool_name.clone(),
                )
            })?;
        let update_projection_error = Arc::new(std::sync::Mutex::new(None::<String>));
        let result = tool
            .execute(
                request.idempotency_key.as_str(),
                request.invocation.arguments,
                request.cancellation,
                Some({
                    let updates = request.updates.clone();
                    let update_projection_error = update_projection_error.clone();
                    Arc::new(move |result: agentdash_agent::AgentToolResult| {
                        let content_items = match project_agent_tool_content(&result.content) {
                            Ok(content_items) => content_items,
                            Err(error) => {
                                if let Ok(mut slot) = update_projection_error.lock()
                                    && slot.is_none()
                                {
                                    *slot = Some(error.to_string());
                                }
                                return;
                            }
                        };
                        if !content_items.is_empty() {
                            let _ = updates.send(content_items);
                        }
                    })
                }),
            )
            .await
            .map_err(|error| {
                agentdash_agent_runtime::ToolBrokerError::Execution(error.to_string())
            })?;
        let projection_error = update_projection_error
            .lock()
            .map_err(|_| {
                agentdash_agent_runtime::ToolBrokerError::Execution(
                    "tool update projection error slot was poisoned".to_string(),
                )
            })?
            .take();
        if let Some(error) = projection_error {
            return Err(agentdash_agent_runtime::ToolBrokerError::Execution(error));
        }
        let content_items = project_agent_tool_content(&result.content)?;
        let serialized_content = serde_json::to_value(&result.content).map_err(|error| {
            agentdash_agent_runtime::ToolBrokerError::Execution(format!(
                "tool result content serialization failed: {error}"
            ))
        })?;
        let serialized_content_items = serde_json::to_value(content_items).map_err(|error| {
            agentdash_agent_runtime::ToolBrokerError::Execution(format!(
                "tool result DynamicToolCall content serialization failed: {error}"
            ))
        })?;
        let mut output = result.details.unwrap_or(serialized_content);
        if let Some(object) = output.as_object_mut() {
            object.insert("content_items".to_string(), serialized_content_items);
        } else {
            output = serde_json::json!({"value":output,"content_items":serialized_content_items});
        }
        Ok(agentdash_agent_runtime::ToolBrokerResult {
            output,
            is_error: result.is_error,
        })
    }
}

pub struct PostgresAgentRunToolBrokerResolver {
    registry: Arc<CompiledAgentRunToolRegistry>,
    repository: Arc<PostgresToolBrokerRepository>,
    journal: Arc<
        agentdash_agent_runtime::ManagedRuntimeToolJournal<
            agentdash_infrastructure::PostgresRuntimeRepository,
        >,
    >,
    policy: Arc<RegistryToolBrokerPolicy>,
    executor: Arc<RegistryToolExecutor>,
}

impl PostgresAgentRunToolBrokerResolver {
    pub fn new(
        pool: sqlx::PgPool,
        runtime: Arc<
            agentdash_agent_runtime::ManagedAgentRuntime<
                agentdash_infrastructure::PostgresRuntimeRepository,
            >,
        >,
        registry: Arc<CompiledAgentRunToolRegistry>,
        capabilities: Arc<dyn AgentRunEffectiveCapabilityPort>,
    ) -> Self {
        Self {
            repository: Arc::new(PostgresToolBrokerRepository::new(pool)),
            journal: Arc::new(agentdash_agent_runtime::ManagedRuntimeToolJournal::new(
                runtime,
            )),
            policy: Arc::new(RegistryToolBrokerPolicy::new(
                registry.clone(),
                capabilities,
            )),
            executor: Arc::new(RegistryToolExecutor {
                registry: registry.clone(),
            }),
            registry,
        }
    }
}

#[async_trait]
impl AgentRunPlatformToolBrokerResolver for PostgresAgentRunToolBrokerResolver {
    async fn resolve(
        &self,
        request: &DriverToolInvocation,
    ) -> Result<agentdash_agent_runtime::PlatformToolBroker, DriverToolCallbackError> {
        let binding = self
            .registry
            .get_revision(&request.binding_id, request.tool_set_revision)
            .await
            .ok_or(DriverToolCallbackError::Stale)?;
        Ok(agentdash_agent_runtime::PlatformToolBroker::new(
            binding.catalog,
            request.binding_id.clone(),
            request.generation,
            agentdash_agent_runtime::PlatformToolBrokerDeps {
                repository: self.repository.clone(),
                journal: self.journal.clone(),
                policy: self.policy.clone(),
                credentials: Arc::new(EmbeddedToolCredentialResolver),
                executor: self.executor.clone(),
            },
        ))
    }
}

#[derive(Default)]
struct CompiledAgentRunToolRegistryState {
    bindings: BTreeMap<(RuntimeBindingId, ToolSetRevision), CompiledAgentRunToolBinding>,
    reservations: BTreeMap<(RuntimeBindingId, ToolSetRevision), CompiledAgentRunToolReservation>,
}

struct CompiledAgentRunToolReservation {
    token: uuid::Uuid,
    binding: CompiledAgentRunToolBinding,
}

#[derive(Default)]
pub struct CompiledAgentRunToolRegistry {
    state: RwLock<CompiledAgentRunToolRegistryState>,
}

struct RegistryPublicationReservation {
    registry: Arc<CompiledAgentRunToolRegistry>,
    key: (RuntimeBindingId, ToolSetRevision),
    token: Option<uuid::Uuid>,
}

#[async_trait]
impl NativeAgentRunSurfacePublicationReservation for RegistryPublicationReservation {
    async fn commit(self: Box<Self>) -> Result<(), AgentRunRuntimeSurfaceSourceError> {
        let Some(token) = self.token else {
            return Ok(());
        };
        let mut state = self.registry.state.write().await;
        let reservation = state
            .reservations
            .remove(&self.key)
            .filter(|reservation| reservation.token == token)
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "binding-scoped tool publication reservation is stale".into(),
            })?;
        state.bindings.insert(self.key.clone(), reservation.binding);
        Ok(())
    }

    async fn abort(self: Box<Self>) {
        let Some(token) = self.token else {
            return;
        };
        let mut state = self.registry.state.write().await;
        if state
            .reservations
            .get(&self.key)
            .is_some_and(|reservation| reservation.token == token)
        {
            state.reservations.remove(&self.key);
        }
    }
}

fn compiled_binding_matches(
    existing: &CompiledAgentRunToolBinding,
    binding: &CompiledAgentRunToolBinding,
) -> bool {
    existing.applied == binding.applied
        && existing.catalog == binding.catalog
        && existing.runtime_session_id == binding.runtime_session_id
        && existing.run_id == binding.run_id
        && existing.agent_id == binding.agent_id
        && existing.frame_id == binding.frame_id
        && existing.tools.keys().eq(binding.tools.keys())
        && existing.hook_runtime.session_id() == binding.hook_runtime.session_id()
        && existing.hook_runtime.control_target() == binding.hook_runtime.control_target()
        && existing.hook_runtime.snapshot() == binding.hook_runtime.snapshot()
        && existing.terminal_hook_effect_binding == binding.terminal_hook_effect_binding
}

impl CompiledAgentRunToolRegistry {
    pub async fn put(
        &self,
        binding: CompiledAgentRunToolBinding,
    ) -> Result<(), AgentRunRuntimeSurfaceSourceError> {
        let mut state = self.state.write().await;
        let key = (binding.applied.binding_id.clone(), binding.catalog.revision);
        if let Some(existing) = state.bindings.get(&key) {
            if !compiled_binding_matches(existing, &binding) {
                return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: "binding-scoped tool catalog is immutable".to_string(),
                });
            }
            return Ok(());
        }
        if state.reservations.contains_key(&key) {
            return Err(AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: "binding-scoped tool publication is already reserved".into(),
                retryable: true,
            });
        }
        state.bindings.insert(key, binding);
        Ok(())
    }

    async fn reserve(
        self: &Arc<Self>,
        binding: CompiledAgentRunToolBinding,
    ) -> Result<
        Box<dyn NativeAgentRunSurfacePublicationReservation>,
        AgentRunRuntimeSurfaceSourceError,
    > {
        let key = (binding.applied.binding_id.clone(), binding.catalog.revision);
        let mut state = self.state.write().await;
        if let Some(existing) = state.bindings.get(&key) {
            if !compiled_binding_matches(existing, &binding) {
                return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: "binding-scoped tool catalog is immutable".into(),
                });
            }
            return Ok(Box::new(RegistryPublicationReservation {
                registry: self.clone(),
                key,
                token: None,
            }));
        }
        if state.reservations.contains_key(&key) {
            return Err(AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: "binding-scoped tool publication is already reserved".into(),
                retryable: true,
            });
        }
        let token = uuid::Uuid::new_v4();
        state.reservations.insert(
            key.clone(),
            CompiledAgentRunToolReservation { token, binding },
        );
        Ok(Box::new(RegistryPublicationReservation {
            registry: self.clone(),
            key,
            token: Some(token),
        }))
    }

    pub async fn get(&self, binding_id: &RuntimeBindingId) -> Option<CompiledAgentRunToolBinding> {
        self.state
            .read()
            .await
            .bindings
            .iter()
            .rev()
            .find(|((candidate, _), _)| candidate == binding_id)
            .map(|(_, binding)| binding.clone())
    }

    pub async fn get_revision(
        &self,
        binding_id: &RuntimeBindingId,
        revision: ToolSetRevision,
    ) -> Option<CompiledAgentRunToolBinding> {
        self.state
            .read()
            .await
            .bindings
            .get(&(binding_id.clone(), revision))
            .cloned()
    }

    pub async fn get_current_hook(
        &self,
        request: &DriverHookInvocation,
    ) -> Option<CompiledAgentRunToolBinding> {
        let binding = self.get(&request.binding_id).await?;
        let applied = &binding.applied;
        (applied.runtime_thread_id == request.thread_id
            && applied.generation == request.generation
            && applied.source_thread_id == request.source_thread_id
            && applied.hook_plan_revision == request.hook_plan_revision
            && applied.hook_plan_digest == request.hook_plan_digest)
            .then_some(binding)
    }

    pub async fn get_applied_surface(
        &self,
        binding_id: &RuntimeBindingId,
        generation: RuntimeDriverGeneration,
        runtime_thread_id: &RuntimeThreadId,
        source_thread_id: &DriverThreadId,
        surface_revision: SurfaceRevision,
        surface_digest: &SurfaceDigest,
    ) -> Option<CompiledAgentRunToolBinding> {
        let binding = self.get(binding_id).await?;
        let applied = &binding.applied;
        (applied.generation == generation
            && &applied.runtime_thread_id == runtime_thread_id
            && &applied.source_thread_id == source_thread_id
            && applied.surface_revision == surface_revision
            && &applied.surface_digest == surface_digest)
            .then_some(binding)
    }
}

pub struct CanonicalRuntimeSurfaceAdopter {
    compiler: Arc<dyn NativeAgentRunSurfaceCompiler>,
    surfaces:
        Arc<dyn agentdash_infrastructure::agent_runtime_composition::AgentRunRuntimeSurfaceStore>,
    bindings:
        Arc<dyn agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository>,
    runtime: Arc<
        agentdash_agent_runtime::ManagedAgentRuntime<
            agentdash_infrastructure::PostgresRuntimeRepository,
        >,
    >,
    tools: Arc<CompiledAgentRunToolRegistry>,
}

impl CanonicalRuntimeSurfaceAdopter {
    pub fn new(
        compiler: Arc<dyn NativeAgentRunSurfaceCompiler>,
        surfaces: Arc<
            dyn agentdash_infrastructure::agent_runtime_composition::AgentRunRuntimeSurfaceStore,
        >,
        bindings: Arc<
            dyn agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingRepository,
        >,
        runtime: Arc<
            agentdash_agent_runtime::ManagedAgentRuntime<
                agentdash_infrastructure::PostgresRuntimeRepository,
            >,
        >,
        tools: Arc<CompiledAgentRunToolRegistry>,
    ) -> Self {
        Self {
            compiler,
            surfaces,
            bindings,
            runtime,
            tools,
        }
    }
}

#[async_trait]
impl RuntimeSurfaceAdoptionPort for CanonicalRuntimeSurfaceAdopter {
    async fn adopt_runtime_surface(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<Vec<DynAgentTool>, RuntimeSurfaceAdoptionError> {
        let binding = self
            .bindings
            .load_by_thread_id(&target.runtime_thread_id)
            .await
            .map_err(|error| RuntimeSurfaceAdoptionError::Failed {
                message: error.to_string(),
            })?
            .ok_or_else(|| RuntimeSurfaceAdoptionError::MissingTarget {
                frame_id: target.frame_id,
                runtime_thread_id: target.runtime_thread_id.clone(),
            })?;
        let request =
            agentdash_application_ports::agent_run_runtime::AgentRunRuntimeProvisionRequest {
                target: binding.target.clone(),
                presentation_thread_id: binding.presentation_thread_id.clone(),
                identity: None,
                backend_selection: None,
                fork: None,
                terminal_hook_effect_binding: binding.surface.terminal_hook_effect_binding.clone(),
            };
        let plan = self
            .compiler
            .compile(&request, &binding.thread_id, &binding.binding_id)
            .await
            .map_err(|error| RuntimeSurfaceAdoptionError::Failed {
                message: error.to_string(),
            })?;
        if plan.source_frame_id != target.frame_id.to_string() {
            return Err(RuntimeSurfaceAdoptionError::MissingTarget {
                frame_id: target.frame_id,
                runtime_thread_id: target.runtime_thread_id,
            });
        }
        let descriptor = RuntimeSurfaceDescriptor {
            source_frame_id: plan.source_frame_id,
            surface_revision: plan.surface.revision,
            surface_digest: plan.surface.digest.clone(),
            vfs_digest: plan.surface.workspace.digest.clone(),
            context_recipe_revision: plan.surface.context.recipe.revision,
            context_digest: plan.surface.context.digest.clone(),
            settings_revision: plan.surface.context.recipe.provenance.settings_revision,
            tool_set_revision: plan.surface.tools.revision,
            tool_set_digest: plan.surface.tools.digest.clone(),
            hook_plan: plan.hook_plan,
            terminal_hook_effect_binding: plan.terminal_hook_effect_binding,
        };
        let applied = AppliedNativeAgentRunSurface {
            runtime_thread_id: binding.thread_id.clone(),
            binding_id: binding.binding_id.clone(),
            generation: binding.driver_generation,
            source_thread_id: binding.source_thread_id.clone(),
            surface_revision: descriptor.surface_revision,
            surface_digest: descriptor.surface_digest.clone(),
            tool_set_revision: descriptor.tool_set_revision,
            hook_plan_revision: descriptor.hook_plan.revision,
            hook_plan_digest: descriptor.hook_plan.digest.clone(),
            terminal_hook_effect_binding: descriptor.terminal_hook_effect_binding.clone(),
        };
        let reservation = plan.publication.reserve(applied).await.map_err(|error| {
            RuntimeSurfaceAdoptionError::Failed {
                message: error.to_string(),
            }
        })?;
        if let Err(error) = self
            .surfaces
            .put_surface(&binding.binding_id, &plan.surface)
            .await
        {
            reservation.abort().await;
            return Err(RuntimeSurfaceAdoptionError::Failed {
                message: error.to_string(),
            });
        }
        let snapshot = match self
            .runtime
            .snapshot(RuntimeSnapshotQuery::Thread {
                thread_id: binding.thread_id.clone(),
                at_revision: None,
            })
            .await
        {
            Ok(RuntimeSnapshotResult::Thread { snapshot }) => snapshot,
            Ok(_) => {
                reservation.abort().await;
                return Err(RuntimeSurfaceAdoptionError::Failed {
                    message: "Runtime surface adoption did not resolve a thread snapshot"
                        .to_string(),
                });
            }
            Err(error) => {
                reservation.abort().await;
                return Err(RuntimeSurfaceAdoptionError::Failed {
                    message: error.to_string(),
                });
            }
        };
        let identity = format!(
            "surface-adopt-{}-{}-{}",
            binding.thread_id, descriptor.surface_revision.0, descriptor.surface_digest
        );
        if let Err(error) = self
            .runtime
            .execute(RuntimeCommandEnvelope {
                presentation: Vec::new(),
                meta: OperationMeta {
                    operation_id: RuntimeOperationId::new(identity.clone())
                        .expect("surface identity is non-empty"),
                    idempotency_key: IdempotencyKey::new(identity)
                        .expect("surface identity is non-empty"),
                    expected_thread_revision: Some(snapshot.revision),
                    actor: RuntimeActor::System {
                        component: "agent_run_runtime_surface_update".to_string(),
                    },
                },
                command: RuntimeCommand::SurfaceAdopt {
                    thread_id: binding.thread_id.clone(),
                    expected_surface_revision: snapshot.surface.surface_revision,
                    expected_surface_digest: snapshot.surface.surface_digest,
                    target: Box::new(descriptor.clone()),
                },
            })
            .await
        {
            reservation.abort().await;
            return Err(RuntimeSurfaceAdoptionError::Failed {
                message: error.to_string(),
            });
        }
        reservation
            .commit()
            .await
            .map_err(|error| RuntimeSurfaceAdoptionError::Failed {
                message: error.to_string(),
            })?;
        self.tools
            .get_revision(&binding.binding_id, descriptor.tool_set_revision)
            .await
            .map(|binding| binding.tools.into_values().collect())
            .ok_or_else(|| RuntimeSurfaceAdoptionError::Failed {
                message: "canonical surface adoption did not publish its compiled tool binding"
                    .to_string(),
            })
    }
}

pub struct AgentFrameNativeSurfaceCompiler {
    surface_query: Arc<BusinessFrameSurfaceQuery>,
    frame_repository: Arc<dyn AgentFrameRepository>,
    runtime_tools: Arc<dyn RuntimeToolProvider>,
    hooks: Arc<dyn ExecutionHookProvider>,
    tool_registry: Arc<CompiledAgentRunToolRegistry>,
}

impl AgentFrameNativeSurfaceCompiler {
    pub fn new(
        surface_query: Arc<BusinessFrameSurfaceQuery>,
        frame_repository: Arc<dyn AgentFrameRepository>,
        runtime_tools: Arc<dyn RuntimeToolProvider>,
        hooks: Arc<dyn ExecutionHookProvider>,
        tool_registry: Arc<CompiledAgentRunToolRegistry>,
    ) -> Self {
        Self {
            surface_query,
            frame_repository,
            runtime_tools,
            hooks,
            tool_registry,
        }
    }
}

#[async_trait]
impl NativeAgentRunSurfaceCompiler for AgentFrameNativeSurfaceCompiler {
    async fn compile(
        &self,
        request: &agentdash_application_ports::agent_run_runtime::AgentRunRuntimeProvisionRequest,
        thread_id: &RuntimeThreadId,
        _binding_id: &RuntimeBindingId,
    ) -> Result<NativeAgentRunSurfacePlan, AgentRunRuntimeSurfaceSourceError> {
        let surface = self
            .surface_query
            .surface_for_provision_target(
                &request.target,
                thread_id,
                RuntimeSurfaceQueryPurpose::new("canonical_agent_runtime_surface"),
            )
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: false,
            })?;
        let frame = self
            .frame_repository
            .get_current(request.target.agent_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "AgentRun has no current AgentFrame".to_string(),
            })?;
        if frame.id != surface.current_surface_frame_id {
            return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "surface query and AgentFrame repository returned different revisions"
                    .to_string(),
            });
        }
        let execution_profile = frame
            .surface
            .as_ref()
            .and_then(|document| document.execution_profile.clone())
            .or_else(|| frame.execution_profile_json.clone())
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "AgentFrame has no execution profile".to_string(),
            })?;
        let executor: AgentConfig = serde_json::from_value(execution_profile).map_err(|error| {
            AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: format!("AgentFrame execution profile is invalid: {error}"),
            }
        })?;
        let executor_id = executor.executor.trim().to_string();
        if executor_id.is_empty() {
            return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "Agent execution profile requires executor".to_string(),
            });
        }
        let provider = executor
            .provider_id
            .clone()
            .filter(|value| !value.trim().is_empty());
        let model = executor
            .model_id
            .clone()
            .filter(|value| !value.trim().is_empty());
        if executor_id == "PI_AGENT" && (provider.is_none() || model.is_none()) {
            return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "Managed Agent execution profile requires provider_id and model_id"
                    .to_string(),
            });
        }
        let working_directory = surface
            .vfs
            .default_mount()
            .map(|mount| PathBuf::from(mount.root_ref.trim()))
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "AgentRun VFS has no usable default mount".to_string(),
            })?;
        let execution_context = ExecutionContext {
            session: ExecutionSessionFrame {
                turn_id: surface.active_turn_id.clone().unwrap_or_else(|| {
                    format!("surface-bootstrap-{}", surface.current_surface_frame_id)
                }),
                working_directory,
                environment_variables: Default::default(),
                executor_config: executor,
                mcp_servers: surface.mcp_servers.clone(),
                vfs: Some(surface.vfs.clone()),
                vfs_access_policy: Some(surface.vfs_access_policy.clone()),
                backend_execution: None,
                runtime_backend_anchor: surface.runtime_backend_anchor.clone(),
                identity: request.identity.clone().or(surface.identity.clone()),
            },
            turn: ExecutionTurnFrame {
                capability_state: surface.capability_state.clone(),
                ..Default::default()
            },
        };
        let tools = self
            .runtime_tools
            .build_tools(&execution_context)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?;
        let revision = u64::try_from(surface.surface_revision).map_err(|_| {
            AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "AgentFrame surface revision must be positive".to_string(),
            }
        })?;
        if revision == 0 {
            return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "AgentFrame surface revision must be positive".to_string(),
            });
        }
        let tool_set_revision = ToolSetRevision(revision);
        let mut direct_tools = BTreeMap::new();
        let mut driver_tools = Vec::new();
        let mut catalog_tools = Vec::new();
        for tool in tools {
            let name = tool.name().trim().to_string();
            if name.is_empty() || direct_tools.insert(name.clone(), tool.clone()).is_some() {
                return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: format!("assembled runtime tool name is empty or duplicated: {name}"),
                });
            }
            let capability_key = capability_for_tool(&surface.capability_state, &name)?;
            let parameters_schema = tool.parameters_schema();
            let protocol = require_tool_protocol_projection(tool.as_ref(), &name)?;
            driver_tools.push(DriverToolDefinition {
                name: name.clone(),
                description: tool.description().to_string(),
                parameters_schema: parameters_schema.clone(),
                channels: vec![ToolChannel::DirectCallback],
                protocol_projection: protocol.projection.clone(),
                parity_fixture_id: protocol.fixture_id.clone(),
            });
            catalog_tools.push(agentdash_agent_runtime::ToolContribution {
                meta: agentdash_agent_runtime::ContributionMeta {
                    key: format!("tool:{capability_key}:{name}"),
                    source: agentdash_agent_runtime::SurfaceSourceRef {
                        layer: "agent_frame".to_string(),
                        key: surface.current_surface_frame_id.to_string(),
                    },
                    priority: 0,
                    requirement: agentdash_agent_runtime::ContributionRequirement::Required,
                },
                runtime_name: name.clone(),
                description: tool.description().to_string(),
                parameters_schema,
                capability_key: capability_key.clone(),
                tool_path: format!("{capability_key}::{name}"),
                allowed_channels: [ToolChannel::DirectCallback].into(),
                configuration_boundary: ConfigurationBoundary::Binding,
                protocol_projection: protocol.projection,
                presentation_emitter:
                    agentdash_agent_runtime_contract::ToolPresentationEmitter::ToolBroker,
                parity_fixture_id: protocol.fixture_id,
            });
        }
        driver_tools.sort_by(|left, right| left.name.cmp(&right.name));
        catalog_tools.sort_by(|left, right| left.runtime_name.cmp(&right.runtime_name));
        let catalog_digest = digest_json(&(tool_set_revision, &catalog_tools))?;
        let catalog = agentdash_agent_runtime::ToolCatalogRevision {
            revision: tool_set_revision,
            digest: catalog_digest.clone(),
            tools: catalog_tools,
            mcp_servers: Vec::new(),
        };
        let hook_snapshot = self
            .hooks
            .load_frame_snapshot(AgentFrameHookSnapshotQuery {
                target: HookControlTarget {
                    run_id: request.target.run_id,
                    agent_id: request.target.agent_id,
                    frame_id: frame.id,
                },
                provenance: RuntimeAdapterProvenance::runtime_session(
                    surface.runtime_session_id.clone(),
                    surface.active_turn_id.clone(),
                    "canonical_agent_runtime_surface",
                ),
            })
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?;
        let hook_runtime: SharedHookRuntime = Arc::new(AgentFrameHookRuntime::new(
            request.target.run_id,
            request.target.agent_id,
            frame.id,
            frame.revision,
            surface.runtime_session_id.clone(),
            self.hooks.clone(),
            hook_snapshot.clone(),
        ));
        let publication: Arc<dyn NativeAgentRunSurfacePublication> =
            Arc::new(PendingCompiledAgentRunToolBinding {
                registry: self.tool_registry.clone(),
                runtime_session_id: surface.runtime_session_id.clone(),
                run_id: request.target.run_id,
                agent_id: request.target.agent_id,
                frame_id: frame.id,
                hook_runtime,
                catalog,
                tools: direct_tools,
            });
        let instructions = hook_snapshot
            .injections
            .iter()
            .map(|injection| injection.content.clone())
            .collect::<Vec<_>>();
        let recipe = ContextRecipe {
            revision: ContextRecipeRevision(revision),
            provenance: ContextProvenance {
                settings_revision: ThreadSettingsRevision(0),
                tool_set_revision,
            },
            source_item_ids: Vec::new(),
        };
        let blocks = initial_driver_context_blocks();
        let context_digest = ContextDigest::new(digest_json(&(&recipe, &instructions, &blocks))?)
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: error.to_string(),
        })?;
        let hook_plan = frame
            .validated_hook_plan()
            .map_err(|reason| AgentRunRuntimeSurfaceSourceError::Invalid { reason })?;
        let (runtime_hook_plan, hook_bindings, hook_configuration_boundary) =
            materialize_hook_plan(&hook_plan);
        let hook_digest = hook_plan.digest;
        let workspace_capabilities = workspace_capabilities(&surface.vfs);
        let workspace_roots = surface
            .vfs
            .mounts
            .iter()
            .map(|mount| mount.root_ref.clone())
            .collect::<Vec<_>>();
        let workspace_digest = digest_json(&(&workspace_capabilities, &workspace_roots))?;
        let surface_revision = SurfaceRevision(revision);
        let surface_digest = SurfaceDigest::new(digest_json(&(
            surface_revision,
            &context_digest,
            &catalog_digest,
            &hook_digest,
            &workspace_capabilities,
            &workspace_roots,
        ))?)
        .map_err(|error| AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: error.to_string(),
        })?;
        Ok(NativeAgentRunSurfacePlan {
            source_frame_id: frame.id.to_string(),
            executor: executor_id,
            provider,
            model,
            hook_plan: runtime_hook_plan,
            publication,
            terminal_hook_effect_binding: request.terminal_hook_effect_binding.clone(),
            surface: MaterializedDriverSurface {
                runtime_thread_id: thread_id.clone(),
                revision: surface_revision,
                digest: surface_digest,
                authorization_identity: request.identity.clone().or(surface.identity),
                context: DriverContextSurface {
                    recipe,
                    instructions: vec![DriverInstructionSet {
                        channel: InstructionChannel::System,
                        entries: instructions,
                    }],
                    blocks,
                    digest: context_digest,
                    fidelity: ContextFidelity::PlatformExact,
                },
                tools: DriverToolSurface {
                    revision: tool_set_revision,
                    digest: catalog_digest,
                    tools: driver_tools,
                },
                hooks: DriverHookSurface {
                    revision: hook_plan.revision,
                    digest: hook_digest,
                    artifact_digest: None,
                    configuration_boundary: hook_configuration_boundary,
                    bindings: hook_bindings,
                },
                workspace: DriverWorkspaceSurface {
                    digest: workspace_digest,
                    capabilities: workspace_capabilities,
                    roots: workspace_roots,
                },
            },
        })
    }
}

fn initial_driver_context_blocks() -> Vec<ContextBlock> {
    // AgentFrame context_slice is control-plane bundle metadata, not model input. Main delivers
    // launch instructions through the dedicated system channel and starts with no replay blocks.
    Vec::new()
}

fn materialize_hook_plan(
    hook_plan: &AgentFrameHookPlan,
) -> (
    BoundRuntimeHookPlan,
    Vec<DriverHookBinding>,
    ConfigurationBoundary,
) {
    let runtime_hook_plan = BoundRuntimeHookPlan {
        revision: hook_plan.revision,
        digest: hook_plan.digest.clone(),
        entries: hook_plan
            .requirements
            .iter()
            .map(|entry| BoundRuntimeHookEntry {
                definition_id: entry.definition_id.clone(),
                point: entry.requirement.point,
                actions: entry.requirement.actions.clone(),
                delivered_strength: entry.requirement.minimum_strength,
                failure_policy: entry.requirement.failure_policy,
                required: entry.requirement.required,
                site: entry.site,
            })
            .collect(),
    };
    let bindings = hook_plan
        .requirements
        .iter()
        .filter(|entry| {
            matches!(
                entry.site,
                HookExecutionSite::AgentCoreCallback | HookExecutionSite::DriverNative
            )
        })
        .map(|entry| DriverHookBinding {
            definition_id: entry.definition_id.clone(),
            point: entry.requirement.point,
            actions: entry.requirement.actions.iter().copied().collect(),
            strength: entry.requirement.minimum_strength,
            failure_policy: entry.requirement.failure_policy,
            required: entry.requirement.required,
            site: entry.site,
        })
        .collect::<Vec<_>>();
    let boundary =
        bindings
            .iter()
            .fold(ConfigurationBoundary::StaticService, |boundary, binding| {
                boundary.max(match binding.site {
                    HookExecutionSite::AgentCoreCallback => ConfigurationBoundary::Binding,
                    HookExecutionSite::DriverNative => ConfigurationBoundary::ThreadStart,
                    _ => ConfigurationBoundary::StaticService,
                })
            });
    (runtime_hook_plan, bindings, boundary)
}

fn capability_for_tool(
    state: &agentdash_spi::CapabilityState,
    tool_name: &str,
) -> Result<String, AgentRunRuntimeSurfaceSourceError> {
    use agentdash_spi::platform::tool_capability::{ToolSource, platform_tool_descriptors};

    let descriptors = platform_tool_descriptors()
        .into_iter()
        .filter(|descriptor| descriptor.name == tool_name)
        .collect::<Vec<_>>();
    let mut candidates = descriptors
        .iter()
        .filter(|descriptor| {
            let cluster = match &descriptor.source {
                ToolSource::Platform { cluster } => Some(*cluster),
                ToolSource::PlatformMcp { .. } | ToolSource::Mcp { .. } => None,
            };
            state.is_capability_tool_enabled(&descriptor.capability_key, tool_name, cluster)
        })
        .map(|descriptor| descriptor.capability_key.clone())
        .collect::<BTreeSet<_>>();

    candidates.extend(
        state
            .tool
            .tool_policy
            .iter()
            .filter(|(key, filter)| {
                filter.allows(tool_name)
                    && state
                        .tool
                        .capabilities
                        .contains(&agentdash_spi::ToolCapability::new((*key).clone()))
            })
            .map(|(key, _)| key.clone()),
    );

    if candidates.len() == 1 {
        return Ok(candidates
            .into_iter()
            .next()
            .expect("one capability candidate"));
    }
    if !descriptors.is_empty() && candidates.is_empty() {
        return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: format!(
                "assembled tool `{tool_name}` is not enabled by current AgentFrame capability"
            ),
        });
    }
    Err(AgentRunRuntimeSurfaceSourceError::Invalid {
        reason: format!(
            "assembled tool `{tool_name}` has no unambiguous AgentFrame capability identity"
        ),
    })
}

fn workspace_capabilities(vfs: &agentdash_spi::Vfs) -> Vec<WorkspaceCapability> {
    let mut values = BTreeSet::new();
    for mount in &vfs.mounts {
        for capability in &mount.capabilities {
            match capability {
                agentdash_domain::common::MountCapability::Read
                | agentdash_domain::common::MountCapability::List => {
                    values.insert(WorkspaceCapability::Read);
                }
                agentdash_domain::common::MountCapability::Search => {
                    values.insert(WorkspaceCapability::Search);
                }
                agentdash_domain::common::MountCapability::Write => {
                    values.insert(WorkspaceCapability::Write);
                }
                agentdash_domain::common::MountCapability::Exec
                | agentdash_domain::common::MountCapability::Watch => {}
            }
        }
    }
    if vfs.mounts.len() > 1 {
        values.insert(WorkspaceCapability::MultipleRoots);
    }
    values.insert(WorkspaceCapability::VirtualFileSystem);
    values.into_iter().collect()
}

fn digest_json(value: &impl serde::Serialize) -> Result<String, AgentRunRuntimeSurfaceSourceError> {
    let value = serde_json::to_value(value).map_err(|error| {
        AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: error.to_string(),
        }
    })?;
    let bytes = agentdash_agent_runtime_host::canonical_json(&value);
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

#[derive(Debug)]
struct AdmittedToolProjection {
    projection: agentdash_agent_runtime_contract::ToolProtocolProjection,
    fixture_id: String,
}

fn require_tool_protocol_projection(
    tool: &dyn agentdash_agent::AgentTool,
    name: &str,
) -> Result<AdmittedToolProjection, AgentRunRuntimeSurfaceSourceError> {
    let projection =
        tool.protocol_projector()
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: format!(
                    "assembled runtime tool `{name}` has no owner-declared protocol projector"
                ),
            })?;
    let fixture_id = tool
        .protocol_fixture_id()
        .filter(|fixture| !fixture.trim().is_empty())
        .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: format!(
                "assembled runtime tool `{name}` has no owner-declared main parity fixture"
            ),
        })?
        .to_string();
    let projection = match projection {
        agentdash_agent::ToolProtocolProjector::Command => {
            agentdash_agent_runtime_contract::ToolProtocolProjection::Command
        }
        agentdash_agent::ToolProtocolProjector::FileChange => {
            agentdash_agent_runtime_contract::ToolProtocolProjection::FileChange
        }
        agentdash_agent::ToolProtocolProjector::FsRead => {
            agentdash_agent_runtime_contract::ToolProtocolProjection::FsRead
        }
        agentdash_agent::ToolProtocolProjector::FsGrep => {
            agentdash_agent_runtime_contract::ToolProtocolProjection::FsGrep
        }
        agentdash_agent::ToolProtocolProjector::FsGlob => {
            agentdash_agent_runtime_contract::ToolProtocolProjection::FsGlob
        }
        agentdash_agent::ToolProtocolProjector::Mcp { server_key } => {
            agentdash_agent_runtime_contract::ToolProtocolProjection::Mcp { server_key }
        }
        agentdash_agent::ToolProtocolProjector::Dynamic { namespace } => {
            agentdash_agent_runtime_contract::ToolProtocolProjection::Dynamic { namespace }
        }
    };
    Ok(AdmittedToolProjection {
        projection,
        fixture_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_agent_runtime::ToolExecutionPort;
    use agentdash_application_ports::agent_frame_hook_plan::AgentFrameHookRequirement;
    use agentdash_spi::{
        CapabilityState, NoopExecutionHookProvider, ToolCapability, ToolCapabilityFilter,
        ToolCluster,
        platform::tool_capability::{CAP_FILE_READ, CAP_WORKSPACE_MODULE},
    };

    fn fixture_hook_runtime(session_id: &str) -> SharedHookRuntime {
        Arc::new(AgentFrameHookRuntime::new(
            uuid::Uuid::nil(),
            uuid::Uuid::nil(),
            uuid::Uuid::nil(),
            1,
            session_id.to_string(),
            Arc::new(NoopExecutionHookProvider),
            agentdash_spi::AgentFrameHookSnapshot::default(),
        ))
    }

    fn fixture_applied(binding_id: &str, revision: u64) -> AppliedNativeAgentRunSurface {
        AppliedNativeAgentRunSurface {
            runtime_thread_id: RuntimeThreadId::new(format!("thread-{binding_id}")).unwrap(),
            binding_id: RuntimeBindingId::new(binding_id).unwrap(),
            generation: RuntimeDriverGeneration(1),
            source_thread_id: DriverThreadId::new(format!("source-{binding_id}")).unwrap(),
            surface_revision: SurfaceRevision(revision),
            surface_digest: SurfaceDigest::new(format!("surface-{binding_id}-{revision}")).unwrap(),
            tool_set_revision: ToolSetRevision(revision),
            hook_plan_revision: HookPlanRevision(revision),
            hook_plan_digest: HookPlanDigest::new(format!("hook-{binding_id}-{revision}")).unwrap(),
            terminal_hook_effect_binding: None,
        }
    }

    fn fixture_terminal_hook_effect_binding() -> RuntimeTerminalHookEffectBinding {
        RuntimeTerminalHookEffectBinding {
            handler: RuntimeTerminalHookEffectHandlerRef {
                handler_type: RuntimeTerminalHookEffectHandlerType::new("agent_run_post_turn")
                    .unwrap(),
                handler_id: RuntimeTerminalHookEffectHandlerId::new("handler-fixture").unwrap(),
                revision: RuntimeTerminalHookEffectHandlerRevision(7),
            },
            supported_effect_kinds: BTreeSet::from([RuntimeHookEffectKind::new(
                "agent_run_control_effect",
            )
            .unwrap()]),
        }
    }

    #[test]
    fn registry_tool_projection_rejects_unrepresentable_content_parts() {
        let error = project_agent_tool_content(&[agentdash_agent::ContentPart::reasoning(
            "private reasoning",
            None,
            None,
        )])
        .expect_err("reasoning content must not be silently discarded");
        assert!(
            error.to_string().contains("unsupported reasoning content"),
            "{error}"
        );
    }

    struct MalformedUpdateTool;

    #[async_trait]
    impl agentdash_agent::AgentTool for MalformedUpdateTool {
        fn name(&self) -> &str {
            "malformed_update"
        }

        fn description(&self) -> &str {
            "fixture"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }

        async fn execute(
            &self,
            _: &str,
            _: serde_json::Value,
            _: tokio_util::sync::CancellationToken,
            on_update: Option<agentdash_agent::ToolUpdateCallback>,
        ) -> Result<agentdash_agent::AgentToolResult, agentdash_agent::AgentToolError> {
            let update = on_update.expect("update callback");
            update(agentdash_agent::AgentToolResult {
                content: vec![agentdash_agent::ContentPart::text("legal update")],
                is_error: false,
                details: None,
            });
            update(agentdash_agent::AgentToolResult {
                content: vec![agentdash_agent::ContentPart::reasoning(
                    "unrepresentable update",
                    None,
                    None,
                )],
                is_error: false,
                details: None,
            });
            Ok(agentdash_agent::AgentToolResult {
                content: vec![agentdash_agent::ContentPart::text("terminal result")],
                is_error: false,
                details: None,
            })
        }
    }

    #[tokio::test]
    async fn malformed_tool_update_fails_terminal_without_diagnostic_content_item() {
        let binding_id = RuntimeBindingId::new("binding-malformed-update").unwrap();
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        registry
            .put(CompiledAgentRunToolBinding {
                applied: fixture_applied("binding-malformed-update", 1),
                runtime_session_id: "session-malformed-update".into(),
                run_id: uuid::Uuid::nil(),
                agent_id: uuid::Uuid::nil(),
                frame_id: uuid::Uuid::nil(),
                hook_runtime: fixture_hook_runtime("session-malformed-update"),
                catalog: agentdash_agent_runtime::ToolCatalogRevision {
                    revision: ToolSetRevision(1),
                    digest: "malformed-update".into(),
                    tools: Vec::new(),
                    mcp_servers: Vec::new(),
                },
                tools: BTreeMap::from([(
                    "malformed_update".to_string(),
                    Arc::new(MalformedUpdateTool) as DynAgentTool,
                )]),
                terminal_hook_effect_binding: None,
            })
            .await
            .unwrap();
        let (updates, mut updates_rx) = tokio::sync::mpsc::unbounded_channel();
        let error = RegistryToolExecutor { registry }
            .execute(agentdash_agent_runtime::ToolExecutionRequest {
                idempotency_key: RuntimeItemId::new("malformed-update-item").unwrap(),
                invocation: agentdash_agent_runtime::ToolBrokerInvocation {
                    coordinates: agentdash_agent_runtime::ToolCallCoordinates {
                        thread_id: RuntimeThreadId::new("malformed-update-thread").unwrap(),
                        turn_id: RuntimeTurnId::new("malformed-update-turn").unwrap(),
                        item_id: RuntimeItemId::new("malformed-update-item").unwrap(),
                        binding_id,
                        binding_generation: RuntimeDriverGeneration(1),
                        tool_set_revision: ToolSetRevision(1),
                    },
                    tool_name: "malformed_update".into(),
                    arguments: serde_json::json!({}),
                    timeout_ms: 1_000,
                },
                credentials: agentdash_agent_runtime::CredentialMaterial::new(BTreeMap::new()),
                cancellation: tokio_util::sync::CancellationToken::new(),
                updates,
            })
            .await
            .expect_err("malformed update must fail the terminal execution");
        assert!(error.to_string().contains("unsupported reasoning content"));
        assert_eq!(
            updates_rx.recv().await,
            Some(vec![
                agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                    text: "legal update".to_string(),
                }
            ])
        );
        assert!(
            updates_rx.try_recv().is_err(),
            "projection failure must not be converted into a diagnostic content item"
        );
    }

    #[tokio::test]
    async fn failed_surface_stages_leave_the_applied_registry_snapshot_unchanged() {
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        let first_runtime = fixture_hook_runtime("presentation-applied");
        registry
            .put(CompiledAgentRunToolBinding {
                applied: fixture_applied("binding-atomic-adopt", 1),
                runtime_session_id: "presentation-applied".into(),
                run_id: uuid::Uuid::nil(),
                agent_id: uuid::Uuid::nil(),
                frame_id: uuid::Uuid::nil(),
                hook_runtime: first_runtime.clone(),
                catalog: agentdash_agent_runtime::ToolCatalogRevision {
                    revision: ToolSetRevision(1),
                    digest: "catalog-applied".into(),
                    tools: Vec::new(),
                    mcp_servers: Vec::new(),
                },
                tools: BTreeMap::new(),
                terminal_hook_effect_binding: None,
            })
            .await
            .unwrap();
        let pending = PendingCompiledAgentRunToolBinding {
            registry: registry.clone(),
            runtime_session_id: "presentation-pending".into(),
            run_id: uuid::Uuid::nil(),
            agent_id: uuid::Uuid::nil(),
            frame_id: uuid::Uuid::nil(),
            hook_runtime: fixture_hook_runtime("presentation-pending"),
            catalog: agentdash_agent_runtime::ToolCatalogRevision {
                revision: ToolSetRevision(2),
                digest: "catalog-pending".into(),
                tools: Vec::new(),
                mcp_servers: Vec::new(),
            },
            tools: BTreeMap::new(),
        };

        for _failure in [
            "injected put_surface failure",
            "injected SurfaceAdopt failure",
        ] {
            let reservation = pending
                .reserve(fixture_applied("binding-atomic-adopt", 2))
                .await
                .unwrap();
            reservation.abort().await;
            let applied = registry
                .get(&RuntimeBindingId::new("binding-atomic-adopt").unwrap())
                .await
                .unwrap();
            assert_eq!(applied.catalog.revision, ToolSetRevision(1));
            assert!(Arc::ptr_eq(&applied.hook_runtime, &first_runtime));
        }
    }

    #[tokio::test]
    async fn concurrent_conflicting_publication_is_rejected_before_adoption() {
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        let pending = |session: &str| PendingCompiledAgentRunToolBinding {
            registry: registry.clone(),
            runtime_session_id: session.into(),
            run_id: uuid::Uuid::nil(),
            agent_id: uuid::Uuid::nil(),
            frame_id: uuid::Uuid::nil(),
            hook_runtime: fixture_hook_runtime(session),
            catalog: agentdash_agent_runtime::ToolCatalogRevision {
                revision: ToolSetRevision(1),
                digest: "catalog-reservation".into(),
                tools: Vec::new(),
                mcp_servers: Vec::new(),
            },
            tools: BTreeMap::new(),
        };
        let first = pending("presentation-reservation-a");
        let conflicting = pending("presentation-reservation-b");
        let applied = fixture_applied("binding-reservation", 1);

        let reservation = first.reserve(applied.clone()).await.unwrap();
        assert!(conflicting.reserve(applied).await.is_err());
        assert!(
            registry
                .get(&RuntimeBindingId::new("binding-reservation").unwrap())
                .await
                .is_none()
        );
        reservation.commit().await.unwrap();
        assert_eq!(
            registry
                .get(&RuntimeBindingId::new("binding-reservation").unwrap())
                .await
                .unwrap()
                .runtime_session_id,
            "presentation-reservation-a"
        );
    }

    fn hook_trace_entry(decision: &str) -> agentdash_spi::HookTraceEntry {
        agentdash_spi::HookTraceEntry {
            sequence: 1,
            timestamp_ms: 1_783_684_800_000,
            revision: 2,
            trigger: agentdash_spi::HookTraceTrigger::BeforeTool,
            decision: decision.into(),
            tool_name: Some("workspace_present".into()),
            tool_call_id: Some("tool-call-1".into()),
            subagent_type: None,
            matched_rule_keys: Vec::new(),
            refresh_snapshot: false,
            effects_applied: false,
            block_reason: None,
            completion: None,
            diagnostics: Vec::new(),
            injections: Vec::new(),
        }
    }

    #[test]
    fn canonical_hook_owner_preserves_main_durable_ephemeral_drop_disposition() {
        let mut durable = hook_trace_entry("deny");
        durable.block_reason = Some("policy denied".into());
        let mut ephemeral = hook_trace_entry("allow");
        ephemeral.matched_rule_keys = vec!["workflow.before_tool".into()];
        let dropped = hook_trace_entry("allow");

        assert_eq!(
            hook_trace_presentation_durability(&durable),
            Some(PresentationDurability::Durable)
        );
        assert_eq!(
            hook_trace_presentation_durability(&ephemeral),
            Some(PresentationDurability::Ephemeral)
        );
        assert_eq!(hook_trace_presentation_durability(&dropped), None);

        let envelope = build_hook_trace_envelope(
            "presentation-thread-1",
            Some("turn-1"),
            agentdash_agent_protocol::SourceInfo {
                connector_id: "agentdash.hook".into(),
                connector_type: "application_hook".into(),
                executor_id: None,
            },
            &ephemeral,
        )
        .expect("Main ephemeral hook body");
        let carried = ImmutablePresentationEvent::new(
            PresentationDurability::Ephemeral,
            envelope.event.clone(),
        );
        assert_eq!(carried.event, envelope.event);
    }

    #[tokio::test]
    async fn registry_exact_replay_is_idempotent_and_conflicting_replay_is_rejected() {
        let registry = CompiledAgentRunToolRegistry::default();
        let first_runtime = fixture_hook_runtime("presentation-thread-1");
        let run_id = uuid::Uuid::nil();
        let agent_id = uuid::Uuid::nil();
        let frame_id = uuid::Uuid::nil();
        let binding = |hook_runtime| CompiledAgentRunToolBinding {
            applied: fixture_applied("binding-refresh", 1),
            runtime_session_id: "presentation-thread-1".into(),
            run_id,
            agent_id,
            frame_id,
            hook_runtime,
            catalog: agentdash_agent_runtime::ToolCatalogRevision {
                revision: ToolSetRevision(1),
                digest: "catalog-1".into(),
                tools: Vec::new(),
                mcp_servers: Vec::new(),
            },
            tools: BTreeMap::new(),
            terminal_hook_effect_binding: None,
        };
        registry.put(binding(first_runtime.clone())).await.unwrap();
        assert_eq!(first_runtime.next_trace_sequence(), 1);
        let replay_revision = first_runtime.revision();

        registry.put(binding(first_runtime.clone())).await.unwrap();
        let conflicting = binding(fixture_hook_runtime("different-presentation-thread"));
        assert!(registry.put(conflicting).await.is_err());
        let rebound = registry
            .get(&RuntimeBindingId::new("binding-refresh").unwrap())
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&first_runtime, &rebound.hook_runtime));
        assert_eq!(rebound.hook_runtime.revision(), replay_revision);
    }

    #[tokio::test]
    async fn registry_preserves_typed_terminal_effect_binding_at_exact_surface_coordinates() {
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        let terminal_binding = fixture_terminal_hook_effect_binding();
        let mut applied = fixture_applied("binding-terminal-effect", 3);
        applied.terminal_hook_effect_binding = Some(terminal_binding.clone());
        let pending = PendingCompiledAgentRunToolBinding {
            registry: registry.clone(),
            runtime_session_id: "presentation-terminal-effect".into(),
            run_id: uuid::Uuid::nil(),
            agent_id: uuid::Uuid::nil(),
            frame_id: uuid::Uuid::nil(),
            hook_runtime: fixture_hook_runtime("presentation-terminal-effect"),
            catalog: agentdash_agent_runtime::ToolCatalogRevision {
                revision: applied.tool_set_revision,
                digest: "catalog-terminal-effect".into(),
                tools: Vec::new(),
                mcp_servers: Vec::new(),
            },
            tools: BTreeMap::new(),
        };

        pending
            .reserve(applied.clone())
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();

        let exact = registry
            .get_applied_surface(
                &applied.binding_id,
                applied.generation,
                &applied.runtime_thread_id,
                &applied.source_thread_id,
                applied.surface_revision,
                &applied.surface_digest,
            )
            .await
            .unwrap();
        assert_eq!(
            exact.applied.terminal_hook_effect_binding,
            Some(terminal_binding.clone())
        );
        assert_eq!(exact.terminal_hook_effect_binding, Some(terminal_binding));
        assert!(
            registry
                .get_applied_surface(
                    &applied.binding_id,
                    RuntimeDriverGeneration(applied.generation.0 + 1),
                    &applied.runtime_thread_id,
                    &applied.source_thread_id,
                    applied.surface_revision,
                    &applied.surface_digest,
                )
                .await
                .is_none()
        );
    }

    fn fixture_hook_request(applied: &AppliedNativeAgentRunSurface) -> DriverHookInvocation {
        DriverHookInvocation {
            thread_id: applied.runtime_thread_id.clone(),
            turn_id: Some(RuntimeTurnId::new("runtime-turn-1").unwrap()),
            item_id: Some(RuntimeItemId::new("item-1").unwrap()),
            binding_id: applied.binding_id.clone(),
            generation: applied.generation,
            hook_plan_revision: applied.hook_plan_revision,
            hook_plan_digest: applied.hook_plan_digest.clone(),
            source_thread_id: applied.source_thread_id.clone(),
            source_turn_id: Some(DriverTurnId::new("source-turn-1").unwrap()),
            source_item_id: Some(DriverItemId::new("item-1").unwrap()),
            definition_id: HookDefinitionId::new("hook-1").unwrap(),
            point: HookPoint::BeforeTool,
            payload: serde_json::json!({}),
            authorization_identity: None,
        }
    }

    #[tokio::test]
    async fn hook_registry_accepts_only_the_current_applied_coordinates() {
        let registry = CompiledAgentRunToolRegistry::default();
        let binding = |revision| CompiledAgentRunToolBinding {
            applied: fixture_applied("binding-hook-fence", revision),
            runtime_session_id: "presentation-hook-fence".into(),
            run_id: uuid::Uuid::nil(),
            agent_id: uuid::Uuid::nil(),
            frame_id: uuid::Uuid::nil(),
            hook_runtime: fixture_hook_runtime("presentation-hook-fence"),
            catalog: agentdash_agent_runtime::ToolCatalogRevision {
                revision: ToolSetRevision(revision),
                digest: format!("catalog-{revision}"),
                tools: Vec::new(),
                mcp_servers: Vec::new(),
            },
            tools: BTreeMap::new(),
            terminal_hook_effect_binding: None,
        };
        let first = binding(1);
        let old_request = fixture_hook_request(&first.applied);
        registry.put(first).await.unwrap();
        assert!(registry.get_current_hook(&old_request).await.is_some());

        let current = binding(2);
        let current_request = fixture_hook_request(&current.applied);
        registry.put(current).await.unwrap();
        assert!(registry.get_current_hook(&old_request).await.is_none());
        assert!(registry.get_current_hook(&current_request).await.is_some());

        let mut stale_generation = current_request.clone();
        stale_generation.generation = RuntimeDriverGeneration(0);
        assert!(registry.get_current_hook(&stale_generation).await.is_none());
        let mut wrong_thread = current_request.clone();
        wrong_thread.thread_id = RuntimeThreadId::new("wrong-thread").unwrap();
        assert!(registry.get_current_hook(&wrong_thread).await.is_none());
        let mut wrong_source_thread = current_request.clone();
        wrong_source_thread.source_thread_id = DriverThreadId::new("wrong-source-thread").unwrap();
        assert!(
            registry
                .get_current_hook(&wrong_source_thread)
                .await
                .is_none()
        );
        let mut wrong_plan = current_request;
        wrong_plan.hook_plan_digest = HookPlanDigest::new("wrong-hook-digest").unwrap();
        assert!(registry.get_current_hook(&wrong_plan).await.is_none());
    }

    #[test]
    fn hook_invocation_rejects_wrong_turn_and_item_correlations() {
        let applied = fixture_applied("binding-hook-coordinates", 1);
        let request = fixture_hook_request(&applied);
        assert!(hook_invocation_coordinates_are_current(
            &request,
            request.turn_id.as_ref(),
        ));

        assert!(!hook_invocation_coordinates_are_current(
            &request,
            Some(&RuntimeTurnId::new("different-runtime-turn").unwrap()),
        ));
        let mut wrong_source_item = request.clone();
        wrong_source_item.source_item_id = Some(DriverItemId::new("different-item").unwrap());
        assert!(!hook_invocation_coordinates_are_current(
            &wrong_source_item,
            wrong_source_item.turn_id.as_ref(),
        ));
        let mut missing_source_turn = request;
        missing_source_turn.source_turn_id = None;
        assert!(!hook_invocation_coordinates_are_current(
            &missing_source_turn,
            missing_source_turn.turn_id.as_ref(),
        ));
    }

    struct MissingProjectorTool;
    #[async_trait]
    impl agentdash_agent::AgentTool for MissingProjectorTool {
        fn name(&self) -> &str {
            "missing_projector"
        }
        fn description(&self) -> &str {
            "fixture"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        async fn execute(
            &self,
            _: &str,
            _: serde_json::Value,
            _: tokio_util::sync::CancellationToken,
            _: Option<agentdash_agent::ToolUpdateCallback>,
        ) -> Result<agentdash_agent::AgentToolResult, agentdash_agent::AgentToolError> {
            unreachable!("admission fails before execution")
        }
    }

    struct MissingFixtureTool;
    #[async_trait]
    impl agentdash_agent::AgentTool for MissingFixtureTool {
        fn name(&self) -> &str {
            "missing_fixture"
        }
        fn description(&self) -> &str {
            "fixture"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        fn protocol_projector(&self) -> Option<agentdash_agent::ToolProtocolProjector> {
            Some(agentdash_agent::ToolProtocolProjector::Dynamic { namespace: None })
        }
        async fn execute(
            &self,
            _: &str,
            _: serde_json::Value,
            _: tokio_util::sync::CancellationToken,
            _: Option<agentdash_agent::ToolUpdateCallback>,
        ) -> Result<agentdash_agent::AgentToolResult, agentdash_agent::AgentToolError> {
            unreachable!("admission fails before execution")
        }
    }

    #[test]
    fn business_surface_rejects_tool_without_owner_projector() {
        let error = require_tool_protocol_projection(&MissingProjectorTool, "missing_projector")
            .expect_err("missing projector must fail admission");
        assert!(
            error
                .to_string()
                .contains("no owner-declared protocol projector")
        );
    }

    #[test]
    fn business_surface_rejects_tool_without_main_parity_fixture() {
        let error = require_tool_protocol_projection(&MissingFixtureTool, "missing_fixture")
            .expect_err("missing fixture must fail admission");
        assert!(
            error
                .to_string()
                .contains("no owner-declared main parity fixture")
        );
    }

    #[tokio::test]
    async fn production_shell_owner_survives_registry_and_terminal_projection() {
        let tool: DynAgentTool = Arc::new(agentdash_application_vfs::tools::ShellExecTool::new(
            Arc::new(agentdash_application_vfs::VfsService::new(Arc::new(
                agentdash_application_vfs::MountProviderRegistryBuilder::new().build(),
            ))),
            agentdash_application_vfs::tools::SharedRuntimeVfs::new(agentdash_spi::Vfs {
                mounts: Vec::new(),
                default_mount_id: None,
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            }),
        ));
        let projection = require_tool_protocol_projection(tool.as_ref(), tool.name()).unwrap();
        let contribution = agentdash_agent_runtime::ToolContribution {
            meta: agentdash_agent_runtime::ContributionMeta {
                key: "tool:shell-exec".into(),
                source: agentdash_agent_runtime::SurfaceSourceRef {
                    layer: "platform".into(),
                    key: "vfs".into(),
                },
                priority: 1,
                requirement: agentdash_agent_runtime::ContributionRequirement::Required,
            },
            runtime_name: tool.name().into(),
            description: tool.description().into(),
            parameters_schema: tool.parameters_schema(),
            capability_key: "shell".into(),
            tool_path: "vfs::shell_exec".into(),
            allowed_channels: BTreeSet::from([ToolChannel::DirectCallback]),
            configuration_boundary: ConfigurationBoundary::Binding,
            protocol_projection: projection.projection,
            presentation_emitter:
                agentdash_agent_runtime_contract::ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: projection.fixture_id,
        };
        let binding_id = RuntimeBindingId::new("binding-shell-owner").unwrap();
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        registry
            .put(CompiledAgentRunToolBinding {
                applied: fixture_applied("binding-shell-owner", 1),
                runtime_session_id: "session-shell-owner".into(),
                run_id: uuid::Uuid::new_v4(),
                agent_id: uuid::Uuid::new_v4(),
                frame_id: uuid::Uuid::new_v4(),
                hook_runtime: fixture_hook_runtime("session-shell-owner"),
                catalog: agentdash_agent_runtime::ToolCatalogRevision {
                    revision: ToolSetRevision(1),
                    digest: "shell-owner".into(),
                    tools: vec![contribution.clone()],
                    mcp_servers: Vec::new(),
                },
                tools: BTreeMap::from([(tool.name().to_string(), tool)]),
                terminal_hook_effect_binding: None,
            })
            .await
            .unwrap();
        let (updates, _updates_rx) = tokio::sync::mpsc::unbounded_channel();
        let arguments = serde_json::json!({"command":"pwd"});
        let result = RegistryToolExecutor { registry }
            .execute(agentdash_agent_runtime::ToolExecutionRequest {
                idempotency_key: RuntimeItemId::new("shell-owner-item").unwrap(),
                invocation: agentdash_agent_runtime::ToolBrokerInvocation {
                    coordinates: agentdash_agent_runtime::ToolCallCoordinates {
                        thread_id: RuntimeThreadId::new("shell-owner-thread").unwrap(),
                        turn_id: RuntimeTurnId::new("shell-owner-turn").unwrap(),
                        item_id: RuntimeItemId::new("shell-owner-item").unwrap(),
                        binding_id,
                        binding_generation: RuntimeDriverGeneration(1),
                        tool_set_revision: ToolSetRevision(1),
                    },
                    tool_name: "shell_exec".into(),
                    arguments: arguments.clone(),
                    timeout_ms: 1_000,
                },
                credentials: agentdash_agent_runtime::CredentialMaterial::new(BTreeMap::new()),
                cancellation: tokio_util::sync::CancellationToken::new(),
                updates,
            })
            .await
            .unwrap();
        assert_eq!(result.output["cwd"], "platform://");
        assert_eq!(result.output["exit_code"], 0);
        let terminal = serde_json::to_value(
            contribution
                .project_completed("shell-owner-item", arguments, &result.output, false)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(terminal["cwd"], "platform://");
        assert_eq!(terminal["exitCode"], 0);
        assert!(terminal["aggregatedOutput"].as_str().is_some());
    }

    #[tokio::test]
    async fn production_apply_patch_owner_preserves_changes_on_registry_failure() {
        let tool: DynAgentTool = Arc::new(agentdash_application_vfs::tools::FsApplyPatchTool::new(
            Arc::new(agentdash_application_vfs::VfsService::new(Arc::new(
                agentdash_application_vfs::MountProviderRegistryBuilder::new().build(),
            ))),
            agentdash_application_vfs::tools::SharedRuntimeVfs::new(agentdash_spi::Vfs {
                mounts: Vec::new(),
                default_mount_id: None,
                source_project_id: None,
                source_story_id: None,
                links: Vec::new(),
            }),
            None,
            None,
        ));
        let projection = require_tool_protocol_projection(tool.as_ref(), tool.name()).unwrap();
        let contribution = agentdash_agent_runtime::ToolContribution {
            meta: agentdash_agent_runtime::ContributionMeta {
                key: "tool:apply-patch".into(),
                source: agentdash_agent_runtime::SurfaceSourceRef {
                    layer: "platform".into(),
                    key: "vfs".into(),
                },
                priority: 1,
                requirement: agentdash_agent_runtime::ContributionRequirement::Required,
            },
            runtime_name: tool.name().into(),
            description: tool.description().into(),
            parameters_schema: tool.parameters_schema(),
            capability_key: "fs.write".into(),
            tool_path: "vfs::apply_patch".into(),
            allowed_channels: BTreeSet::from([ToolChannel::DirectCallback]),
            configuration_boundary: ConfigurationBoundary::Binding,
            protocol_projection: projection.projection,
            presentation_emitter:
                agentdash_agent_runtime_contract::ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: projection.fixture_id,
        };
        let binding_id = RuntimeBindingId::new("binding-patch-owner").unwrap();
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        registry
            .put(CompiledAgentRunToolBinding {
                applied: fixture_applied("binding-patch-owner", 1),
                runtime_session_id: "session-patch-owner".into(),
                run_id: uuid::Uuid::new_v4(),
                agent_id: uuid::Uuid::new_v4(),
                frame_id: uuid::Uuid::new_v4(),
                hook_runtime: fixture_hook_runtime("session-patch-owner"),
                catalog: agentdash_agent_runtime::ToolCatalogRevision {
                    revision: ToolSetRevision(1),
                    digest: "patch-owner".into(),
                    tools: vec![contribution.clone()],
                    mcp_servers: Vec::new(),
                },
                tools: BTreeMap::from([(tool.name().to_string(), tool)]),
                terminal_hook_effect_binding: None,
            })
            .await
            .unwrap();
        let patch = "*** Begin Patch\n*** Add File: main://src/new.rs\n+new\n*** Update File: main://src/lib.rs\n*** Move to: main://src/moved.rs\n@@\n-old\n+new\n*** Delete File: main://src/old.rs\n*** End Patch";
        let arguments = serde_json::json!({"patch":patch});
        let (updates, _updates_rx) = tokio::sync::mpsc::unbounded_channel();
        let execution = RegistryToolExecutor { registry }
            .execute(agentdash_agent_runtime::ToolExecutionRequest {
                idempotency_key: RuntimeItemId::new("patch-owner-item").unwrap(),
                invocation: agentdash_agent_runtime::ToolBrokerInvocation {
                    coordinates: agentdash_agent_runtime::ToolCallCoordinates {
                        thread_id: RuntimeThreadId::new("patch-owner-thread").unwrap(),
                        turn_id: RuntimeTurnId::new("patch-owner-turn").unwrap(),
                        item_id: RuntimeItemId::new("patch-owner-item").unwrap(),
                        binding_id,
                        binding_generation: RuntimeDriverGeneration(1),
                        tool_set_revision: ToolSetRevision(1),
                    },
                    tool_name: "fs_apply_patch".into(),
                    arguments: arguments.clone(),
                    timeout_ms: 1_000,
                },
                credentials: agentdash_agent_runtime::CredentialMaterial::new(BTreeMap::new()),
                cancellation: tokio_util::sync::CancellationToken::new(),
                updates,
            })
            .await;
        assert!(
            execution.is_err(),
            "missing production mount must fail through Registry"
        );
        let started = serde_json::to_value(
            contribution
                .project_started("patch-owner-item", arguments.clone())
                .unwrap(),
        )
        .unwrap();
        let failed = serde_json::to_value(
            contribution
                .project_completed(
                    "patch-owner-item",
                    arguments,
                    &serde_json::json!({"message":"mount unavailable"}),
                    true,
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(started["changes"][0]["path"], "main://src/new.rs");
        assert_eq!(started["changes"][1]["path"], "main://src/lib.rs");
        assert_eq!(
            started["changes"][1]["kind"]["move_path"],
            "main://src/moved.rs"
        );
        assert_eq!(started["changes"].as_array().unwrap().len(), 3);
        assert_eq!(failed["changes"].as_array().unwrap().len(), 3);
        for change in failed["changes"].as_array().unwrap() {
            let diff = change["diff"].as_str().unwrap();
            let path = change["path"].as_str().unwrap();
            for other in [
                "main://src/new.rs",
                "main://src/lib.rs",
                "main://src/old.rs",
            ] {
                if other != path {
                    assert!(!diff.contains(other));
                }
            }
        }
        assert_eq!(failed["status"], "failed");
    }

    fn capability_state_with_platform_tools() -> CapabilityState {
        let mut state =
            CapabilityState::from_clusters([ToolCluster::Read, ToolCluster::WorkspaceModule]);
        state
            .tool
            .capabilities
            .insert(ToolCapability::new(CAP_FILE_READ));
        state
            .tool
            .capabilities
            .insert(ToolCapability::new(CAP_WORKSPACE_MODULE));
        state
    }

    #[test]
    fn canonical_descriptor_resolves_mounts_list_without_sparse_policy_entry() {
        let state = capability_state_with_platform_tools();

        assert_eq!(
            capability_for_tool(&state, "mounts_list").expect("canonical capability"),
            CAP_FILE_READ
        );
    }

    #[test]
    fn canonical_descriptor_respects_sparse_tool_policy_exclusion() {
        let mut state = capability_state_with_platform_tools();
        let mut filter = ToolCapabilityFilter::default();
        filter.exclude.insert("mounts_list".to_string());
        state
            .tool
            .tool_policy
            .insert(CAP_FILE_READ.to_string(), filter);

        let error = capability_for_tool(&state, "mounts_list").expect_err("excluded tool");
        assert!(error.to_string().contains("is not enabled"));
    }

    #[test]
    fn unknown_tool_does_not_inherit_the_only_enabled_capability() {
        let mut state = CapabilityState::from_clusters([ToolCluster::Read]);
        state
            .tool
            .capabilities
            .insert(ToolCapability::new(CAP_FILE_READ));

        let error = capability_for_tool(&state, "unknown_runtime_tool")
            .expect_err("unknown tool must remain unowned");
        assert!(error.to_string().contains("no unambiguous"));
    }

    #[test]
    fn native_offer_profile_satisfies_supervised_frame_hook_requirement() {
        let profile = agentdash_integration_native_agent::native_runtime_profile();
        let requirement = HookRequirement {
            point: HookPoint::BeforeTool,
            actions: BTreeSet::from([HookAction::RequestApproval]),
            minimum_strength: SemanticStrength::ExactSynchronous,
            failure_policy: HookFailurePolicy::FailClosed,
            required: true,
        };
        assert!(
            profile.hooks.satisfies(&requirement),
            "native profile must satisfy {requirement:?}"
        );
    }

    #[test]
    fn initial_driver_context_does_not_project_frame_summary_as_user_input() {
        assert!(initial_driver_context_blocks().is_empty());
    }

    #[test]
    fn tool_broker_hook_remains_in_runtime_plan_but_not_driver_admission() {
        let requirement = AgentFrameHookRequirement {
            definition_id: HookDefinitionId::new("workflow.supervised_tool_gate").unwrap(),
            requirement: HookRequirement {
                point: HookPoint::BeforeTool,
                actions: BTreeSet::from([HookAction::RequestApproval]),
                minimum_strength: SemanticStrength::ExactSynchronous,
                failure_policy: HookFailurePolicy::FailClosed,
                required: true,
            },
            site: HookExecutionSite::ToolBroker,
        };
        let plan = AgentFrameHookPlan::compile(HookPlanRevision(1), vec![requirement]).unwrap();
        let (runtime, driver, boundary) = materialize_hook_plan(&plan);

        assert_eq!(runtime.digest, plan.digest);
        assert_eq!(runtime.entries.len(), 1);
        assert_eq!(runtime.entries[0].site, HookExecutionSite::ToolBroker);
        assert!(driver.is_empty());
        assert_eq!(boundary, ConfigurationBoundary::StaticService);
    }
}
