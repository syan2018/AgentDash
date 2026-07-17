use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock, Weak},
};

use agentdash_agent_runtime_contract::*;
use agentdash_application_agentrun::agent_run::{
    AgentBusinessSurfaceSource, AgentFrameHookRuntime, AgentFrameSurfaceExt,
    AllowAllAgentRunPermissionFacade,
};
use agentdash_application_ports::agent_frame_hook_plan::AgentFrameHookPlan;
use agentdash_application_ports::agent_run_permission::{
    AgentRunPermissionDecision, AgentRunPermissionFacade, AgentRunPermissionRequest,
};
use agentdash_application_ports::agent_run_surface::{
    AgentRunAdmissionRequest, AgentRunEffectiveCapabilityPort, AgentRunRuntimeSurface,
};
use agentdash_application_ports::runtime_surface_adoption::{
    AgentFrameRuntimeTarget, RuntimeSurfaceAdoptionError, RuntimeSurfaceAdoptionPort,
};
use agentdash_infrastructure::persistence::postgres::PostgresToolBrokerRepository;
use agentdash_integration_api::*;
use agentdash_spi::{
    DynAgentTool, HookRuntimeEvaluationQuery, HookRuntimeRefreshQuery, HookTrigger,
    RuntimeAdapterProvenance, SharedHookRuntime, build_hook_trace_envelope,
    hook_trace_entry_storage_disposition,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};

use super::agent_runtime::{
    AgentRunPlatformToolBrokerResolver, AgentRunRuntimeSurfaceSourceError,
    AppliedNativeAgentRunSurface, NativeAgentRunSurfaceCompiler, NativeAgentRunSurfacePlan,
    NativeAgentRunSurfacePublication, NativeAgentRunSurfacePublicationReservation,
};

#[async_trait]
pub trait AgentRunToolInvocationFactory: Send + Sync {
    async fn build_tools(
        &self,
        surface: &AgentRunRuntimeSurface,
        executor: &agentdash_spi::AgentConfig,
        coordinates: &agentdash_agent_runtime::ToolCallCoordinates,
        hook_runtime: SharedHookRuntime,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Result<Vec<DynAgentTool>, String>;
}

#[async_trait]
impl AgentRunToolInvocationFactory for AgentBusinessSurfaceSource {
    async fn build_tools(
        &self,
        surface: &AgentRunRuntimeSurface,
        executor: &agentdash_spi::AgentConfig,
        coordinates: &agentdash_agent_runtime::ToolCallCoordinates,
        hook_runtime: SharedHookRuntime,
        identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Result<Vec<DynAgentTool>, String> {
        self.build_tools_for_invocation(surface, executor, coordinates, hook_runtime, identity)
            .await
    }
}

#[derive(Clone)]
pub struct CompiledAgentRunToolBinding {
    pub applied: AppliedNativeAgentRunSurface,
    pub runtime_session_id: String,
    pub run_id: uuid::Uuid,
    pub agent_id: uuid::Uuid,
    pub frame_id: uuid::Uuid,
    pub hook_runtime: SharedHookRuntime,
    pub catalog: agentdash_agent_runtime::ToolCatalogRevision,
    pub tool_factory: Arc<dyn AgentRunToolInvocationFactory>,
    pub surface: AgentRunRuntimeSurface,
    pub executor: agentdash_spi::AgentConfig,
    pub tool_names: BTreeSet<String>,
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
    pub(crate) tool_factory: Arc<dyn AgentRunToolInvocationFactory>,
    pub(crate) surface: AgentRunRuntimeSurface,
    pub(crate) executor: agentdash_spi::AgentConfig,
    pub(crate) tool_names: BTreeSet<String>,
}

impl PendingCompiledAgentRunToolBinding {
    fn applied_binding(
        &self,
        applied: AppliedNativeAgentRunSurface,
    ) -> Result<CompiledAgentRunToolBinding, AgentRunRuntimeSurfaceSourceError> {
        let surface_revision = u64::try_from(self.surface.surface_revision).map_err(|_| {
            AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "compiled invocation surface revision is invalid".to_string(),
            }
        })?;
        let hook_target = self.hook_runtime.control_target();
        if self.catalog.revision != applied.tool_set_revision
            || SurfaceRevision(surface_revision) != applied.surface_revision
            || self.runtime_session_id != applied.runtime_thread_id.to_string()
            || self.surface.runtime_session_id != self.runtime_session_id
            || self.surface.current_surface_frame_id != self.frame_id
            || self.surface.run_id != self.run_id
            || self.surface.agent_id != self.agent_id
            || self.hook_runtime.session_id() != self.runtime_session_id
            || hook_target.run_id != self.run_id
            || hook_target.agent_id != self.agent_id
            || hook_target.frame_id != self.frame_id
        {
            return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason:
                    "compiled invocation context does not match the adopted Runtime coordinates"
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
            tool_factory: self.tool_factory.clone(),
            surface: self.surface.clone(),
            executor: self.executor.clone(),
            tool_names: self.tool_names.clone(),
            terminal_hook_effect_binding,
        })
    }
}

#[cfg(test)]
impl CompiledAgentRunToolBinding {
    pub(crate) fn from_test_tools(
        applied: AppliedNativeAgentRunSurface,
        run_id: uuid::Uuid,
        agent_id: uuid::Uuid,
        frame_id: uuid::Uuid,
        hook_runtime: SharedHookRuntime,
        catalog: agentdash_agent_runtime::ToolCatalogRevision,
        tools: Vec<DynAgentTool>,
    ) -> Self {
        PendingCompiledAgentRunToolBinding::from_test_tools(
            Arc::new(CompiledAgentRunToolRegistry::default()),
            &applied,
            run_id,
            agent_id,
            frame_id,
            hook_runtime,
            catalog,
            tools,
        )
        .applied_binding(applied)
        .expect("test compiled invocation binding")
    }
}

#[cfg(test)]
#[derive(Clone)]
struct StaticTestToolInvocationFactory {
    tools: Vec<DynAgentTool>,
}

#[cfg(test)]
#[async_trait]
impl AgentRunToolInvocationFactory for StaticTestToolInvocationFactory {
    async fn build_tools(
        &self,
        _surface: &AgentRunRuntimeSurface,
        _executor: &agentdash_spi::AgentConfig,
        _coordinates: &agentdash_agent_runtime::ToolCallCoordinates,
        _hook_runtime: SharedHookRuntime,
        _identity: Option<agentdash_spi::AuthIdentity>,
    ) -> Result<Vec<DynAgentTool>, String> {
        Ok(self.tools.clone())
    }
}

#[cfg(test)]
fn test_invocation_surface(
    runtime_session_id: String,
    run_id: uuid::Uuid,
    agent_id: uuid::Uuid,
    frame_id: uuid::Uuid,
    revision: SurfaceRevision,
) -> AgentRunRuntimeSurface {
    AgentRunRuntimeSurface {
        presentation_thread_id: format!("presentation-{runtime_session_id}")
            .parse()
            .expect("test presentation thread"),
        runtime_session_id,
        run_id,
        project_id: uuid::Uuid::nil(),
        agent_id,
        runtime_address: agentdash_application_ports::agent_run_surface::AgentRunRuntimeAddress {
            run_id,
            agent_id,
            frame_id,
        },
        launch_evidence_frame_id: frame_id,
        current_surface_frame_id: frame_id,
        surface_revision: i32::try_from(revision.0).expect("test surface revision"),
        capability_state: agentdash_spi::CapabilityState::default(),
        vfs: agentdash_spi::Vfs::default(),
        vfs_access_policy: agentdash_spi::RuntimeVfsAccessPolicy::default(),
        mcp_servers: Vec::new(),
        runtime_backend_anchor: None,
        active_turn_id: None,
        identity: None,
        provenance:
            agentdash_application_ports::agent_run_surface::AgentRunRuntimeSurfaceProvenance {
                launch_evidence_frame_id: frame_id,
                launch_created_by_kind: "test".to_string(),
                current_surface_frame_id: frame_id,
                surface_revision: i32::try_from(revision.0).expect("test surface revision"),
                surface_created_by_kind: "test".to_string(),
                anchor_updated_at: chrono::Utc::now(),
                orchestration_id: None,
                node_path: None,
                node_attempt: None,
            },
        closure: agentdash_application_ports::agent_run_surface::AgentRunRuntimeSurfaceClosure {
            capability_field_present: true,
            vfs_field_present: true,
            mcp_field_present: true,
        },
    }
}

#[cfg(test)]
impl PendingCompiledAgentRunToolBinding {
    pub(crate) fn from_test_tools(
        registry: Arc<CompiledAgentRunToolRegistry>,
        applied: &AppliedNativeAgentRunSurface,
        run_id: uuid::Uuid,
        agent_id: uuid::Uuid,
        frame_id: uuid::Uuid,
        hook_runtime: SharedHookRuntime,
        catalog: agentdash_agent_runtime::ToolCatalogRevision,
        tools: Vec<DynAgentTool>,
    ) -> Self {
        let runtime_session_id = applied.runtime_thread_id.to_string();
        let tool_names = tools.iter().map(|tool| tool.name().to_string()).collect();
        Self {
            registry,
            runtime_session_id: runtime_session_id.clone(),
            run_id,
            agent_id,
            frame_id,
            hook_runtime,
            catalog,
            tool_factory: Arc::new(StaticTestToolInvocationFactory { tools }),
            surface: test_invocation_surface(
                runtime_session_id,
                run_id,
                agent_id,
                frame_id,
                applied.surface_revision,
            ),
            executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
            tool_names,
        }
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
                presentation_turn_id: snapshot.active_presentation_turn_id.clone(),
                runtime_item_id: request.item_id.clone(),
                interaction_id: None,
                source_thread_id: Some(binding.runtime_session_id.clone()),
                source_turn_id: snapshot
                    .active_presentation_turn_id
                    .clone()
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

fn hook_context_presentation_facts(
    facts: agentdash_spi::hooks::HookContextPresentationFacts,
) -> Result<agentdash_agent_runtime::ContextFrameFacts, DriverHookCallbackError> {
    let facts = match facts {
        agentdash_spi::hooks::HookContextPresentationFacts::SystemNotice {
            title,
            summary,
            body,
        } => agentdash_agent_runtime::HookSemanticPresentationFacts::SystemNotice {
            title,
            summary,
            body,
        },
        agentdash_spi::hooks::HookContextPresentationFacts::AssignmentInjection {
            title,
            summary,
            injections,
        } => agentdash_agent_runtime::HookSemanticPresentationFacts::AssignmentInjection {
            title,
            summary,
            injections: injections
                .into_iter()
                .map(
                    |injection| agentdash_agent_protocol::RuntimeHookInjectionEntry {
                        slot: injection.slot,
                        content: injection.content,
                        source: injection.source,
                        context_usage_kind: None,
                    },
                )
                .collect(),
        },
    };
    agentdash_agent_runtime::compile_hook_presentation_facts(
        agentdash_agent_protocol::ContextFrameSource::RuntimeContextUpdate,
        facts,
    )
    .map_err(|reason| DriverHookCallbackError::ProtocolViolation { reason })
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
                let presentation = effect
                    .presentation
                    .map(hook_context_presentation_facts)
                    .transpose()?;
                let effect_type = if presentation.is_some() {
                    "runtime_context_presentation".to_string()
                } else {
                    effect.kind
                };
                let payload_digest = presentation.as_ref().map_or_else(
                    || agentdash_agent_runtime::hook_effect_payload_digest(&effect.payload),
                    |facts| {
                        agentdash_agent_runtime::hook_effect_payload_digest(
                            &serde_json::to_value(facts)
                                .expect("typed Hook presentation facts serialize"),
                        )
                    },
                );
                Ok(agentdash_agent_runtime::HookEffect {
                    effect_id,
                    hook_run_id: hook_run_id.clone(),
                    thread_id: request.thread_id.clone(),
                    idempotency_key: format!("{hook_run_id}:{index}"),
                    descriptor: agentdash_agent_runtime::HookEffectDescriptor {
                        effect_type,
                        schema_version: 1,
                        target_authority: "agentdash_hook_effect_dispatcher".to_string(),
                        retry_limit: 3,
                        payload_digest,
                    },
                    payload: effect.payload,
                    presentation,
                })
            })
            .collect::<Result<Vec<_>, DriverHookCallbackError>>()?;
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
    permissions: Arc<dyn AgentRunPermissionFacade>,
}

impl RegistryToolBrokerPolicy {
    pub fn new(
        registry: Arc<CompiledAgentRunToolRegistry>,
        capabilities: Arc<dyn AgentRunEffectiveCapabilityPort>,
        permissions: Arc<dyn AgentRunPermissionFacade>,
    ) -> Self {
        Self {
            registry,
            capabilities,
            permissions,
        }
    }

    async fn binding(
        &self,
        invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
    ) -> Result<CompiledAgentRunToolBinding, agentdash_agent_runtime::ToolBrokerError> {
        binding_for_invocation(self.registry.as_ref(), invocation).await
    }
}

async fn binding_for_invocation(
    registry: &CompiledAgentRunToolRegistry,
    invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
) -> Result<CompiledAgentRunToolBinding, agentdash_agent_runtime::ToolBrokerError> {
    let coordinates = &invocation.coordinates;
    let binding = registry
        .get_revision(&coordinates.binding_id, coordinates.tool_set_revision)
        .await
        .ok_or(agentdash_agent_runtime::ToolBrokerError::StaleCoordinates)?;
    let applied = &binding.applied;
    if applied.binding_id != coordinates.binding_id
        || applied.runtime_thread_id != coordinates.thread_id
        || applied.generation != coordinates.binding_generation
        || applied.tool_set_revision != coordinates.tool_set_revision
        || binding.catalog.revision != coordinates.tool_set_revision
        || binding.runtime_session_id != coordinates.thread_id.to_string()
        || binding.surface.current_surface_frame_id != binding.frame_id
        || !binding.tool_names.contains(&invocation.tool_name)
    {
        return Err(agentdash_agent_runtime::ToolBrokerError::StaleCoordinates);
    }
    Ok(binding)
}

fn surface_policy_revision(binding: &CompiledAgentRunToolBinding) -> u64 {
    u64::try_from(binding.surface.surface_revision).unwrap_or_default()
}

#[async_trait]
impl agentdash_agent_runtime::ToolBrokerPolicyPort for RegistryToolBrokerPolicy {
    async fn validate_binding(
        &self,
        invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
    ) -> Result<agentdash_agent_runtime::ToolGuardDecision, agentdash_agent_runtime::ToolBrokerError>
    {
        let binding = self.binding(invocation).await?;
        Ok(agentdash_agent_runtime::ToolGuardDecision::Allowed(
            agentdash_agent_runtime::ToolPolicyCheck {
                revision: surface_policy_revision(&binding),
            },
        ))
    }

    async fn authorize_capability(
        &self,
        invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
        tool: &agentdash_agent_runtime::ToolContribution,
    ) -> Result<agentdash_agent_runtime::ToolGuardDecision, agentdash_agent_runtime::ToolBrokerError>
    {
        let binding = self.binding(invocation).await?;
        let revision = surface_policy_revision(&binding);
        let decision = self
            .capabilities
            .admit_tool(AgentRunAdmissionRequest::tool(
                binding.runtime_session_id.clone(),
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
                agentdash_agent_runtime::ToolPolicyCheck { revision },
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
        invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
        tool: &agentdash_agent_runtime::ToolContribution,
    ) -> Result<
        agentdash_agent_runtime::ToolPermissionDecision,
        agentdash_agent_runtime::ToolBrokerError,
    > {
        let binding = self.binding(invocation).await?;
        let revision = surface_policy_revision(&binding);
        let decision = self
            .permissions
            .authorize(AgentRunPermissionRequest {
                run_id: binding.run_id,
                agent_id: binding.agent_id,
                runtime_session_id: binding.runtime_session_id,
                turn_id: invocation.coordinates.turn_id.to_string(),
                item_id: invocation.coordinates.item_id.to_string(),
                capability_key: tool.capability_key.clone(),
                tool_name: tool.runtime_name.clone(),
            })
            .await
            .map_err(|error| {
                agentdash_agent_runtime::ToolBrokerError::Execution(error.to_string())
            })?;
        match decision {
            AgentRunPermissionDecision::Allowed => {
                Ok(agentdash_agent_runtime::ToolPermissionDecision::Allowed(
                    agentdash_agent_runtime::ToolPolicyCheck { revision },
                ))
            }
            AgentRunPermissionDecision::Denied { reason } => {
                Ok(agentdash_agent_runtime::ToolPermissionDecision::Denied { reason })
            }
            AgentRunPermissionDecision::PendingApproval {
                interaction_id,
                reason,
            } => Ok(
                agentdash_agent_runtime::ToolPermissionDecision::ApprovalRequired {
                    interaction_id: RuntimeInteractionId::new(interaction_id).map_err(|error| {
                        agentdash_agent_runtime::ToolBrokerError::Execution(error.to_string())
                    })?,
                    reason,
                },
            ),
        }
    }

    async fn authorize_vfs(
        &self,
        invocation: &agentdash_agent_runtime::ToolBrokerInvocation,
        tool: &agentdash_agent_runtime::ToolContribution,
    ) -> Result<agentdash_agent_runtime::ToolGuardDecision, agentdash_agent_runtime::ToolBrokerError>
    {
        let binding = self.binding(invocation).await?;
        let required_operation = match &tool.protocol_projection {
            ToolProtocolProjection::Command => Some(agentdash_spi::RuntimeVfsOperation::Exec),
            ToolProtocolProjection::FileChange => {
                Some(agentdash_spi::RuntimeVfsOperation::ApplyPatch)
            }
            ToolProtocolProjection::FsRead => Some(agentdash_spi::RuntimeVfsOperation::Read),
            ToolProtocolProjection::FsGrep | ToolProtocolProjection::FsGlob => {
                Some(agentdash_spi::RuntimeVfsOperation::Search)
            }
            ToolProtocolProjection::Mcp { .. } | ToolProtocolProjection::Dynamic { .. } => None,
        };
        if let Some(required_operation) = required_operation
            && !binding
                .surface
                .vfs_access_policy
                .rules
                .iter()
                .any(|rule| rule.operations.contains(&required_operation))
        {
            return Ok(agentdash_agent_runtime::ToolGuardDecision::Denied {
                reason: format!(
                    "AgentFrame VFS policy does not grant {required_operation:?} for tool `{}`",
                    tool.runtime_name
                ),
            });
        }
        Ok(agentdash_agent_runtime::ToolGuardDecision::Allowed(
            agentdash_agent_runtime::ToolPolicyCheck {
                revision: surface_policy_revision(&binding),
            },
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
    authorization_identity: Option<agentdash_spi::AuthIdentity>,
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
        let binding = binding_for_invocation(self.registry.as_ref(), &request.invocation).await?;
        let hook_target = binding.hook_runtime.control_target();
        if hook_target.run_id != binding.run_id
            || hook_target.agent_id != binding.agent_id
            || hook_target.frame_id != binding.frame_id
            || binding.hook_runtime.session_id() != binding.runtime_session_id
        {
            return Err(agentdash_agent_runtime::ToolBrokerError::StaleCoordinates);
        }
        let rebuilt_tools = binding
            .tool_factory
            .build_tools(
                &binding.surface,
                &binding.executor,
                &request.invocation.coordinates,
                binding.hook_runtime.clone(),
                self.authorization_identity.clone(),
            )
            .await
            .map_err(agentdash_agent_runtime::ToolBrokerError::Execution)?;
        let mut rebuilt_names = BTreeSet::new();
        let mut selected_tool = None;
        for tool in rebuilt_tools {
            let name = tool.name().trim().to_string();
            if name.is_empty() || !rebuilt_names.insert(name.clone()) {
                return Err(agentdash_agent_runtime::ToolBrokerError::Execution(
                    "invocation tool rebuild produced an empty or duplicate tool name".to_string(),
                ));
            }
            if name == request.invocation.tool_name {
                selected_tool = Some(tool);
            }
        }
        if rebuilt_names != binding.tool_names {
            return Err(agentdash_agent_runtime::ToolBrokerError::StaleCoordinates);
        }
        let tool = selected_tool.ok_or_else(|| {
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
                Arc::new(AllowAllAgentRunPermissionFacade),
            )),
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
        if binding.applied.runtime_thread_id != request.thread_id
            || binding.applied.generation != request.generation
            || binding.applied.source_thread_id != request.source_thread_id
        {
            return Err(DriverToolCallbackError::Stale);
        }
        Ok(agentdash_agent_runtime::PlatformToolBroker::new(
            binding.catalog,
            request.binding_id.clone(),
            request.generation,
            agentdash_agent_runtime::PlatformToolBrokerDeps {
                repository: self.repository.clone(),
                journal: self.journal.clone(),
                policy: self.policy.clone(),
                credentials: Arc::new(EmbeddedToolCredentialResolver),
                executor: Arc::new(RegistryToolExecutor {
                    registry: self.registry.clone(),
                    authorization_identity: request.authorization_identity.clone(),
                }),
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

pub struct CompiledAgentRunToolRegistry {
    state: RwLock<CompiledAgentRunToolRegistryState>,
    recovery: OnceLock<Arc<dyn CompiledAgentRunToolBindingRecovery>>,
    recovery_locks: Mutex<BTreeMap<RuntimeBindingId, Arc<Mutex<()>>>>,
}

impl Default for CompiledAgentRunToolRegistry {
    fn default() -> Self {
        Self {
            state: RwLock::new(CompiledAgentRunToolRegistryState::default()),
            recovery: OnceLock::new(),
            recovery_locks: Mutex::new(BTreeMap::new()),
        }
    }
}

#[async_trait]
pub(crate) trait CompiledAgentRunToolBindingRecovery: Send + Sync {
    async fn recover(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<(), AgentRunRuntimeSurfaceSourceError>;
}

pub(crate) struct CanonicalCompiledAgentRunToolBindingRecovery {
    compiler: Weak<AgentFrameSurfaceCompositionAdapter>,
    repository: Arc<
        agentdash_infrastructure::persistence::postgres::PostgresAgentRuntimeCompositionRepository,
    >,
}

impl CanonicalCompiledAgentRunToolBindingRecovery {
    pub(crate) fn new(
        compiler: Weak<AgentFrameSurfaceCompositionAdapter>,
        repository: Arc<
            agentdash_infrastructure::persistence::postgres::PostgresAgentRuntimeCompositionRepository,
        >,
    ) -> Self {
        Self {
            compiler,
            repository,
        }
    }
}

#[async_trait]
impl CompiledAgentRunToolBindingRecovery for CanonicalCompiledAgentRunToolBindingRecovery {
    async fn recover(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<(), AgentRunRuntimeSurfaceSourceError> {
        let compiler = self.compiler.upgrade().ok_or_else(|| {
            AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: "AgentRun business surface compiler is no longer available".to_string(),
                retryable: true,
            }
        })?;
        let binding = self
            .repository
            .load_by_runtime_binding(binding_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "canonical AgentRun Runtime binding does not exist".to_string(),
            })?;
        let persisted_surface = self
            .repository
            .load_bound_surface(binding_id)
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "canonical materialized Runtime surface does not exist".to_string(),
            })?;
        let persisted_business_surface = self
            .repository
            .load_business_surface(
                binding_id,
                persisted_surface.revision,
                &persisted_surface.digest,
            )
            .await
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?;
        let request =
            agentdash_application_ports::agent_run_runtime::AgentRunRuntimeProvisionRequest {
                target: binding.target.clone(),
                presentation_thread_id: binding.presentation_thread_id.clone(),
                identity: persisted_surface.authorization_identity.clone(),
                backend_selection: None,
                fork: None,
                terminal_hook_effect_binding: binding.surface.terminal_hook_effect_binding.clone(),
            };
        let plan = compiler
            .compile(
                &request,
                &binding.thread_id,
                binding_id,
                agentdash_infrastructure::agent_runtime_composition::NativeAgentRunSurfaceCompileTarget::ExactAgentFrame(
                    uuid::Uuid::parse_str(
                        &persisted_business_surface.presentation.source_frame_id,
                    )
                    .map_err(|_| AgentRunRuntimeSurfaceSourceError::Invalid {
                        reason: "persisted business surface source_frame_id is invalid".to_string(),
                    })?,
                ),
            )
            .await?;
        // Recovery republishes executable handles only. Context, workspace and presentation are
        // already canonical durable facts and can depend on relay-backed discovery that is not
        // available during process startup. The freshly built executable portion must still be
        // byte-for-byte equivalent to the persisted tool/Hook surface before it may be attached
        // to those durable coordinates.
        if plan.source_frame_id != persisted_business_surface.presentation.source_frame_id
            || plan.surface.runtime_thread_id != persisted_surface.runtime_thread_id
            || plan.surface.revision != persisted_surface.revision
            || plan.surface.tools != persisted_surface.tools
            || plan.surface.hooks != persisted_surface.hooks
            || plan.business_surface.snapshot.tools != persisted_business_surface.snapshot.tools
            || plan.business_surface.snapshot.hook_plan
                != persisted_business_surface.snapshot.hook_plan
            || plan.hook_plan.revision != persisted_surface.hooks.revision
            || plan.hook_plan.digest != persisted_surface.hooks.digest
        {
            return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "recompiled AgentRun executable tool/Hook surface does not match the canonical persisted artifact"
                    .to_string(),
            });
        }
        let applied = AppliedNativeAgentRunSurface {
            runtime_thread_id: binding.thread_id,
            binding_id: binding.binding_id,
            generation: binding.driver_generation,
            source_thread_id: binding.source_thread_id,
            surface_revision: persisted_surface.revision,
            surface_digest: persisted_surface.digest,
            tool_set_revision: persisted_surface.tools.revision,
            hook_plan_revision: plan.hook_plan.revision,
            hook_plan_digest: plan.hook_plan.digest,
            terminal_hook_effect_binding: plan.terminal_hook_effect_binding,
        };
        plan.publication.reserve(applied).await?.commit().await
    }
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
        && Arc::ptr_eq(&existing.tool_factory, &binding.tool_factory)
        && existing.surface.runtime_session_id == binding.surface.runtime_session_id
        && existing.surface.presentation_thread_id == binding.surface.presentation_thread_id
        && existing.surface.run_id == binding.surface.run_id
        && existing.surface.project_id == binding.surface.project_id
        && existing.surface.agent_id == binding.surface.agent_id
        && existing.surface.runtime_address == binding.surface.runtime_address
        && existing.surface.launch_evidence_frame_id == binding.surface.launch_evidence_frame_id
        && existing.surface.current_surface_frame_id == binding.surface.current_surface_frame_id
        && existing.surface.surface_revision == binding.surface.surface_revision
        && existing.surface.capability_state == binding.surface.capability_state
        && existing.surface.vfs == binding.surface.vfs
        && existing.surface.vfs_access_policy == binding.surface.vfs_access_policy
        && existing.surface.mcp_servers == binding.surface.mcp_servers
        && existing.surface.runtime_backend_anchor == binding.surface.runtime_backend_anchor
        && existing.surface.active_turn_id == binding.surface.active_turn_id
        && existing.surface.identity == binding.surface.identity
        && existing.surface.provenance == binding.surface.provenance
        && existing.surface.closure == binding.surface.closure
        && existing.executor.executor == binding.executor.executor
        && existing.executor.provider_id == binding.executor.provider_id
        && existing.executor.model_id == binding.executor.model_id
        && existing.executor.agent_id == binding.executor.agent_id
        && existing.executor.thinking_level == binding.executor.thinking_level
        && existing.executor.system_prompt == binding.executor.system_prompt
        && existing.tool_names == binding.tool_names
        && existing.hook_runtime.session_id() == binding.hook_runtime.session_id()
        && existing.hook_runtime.control_target() == binding.hook_runtime.control_target()
        && existing.hook_runtime.snapshot() == binding.hook_runtime.snapshot()
        && existing.terminal_hook_effect_binding == binding.terminal_hook_effect_binding
}

impl CompiledAgentRunToolRegistry {
    pub(crate) fn bind_recovery(
        &self,
        recovery: Arc<dyn CompiledAgentRunToolBindingRecovery>,
    ) -> Result<(), &'static str> {
        self.recovery
            .set(recovery)
            .map_err(|_| "compiled AgentRun tool binding recovery is already configured")
    }

    async fn recover_if_missing(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<CompiledAgentRunToolBinding, AgentRunRuntimeSurfaceSourceError> {
        if let Some(binding) = self.get(binding_id).await {
            return Ok(binding);
        }
        let recovery = self.recovery.get().cloned().ok_or_else(|| {
            AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason: "compiled AgentRun binding recovery is not configured".to_string(),
                retryable: true,
            }
        })?;
        let lock = {
            let mut locks = self.recovery_locks.lock().await;
            locks
                .entry(binding_id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;
        if let Some(binding) = self.get(binding_id).await {
            return Ok(binding);
        }
        recovery.recover(binding_id).await?;
        self.get(binding_id)
            .await
            .ok_or_else(|| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "canonical recovery completed without publishing the compiled binding"
                    .to_string(),
            })
    }

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

#[async_trait]
impl agentdash_application_ports::agent_run_runtime::AgentRunTurnStartContextSource
    for CompiledAgentRunToolRegistry
{
    async fn take_turn_start_context(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<
        agentdash_application_ports::agent_run_runtime::AgentRunTurnStartContextFacts,
        agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingError,
    > {
        let binding = self.recover_if_missing(binding_id).await.map_err(|error| {
            agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingError::Unavailable {
                reason: error.to_string(),
                retryable: matches!(
                    error,
                    AgentRunRuntimeSurfaceSourceError::Unavailable {
                        retryable: true,
                        ..
                    }
                ),
            }
        })?;
        Ok(
            agentdash_application_ports::agent_run_runtime::AgentRunTurnStartContextFacts {
                runtime_snapshot: Some(binding.hook_runtime.runtime_snapshot()),
                pending_actions: binding.hook_runtime.collect_pending_actions_for_injection(),
                notices: binding.hook_runtime.peek_turn_start_notices(),
            },
        )
    }

    async fn acknowledge_turn_start_context(
        &self,
        binding_id: &RuntimeBindingId,
        notice_ids: &[String],
    ) -> Result<(), agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingError>
    {
        let binding = self.recover_if_missing(binding_id).await.map_err(|error| {
            agentdash_application_ports::agent_run_runtime::AgentRunRuntimeBindingError::Unavailable {
                reason: error.to_string(),
                retryable: matches!(
                    error,
                    AgentRunRuntimeSurfaceSourceError::Unavailable {
                        retryable: true,
                        ..
                    }
                ),
            }
        })?;
        binding
            .hook_runtime
            .acknowledge_turn_start_notices(notice_ids);
        Ok(())
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

pub struct ManagedRuntimeSurfaceHead {
    runtime: Arc<
        agentdash_agent_runtime::ManagedAgentRuntime<
            agentdash_infrastructure::PostgresRuntimeRepository,
        >,
    >,
}

impl ManagedRuntimeSurfaceHead {
    pub fn new(
        runtime: Arc<
            agentdash_agent_runtime::ManagedAgentRuntime<
                agentdash_infrastructure::PostgresRuntimeRepository,
            >,
        >,
    ) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl agentdash_application_agentrun::agent_run::AgentRunRuntimeSurfaceHead
    for ManagedRuntimeSurfaceHead
{
    async fn current_surface(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<RuntimeSurfaceDescriptor, String> {
        match self
            .runtime
            .snapshot(RuntimeSnapshotQuery::Thread {
                thread_id: thread_id.clone(),
                at_revision: None,
            })
            .await
            .map_err(|error| error.to_string())?
        {
            RuntimeSnapshotResult::Thread { snapshot } => Ok(snapshot.surface),
            _ => Err("Runtime surface head query returned a non-thread snapshot".to_string()),
        }
    }
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

async fn compile_exact_runtime_surface_candidate(
    compiler: &dyn NativeAgentRunSurfaceCompiler,
    request: &agentdash_application_ports::agent_run_runtime::AgentRunRuntimeProvisionRequest,
    thread_id: &RuntimeThreadId,
    binding_id: &RuntimeBindingId,
    frame_id: uuid::Uuid,
) -> Result<NativeAgentRunSurfacePlan, AgentRunRuntimeSurfaceSourceError> {
    compiler
        .compile(
            request,
            thread_id,
            binding_id,
            agentdash_infrastructure::agent_runtime_composition::NativeAgentRunSurfaceCompileTarget::ExactAgentFrame(
                frame_id,
            ),
        )
        .await
}

#[async_trait]
impl RuntimeSurfaceAdoptionPort for CanonicalRuntimeSurfaceAdopter {
    async fn adopt_runtime_surface(
        &self,
        target: AgentFrameRuntimeTarget,
    ) -> Result<(), RuntimeSurfaceAdoptionError> {
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
        let plan = compile_exact_runtime_surface_candidate(
            self.compiler.as_ref(),
            &request,
            &binding.thread_id,
            &binding.binding_id,
            target.frame_id,
        )
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
            .put_surface(&binding.binding_id, &plan.surface, &plan.business_surface)
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
        if snapshot.surface.surface_revision == descriptor.surface_revision
            && snapshot.surface.surface_digest == descriptor.surface_digest
        {
            reservation
                .commit()
                .await
                .map_err(|error| RuntimeSurfaceAdoptionError::Failed {
                    message: error.to_string(),
                })?;
            return self
                .tools
                .get_revision(&binding.binding_id, descriptor.tool_set_revision)
                .await
                .map(|_| ())
                .ok_or_else(|| RuntimeSurfaceAdoptionError::Failed {
                    message: "idempotent surface adoption has no compiled tool binding".to_string(),
                });
        }
        let previous_business_surface = match self
            .surfaces
            .load_business_surface(
                &binding.binding_id,
                snapshot.surface.surface_revision,
                &snapshot.surface.surface_digest,
            )
            .await
        {
            Ok(surface) => surface,
            Err(error) => {
                reservation.abort().await;
                return Err(RuntimeSurfaceAdoptionError::Failed {
                    message: error.to_string(),
                });
            }
        };
        let adoption_plan = agentdash_agent_runtime::RuntimeSurfacePresentationPlan::for_adoption(
            &previous_business_surface.snapshot,
            &plan.business_surface,
        );
        let presentation = adoption_plan.adoption_presentation(
            &binding.presentation_thread_id,
            snapshot.active_presentation_turn_id.as_ref(),
            &identity,
        );
        if let Err(error) = self
            .runtime
            .execute(RuntimeCommandEnvelope {
                presentation,
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
            .map(|_| ())
            .ok_or_else(|| RuntimeSurfaceAdoptionError::Failed {
                message: "canonical surface adoption did not publish its compiled tool binding"
                    .to_string(),
            })
    }
}

/// Composition adapter: binds Application source facts and callable publication handles to the
/// Runtime-owned `AgentSurfaceCompiler`. Business projection rules live in Runtime.
pub struct AgentFrameSurfaceCompositionAdapter {
    source: Arc<AgentBusinessSurfaceSource>,
    tool_registry: Arc<CompiledAgentRunToolRegistry>,
}

impl AgentFrameSurfaceCompositionAdapter {
    pub fn new(
        source: Arc<AgentBusinessSurfaceSource>,
        tool_registry: Arc<CompiledAgentRunToolRegistry>,
    ) -> Self {
        Self {
            source,
            tool_registry,
        }
    }
}

#[async_trait]
impl NativeAgentRunSurfaceCompiler for AgentFrameSurfaceCompositionAdapter {
    async fn compile(
        &self,
        request: &agentdash_application_ports::agent_run_runtime::AgentRunRuntimeProvisionRequest,
        thread_id: &RuntimeThreadId,
        binding_id: &RuntimeBindingId,
        target: agentdash_infrastructure::agent_runtime_composition::NativeAgentRunSurfaceCompileTarget,
    ) -> Result<NativeAgentRunSurfacePlan, AgentRunRuntimeSurfaceSourceError> {
        let loaded = self
            .source
            .load(
                request,
                thread_id,
                format!("surface-compile-{binding_id}"),
                match target {
                    agentdash_infrastructure::agent_runtime_composition::NativeAgentRunSurfaceCompileTarget::LatestPersistedAgentFrame => {
                        agentdash_application_agentrun::agent_run::AgentBusinessSurfaceFrameTarget::LatestPersisted
                    }
                    agentdash_infrastructure::agent_runtime_composition::NativeAgentRunSurfaceCompileTarget::ExactAgentFrame(frame_id) => {
                        agentdash_application_agentrun::agent_run::AgentBusinessSurfaceFrameTarget::Exact(frame_id)
                    }
                },
            )
            .await
            .map_err(|reason| AgentRunRuntimeSurfaceSourceError::Unavailable {
                reason,
                retryable: false,
            })?;
        let context_source = loaded.context_source;
        let surface = context_source.runtime.clone();
        let frame = loaded.frame;
        let executor = loaded.executor;
        let tools = loaded.tools;
        let hook_snapshot = loaded.hook_snapshot;
        let hook_provider = loaded.hook_provider;
        let business_facts = loaded.business_facts;
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
        let mut tool_names = BTreeSet::new();
        let mut driver_tools = Vec::new();
        let catalog_tools = business_facts
            .tools
            .iter()
            .map(|tool| (tool.runtime_name.clone(), tool))
            .collect::<BTreeMap<_, _>>();
        for tool in tools {
            let name = tool.name().trim().to_string();
            if name.is_empty() || !tool_names.insert(name.clone()) {
                return Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: format!("assembled runtime tool name is empty or duplicated: {name}"),
                });
            }
            let contribution = catalog_tools.get(&name).ok_or_else(|| {
                AgentRunRuntimeSurfaceSourceError::Invalid {
                    reason: format!("callable runtime tool `{name}` has no compiled business fact"),
                }
            })?;
            driver_tools.push(DriverToolDefinition {
                name: name.clone(),
                description: contribution.description.clone(),
                parameters_schema: contribution.parameters_schema.clone(),
                channels: vec![ToolChannel::DirectCallback],
                protocol_projection: contribution.protocol_projection.clone(),
                presentation_emitter: contribution.presentation_emitter,
                parity_fixture_id: contribution.parity_fixture_id.clone(),
            });
        }
        driver_tools.sort_by(|left, right| left.name.cmp(&right.name));
        let business_surface = agentdash_agent_runtime::AgentSurfaceCompiler
            .compile_business_facts(business_facts)
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: error.to_string(),
            })?;
        let catalog = business_surface.snapshot.tools.clone();
        let hook_runtime: SharedHookRuntime = Arc::new(AgentFrameHookRuntime::new(
            request.target.run_id,
            request.target.agent_id,
            frame.id,
            frame.revision,
            surface.runtime_session_id.clone(),
            hook_provider,
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
                catalog: catalog.clone(),
                tool_factory: self.source.clone(),
                surface: surface.clone(),
                executor: executor.clone(),
                tool_names,
            });
        let recipe = business_surface.snapshot.context.recipe.clone();
        let (instructions, blocks) = materialize_driver_context(&business_surface.snapshot.context);
        let context_digest = ContextDigest::new(business_surface.snapshot.context.digest.clone())
            .map_err(|error| AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: error.to_string(),
        })?;
        let hook_plan = frame
            .validated_hook_plan()
            .map_err(|reason| AgentRunRuntimeSurfaceSourceError::Invalid { reason })?;
        let (runtime_hook_plan, hook_bindings, hook_configuration_boundary) =
            materialize_hook_plan(&hook_plan);
        let hook_digest = business_surface.snapshot.hook_plan.digest.clone();
        let workspace_capabilities = business_surface
            .snapshot
            .workspace
            .capabilities
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let workspace_roots = surface
            .vfs
            .mounts
            .iter()
            .map(|mount| mount.root_ref.clone())
            .collect::<Vec<_>>();
        let surface_revision = SurfaceRevision(revision);
        let surface_digest = business_surface.snapshot.digest.clone();
        Ok(NativeAgentRunSurfacePlan {
            source_frame_id: frame.id.to_string(),
            executor: executor_id,
            provider,
            model,
            hook_plan: runtime_hook_plan,
            publication,
            terminal_hook_effect_binding: request.terminal_hook_effect_binding.clone(),
            business_surface,
            surface: MaterializedDriverSurface {
                runtime_thread_id: thread_id.clone(),
                revision: surface_revision,
                digest: surface_digest,
                authorization_identity: request.identity.clone().or(surface.identity),
                context: DriverContextSurface {
                    recipe,
                    instructions,
                    blocks,
                    digest: context_digest,
                    fidelity: ContextFidelity::PlatformExact,
                },
                tools: DriverToolSurface {
                    revision: catalog.revision,
                    digest: catalog.digest.clone(),
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
                    digest: digest_json(&(&workspace_capabilities, &workspace_roots))?,
                    capabilities: workspace_capabilities,
                    roots: workspace_roots,
                },
            },
        })
    }
}

fn materialize_driver_context(
    context: &agentdash_agent_runtime::ContextEnvelope,
) -> (Vec<DriverInstructionSet>, Vec<ContextBlock>) {
    let mut instructions_by_channel = BTreeMap::<InstructionChannel, Vec<String>>::new();
    for entry in &context.instructions.entries {
        instructions_by_channel
            .entry(entry.channel)
            .or_default()
            .push(entry.content.clone());
    }
    let instructions = instructions_by_channel
        .into_iter()
        .map(|(channel, entries)| DriverInstructionSet { channel, entries })
        .collect();
    let blocks = context
        .contributions
        .iter()
        .flat_map(|entry| entry.blocks.iter().cloned())
        .collect();
    (instructions, blocks)
}

#[cfg(test)]
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

fn digest_json(value: &impl serde::Serialize) -> Result<String, AgentRunRuntimeSurfaceSourceError> {
    let value = serde_json::to_value(value).map_err(|error| {
        AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: error.to_string(),
        }
    })?;
    let bytes = agentdash_agent_runtime_host::canonical_json(&value);
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct RecordingSurfaceCompiler {
        target: std::sync::Mutex<
            Option<
                agentdash_infrastructure::agent_runtime_composition::NativeAgentRunSurfaceCompileTarget,
            >,
        >,
    }

    #[async_trait]
    impl NativeAgentRunSurfaceCompiler for RecordingSurfaceCompiler {
        async fn compile(
            &self,
            _request: &agentdash_application_ports::agent_run_runtime::AgentRunRuntimeProvisionRequest,
            _thread_id: &RuntimeThreadId,
            _binding_id: &RuntimeBindingId,
            target: agentdash_infrastructure::agent_runtime_composition::NativeAgentRunSurfaceCompileTarget,
        ) -> Result<NativeAgentRunSurfacePlan, AgentRunRuntimeSurfaceSourceError> {
            *self.target.lock().expect("record compile target") = Some(target);
            Err(AgentRunRuntimeSurfaceSourceError::Invalid {
                reason: "stop after recording target".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn surface_adoption_compiles_the_exact_candidate_frame() {
        let compiler = RecordingSurfaceCompiler::default();
        let frame_id = uuid::Uuid::new_v4();
        let request =
            agentdash_application_ports::agent_run_runtime::AgentRunRuntimeProvisionRequest {
                target: agentdash_application_ports::agent_run_runtime::AgentRunRuntimeTarget {
                    run_id: uuid::Uuid::new_v4(),
                    agent_id: uuid::Uuid::new_v4(),
                },
                presentation_thread_id: PresentationThreadId::new("candidate-presentation")
                    .expect("presentation thread"),
                identity: None,
                backend_selection: None,
                fork: None,
                terminal_hook_effect_binding: None,
            };
        let thread_id = RuntimeThreadId::new("candidate-runtime").expect("runtime thread");
        let binding_id = RuntimeBindingId::new("candidate-binding").expect("binding");

        let result = compile_exact_runtime_surface_candidate(
            &compiler,
            &request,
            &thread_id,
            &binding_id,
            frame_id,
        )
        .await;
        let error = match result {
            Ok(_) => panic!("recording compiler must stop after observing the target"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("stop after recording target"));
        assert_eq!(
            *compiler.target.lock().expect("load compile target"),
            Some(
                agentdash_infrastructure::agent_runtime_composition::NativeAgentRunSurfaceCompileTarget::ExactAgentFrame(
                    frame_id,
                )
            )
        );
    }

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

    #[derive(Clone, Default)]
    struct FixtureToolInvocationFactory {
        tools: Vec<DynAgentTool>,
    }

    #[async_trait]
    impl AgentRunToolInvocationFactory for FixtureToolInvocationFactory {
        async fn build_tools(
            &self,
            _surface: &AgentRunRuntimeSurface,
            _executor: &agentdash_spi::AgentConfig,
            _coordinates: &agentdash_agent_runtime::ToolCallCoordinates,
            _hook_runtime: SharedHookRuntime,
            _identity: Option<agentdash_spi::AuthIdentity>,
        ) -> Result<Vec<DynAgentTool>, String> {
            Ok(self.tools.clone())
        }
    }

    fn fixture_tool_factory(tools: Vec<DynAgentTool>) -> Arc<dyn AgentRunToolInvocationFactory> {
        Arc::new(FixtureToolInvocationFactory { tools })
    }

    struct OwnerEchoTool {
        owner: serde_json::Value,
    }

    #[async_trait]
    impl agentdash_spi::AgentTool for OwnerEchoTool {
        fn name(&self) -> &str {
            "owner_echo"
        }

        fn description(&self) -> &str {
            "Echo the final invocation owner"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "additionalProperties": false})
        }

        async fn execute(
            &self,
            _tool_use_id: &str,
            _args: serde_json::Value,
            _cancel: tokio_util::sync::CancellationToken,
            _update: Option<agentdash_spi::ToolUpdateCallback>,
        ) -> Result<agentdash_spi::AgentToolResult, agentdash_spi::AgentToolError> {
            Ok(agentdash_spi::AgentToolResult {
                content: vec![agentdash_spi::ContentPart::text("typed invocation owner")],
                details: Some(self.owner.clone()),
                is_error: false,
            })
        }
    }

    #[derive(Default)]
    struct OwnerEchoToolInvocationFactory {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl AgentRunToolInvocationFactory for OwnerEchoToolInvocationFactory {
        async fn build_tools(
            &self,
            surface: &AgentRunRuntimeSurface,
            _executor: &agentdash_spi::AgentConfig,
            coordinates: &agentdash_agent_runtime::ToolCallCoordinates,
            _hook_runtime: SharedHookRuntime,
            _identity: Option<agentdash_spi::AuthIdentity>,
        ) -> Result<Vec<DynAgentTool>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![Arc::new(OwnerEchoTool {
                owner: serde_json::json!({
                    "run_id": surface.run_id,
                    "project_id": surface.project_id,
                    "agent_id": surface.agent_id,
                    "frame_id": surface.current_surface_frame_id,
                    "launch_evidence_frame_id": surface.launch_evidence_frame_id,
                    "runtime_thread_id": coordinates.thread_id,
                    "presentation_thread_id": surface.presentation_thread_id,
                    "runtime_turn_id": coordinates.turn_id,
                    "runtime_item_id": coordinates.item_id,
                    "presentation_item_id": coordinates.presentation_item_id,
                    "source_thread_id": coordinates.source_thread_id,
                    "source_turn_id": coordinates.source_turn_id,
                    "source_item_id": coordinates.source_item_id,
                    "binding_id": coordinates.binding_id,
                    "binding_generation": coordinates.binding_generation.0,
                    "tool_set_revision": coordinates.tool_set_revision.0,
                }),
            })])
        }
    }

    struct UnusedCapabilityPort;

    #[async_trait]
    impl AgentRunEffectiveCapabilityPort for UnusedCapabilityPort {
        async fn effective_capability(
            &self,
            _request: agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityRequest,
        ) -> Result<
            agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityView,
            agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityError,
        > {
            panic!("VFS policy test must not query capability projection")
        }

        async fn admit_tool(
            &self,
            _request: AgentRunAdmissionRequest,
        ) -> Result<
            agentdash_application_ports::agent_run_surface::AgentRunAdmissionDecision,
            agentdash_application_ports::agent_run_surface::AgentRunEffectiveCapabilityError,
        > {
            panic!("VFS policy test must not query capability admission")
        }
    }

    fn fixture_runtime_surface(
        runtime_session_id: &str,
        frame_id: uuid::Uuid,
        revision: u64,
    ) -> AgentRunRuntimeSurface {
        AgentRunRuntimeSurface {
            runtime_session_id: runtime_session_id.to_string(),
            presentation_thread_id: format!("presentation-{runtime_session_id}")
                .parse()
                .expect("fixture presentation thread"),
            run_id: uuid::Uuid::nil(),
            project_id: uuid::Uuid::nil(),
            agent_id: uuid::Uuid::nil(),
            runtime_address:
                agentdash_application_ports::agent_run_surface::AgentRunRuntimeAddress {
                    run_id: uuid::Uuid::nil(),
                    agent_id: uuid::Uuid::nil(),
                    frame_id,
                },
            launch_evidence_frame_id: frame_id,
            current_surface_frame_id: frame_id,
            surface_revision: i32::try_from(revision).unwrap(),
            capability_state: CapabilityState::default(),
            vfs: agentdash_spi::Vfs::default(),
            vfs_access_policy: agentdash_spi::RuntimeVfsAccessPolicy::default(),
            mcp_servers: Vec::new(),
            runtime_backend_anchor: None,
            active_turn_id: None,
            identity: None,
            provenance:
                agentdash_application_ports::agent_run_surface::AgentRunRuntimeSurfaceProvenance {
                    launch_evidence_frame_id: frame_id,
                    launch_created_by_kind: "fixture".to_string(),
                    current_surface_frame_id: frame_id,
                    surface_revision: i32::try_from(revision).unwrap(),
                    surface_created_by_kind: "fixture".to_string(),
                    anchor_updated_at: chrono::Utc::now(),
                    orchestration_id: None,
                    node_path: None,
                    node_attempt: None,
                },
            closure:
                agentdash_application_ports::agent_run_surface::AgentRunRuntimeSurfaceClosure {
                    capability_field_present: true,
                    vfs_field_present: true,
                    mcp_field_present: true,
                },
        }
    }

    #[test]
    fn hook_semantic_notice_derives_platform_metadata() {
        let facts = hook_context_presentation_facts(
            agentdash_spi::hooks::HookContextPresentationFacts::SystemNotice {
                title: "Hook Notice".to_string(),
                summary: "continue".to_string(),
                body: Some("继续处理".to_string()),
            },
        )
        .expect("semantic notice");

        assert_eq!(
            facts.kind,
            agentdash_agent_protocol::ContextFrameKind::SystemNotice
        );
        assert_eq!(
            facts.delivery_status,
            agentdash_agent_protocol::ContextDeliveryStatus::QueuedForTransformContext
        );
        assert_eq!(
            facts.delivery_channel,
            agentdash_agent_protocol::ContextDeliveryChannel::TurnStart
        );
        assert_eq!(
            facts.message_role,
            agentdash_agent_protocol::ContextMessageRole::User
        );
        assert_eq!(facts.rendered_text, "继续处理");
    }

    #[test]
    fn hook_semantic_assignment_rejects_empty_injection() {
        let result = hook_context_presentation_facts(
            agentdash_spi::hooks::HookContextPresentationFacts::AssignmentInjection {
                title: "Assignment Context".to_string(),
                summary: "Hook injection".to_string(),
                injections: vec![agentdash_spi::HookInjection {
                    slot: "workflow".to_string(),
                    content: "  ".to_string(),
                    source: "hook:test".to_string(),
                }],
            },
        );
        assert!(matches!(
            result,
            Err(DriverHookCallbackError::ProtocolViolation { .. })
        ));
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

    struct FixtureCompiledBindingRecovery {
        registry: Weak<CompiledAgentRunToolRegistry>,
        binding: CompiledAgentRunToolBinding,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl CompiledAgentRunToolBindingRecovery for FixtureCompiledBindingRecovery {
        async fn recover(
            &self,
            _binding_id: &RuntimeBindingId,
        ) -> Result<(), AgentRunRuntimeSurfaceSourceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.registry
                .upgrade()
                .expect("fixture registry")
                .put(self.binding.clone())
                .await
        }
    }

    #[tokio::test]
    async fn missing_compiled_binding_is_recovered_once_from_the_configured_owner() {
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        let binding_id = RuntimeBindingId::new("binding-restart-recovery").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let run_id = uuid::Uuid::new_v4();
        let project_id = uuid::Uuid::new_v4();
        let agent_id = uuid::Uuid::new_v4();
        let launch_evidence_frame_id = uuid::Uuid::new_v4();
        let current_surface_frame_id = uuid::Uuid::new_v4();
        let orchestration_id = uuid::Uuid::new_v4();
        let applied = fixture_applied(binding_id.as_str(), 1);
        let runtime_thread_id = applied.runtime_thread_id.to_string();
        let presentation_thread_id = agentdash_agent_runtime_contract::PresentationThreadId::new(
            "presentation-restart-recovery",
        )
        .unwrap();
        let mut surface = fixture_runtime_surface(&runtime_thread_id, current_surface_frame_id, 1);
        surface.presentation_thread_id = presentation_thread_id.clone();
        surface.run_id = run_id;
        surface.project_id = project_id;
        surface.agent_id = agent_id;
        surface.runtime_address =
            agentdash_application_ports::agent_run_surface::AgentRunRuntimeAddress {
                run_id,
                agent_id,
                frame_id: current_surface_frame_id,
            };
        surface.launch_evidence_frame_id = launch_evidence_frame_id;
        surface.provenance.launch_evidence_frame_id = launch_evidence_frame_id;
        surface.provenance.current_surface_frame_id = current_surface_frame_id;
        surface.provenance.orchestration_id = Some(orchestration_id);
        surface.provenance.node_path = Some("root/recovered".to_string());
        surface.provenance.node_attempt = Some(3);
        let binding = CompiledAgentRunToolBinding {
            applied,
            runtime_session_id: runtime_thread_id.clone(),
            run_id,
            agent_id,
            frame_id: current_surface_frame_id,
            hook_runtime: fixture_hook_runtime(&runtime_thread_id),
            catalog: agentdash_agent_runtime::ToolCatalogRevision {
                revision: ToolSetRevision(1),
                digest: "catalog-restart-recovery".into(),
                tools: Vec::new(),
                mcp_servers: Vec::new(),
            },
            tool_factory: fixture_tool_factory(Vec::new()),
            surface,
            executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
            tool_names: BTreeSet::new(),
            terminal_hook_effect_binding: None,
        };
        registry
            .bind_recovery(Arc::new(FixtureCompiledBindingRecovery {
                registry: Arc::downgrade(&registry),
                binding,
                calls: calls.clone(),
            }))
            .unwrap();

        let (first, second) = tokio::join!(
            registry.recover_if_missing(&binding_id),
            registry.recover_if_missing(&binding_id)
        );
        let first = first.unwrap();
        let second = second.unwrap();
        assert_eq!(first.catalog.revision, ToolSetRevision(1));
        assert_eq!(second.catalog.revision, ToolSetRevision(1));
        for recovered in [&first, &second] {
            assert_eq!(recovered.run_id, run_id);
            assert_eq!(recovered.agent_id, agent_id);
            assert_eq!(recovered.frame_id, current_surface_frame_id);
            assert_eq!(recovered.surface.project_id, project_id);
            assert_eq!(recovered.surface.runtime_session_id, runtime_thread_id);
            assert_eq!(
                recovered.surface.presentation_thread_id,
                presentation_thread_id
            );
            assert_ne!(
                recovered.surface.runtime_session_id,
                recovered.surface.presentation_thread_id.as_str()
            );
            assert_eq!(
                recovered.surface.launch_evidence_frame_id,
                launch_evidence_frame_id
            );
            assert_eq!(
                recovered.surface.current_surface_frame_id,
                current_surface_frame_id
            );
            assert_eq!(
                recovered.surface.provenance.orchestration_id,
                Some(orchestration_id)
            );
            assert_eq!(
                recovered.surface.provenance.node_path.as_deref(),
                Some("root/recovered")
            );
            assert_eq!(recovered.surface.provenance.node_attempt, Some(3));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn registry_executor_rebuilds_final_tool_with_real_owner_surface() {
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        let binding_id = RuntimeBindingId::new("binding-real-owner").unwrap();
        let applied = fixture_applied(binding_id.as_str(), 7);
        let runtime_thread_id = applied.runtime_thread_id.clone();
        let presentation_thread_id =
            agentdash_agent_runtime_contract::PresentationThreadId::new("presentation-real-owner")
                .unwrap();
        let run_id = uuid::Uuid::new_v4();
        let project_id = uuid::Uuid::new_v4();
        let agent_id = uuid::Uuid::new_v4();
        let launch_frame_id = uuid::Uuid::new_v4();
        let current_frame_id = uuid::Uuid::new_v4();
        let mut surface = fixture_runtime_surface(runtime_thread_id.as_str(), current_frame_id, 7);
        surface.presentation_thread_id = presentation_thread_id.clone();
        surface.run_id = run_id;
        surface.project_id = project_id;
        surface.agent_id = agent_id;
        surface.runtime_address =
            agentdash_application_ports::agent_run_surface::AgentRunRuntimeAddress {
                run_id,
                agent_id,
                frame_id: current_frame_id,
            };
        surface.launch_evidence_frame_id = launch_frame_id;
        surface.current_surface_frame_id = current_frame_id;
        surface.provenance.launch_evidence_frame_id = launch_frame_id;
        surface.provenance.current_surface_frame_id = current_frame_id;
        let hook_runtime: SharedHookRuntime = Arc::new(AgentFrameHookRuntime::new(
            run_id,
            agent_id,
            current_frame_id,
            7,
            runtime_thread_id.to_string(),
            Arc::new(NoopExecutionHookProvider),
            agentdash_spi::AgentFrameHookSnapshot::default(),
        ));
        let factory = Arc::new(OwnerEchoToolInvocationFactory::default());
        registry
            .put(CompiledAgentRunToolBinding {
                applied: applied.clone(),
                runtime_session_id: runtime_thread_id.to_string(),
                run_id,
                agent_id,
                frame_id: current_frame_id,
                hook_runtime,
                catalog: agentdash_agent_runtime::ToolCatalogRevision {
                    revision: ToolSetRevision(7),
                    digest: "real-owner-catalog".into(),
                    tools: Vec::new(),
                    mcp_servers: Vec::new(),
                },
                tool_factory: factory.clone(),
                surface,
                executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
                tool_names: BTreeSet::from(["owner_echo".to_string()]),
                terminal_hook_effect_binding: None,
            })
            .await
            .unwrap();

        let turn_id = RuntimeTurnId::new("turn-real-owner").unwrap();
        let item_id = RuntimeItemId::new("item-real-owner").unwrap();
        let (updates, _updates_rx) = tokio::sync::mpsc::unbounded_channel();
        let result = RegistryToolExecutor {
            registry,
            authorization_identity: None,
        }
        .execute(agentdash_agent_runtime::ToolExecutionRequest {
            idempotency_key: item_id.clone(),
            invocation: agentdash_agent_runtime::ToolBrokerInvocation {
                coordinates: agentdash_agent_runtime::ToolCallCoordinates {
                    thread_id: runtime_thread_id.clone(),
                    turn_id: turn_id.clone(),
                    item_id: item_id.clone(),
                    presentation_item_id:
                        agentdash_agent_runtime_contract::PresentationItemId::new(
                            "turn-real-owner:tool-real-owner",
                        )
                        .unwrap(),
                    source_thread_id: agentdash_agent_runtime_contract::DriverThreadId::new(
                        "source-real-owner",
                    )
                    .unwrap(),
                    source_turn_id: agentdash_agent_runtime_contract::DriverTurnId::new(
                        "source-turn-real-owner",
                    )
                    .unwrap(),
                    source_item_id: agentdash_agent_runtime_contract::DriverItemId::new(
                        "source-item-real-owner",
                    )
                    .unwrap(),
                    binding_id: binding_id.clone(),
                    binding_generation: RuntimeDriverGeneration(1),
                    tool_set_revision: ToolSetRevision(7),
                },
                tool_name: "owner_echo".to_string(),
                arguments: serde_json::json!({}),
                timeout_ms: 1_000,
            },
            credentials: agentdash_agent_runtime::CredentialMaterial::new(BTreeMap::new()),
            cancellation: tokio_util::sync::CancellationToken::new(),
            updates,
        })
        .await
        .unwrap();

        assert_eq!(factory.calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.output["run_id"], run_id.to_string());
        assert_eq!(result.output["project_id"], project_id.to_string());
        assert_eq!(result.output["agent_id"], agent_id.to_string());
        assert_eq!(result.output["frame_id"], current_frame_id.to_string());
        assert_eq!(
            result.output["launch_evidence_frame_id"],
            launch_frame_id.to_string()
        );
        assert_eq!(
            result.output["runtime_thread_id"],
            runtime_thread_id.as_str()
        );
        assert_eq!(
            result.output["presentation_thread_id"],
            presentation_thread_id.as_str()
        );
        assert_ne!(
            result.output["runtime_thread_id"],
            result.output["presentation_thread_id"]
        );
        assert_eq!(result.output["runtime_turn_id"], turn_id.as_str());
        assert_eq!(result.output["runtime_item_id"], item_id.as_str());
        assert_eq!(
            result.output["presentation_item_id"],
            "turn-real-owner:tool-real-owner"
        );
        assert_eq!(result.output["source_thread_id"], "source-real-owner");
        assert_eq!(result.output["source_turn_id"], "source-turn-real-owner");
        assert_eq!(result.output["source_item_id"], "source-item-real-owner");
        assert_eq!(result.output["binding_id"], binding_id.as_str());
        assert_eq!(result.output["binding_generation"], 1);
        assert_eq!(result.output["tool_set_revision"], 7);
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
        let applied = fixture_applied("binding-malformed-update", 1);
        let runtime_session_id = applied.runtime_thread_id.to_string();
        let malformed_tool = Arc::new(MalformedUpdateTool) as DynAgentTool;
        registry
            .put(CompiledAgentRunToolBinding {
                applied: applied.clone(),
                runtime_session_id: runtime_session_id.clone(),
                run_id: uuid::Uuid::nil(),
                agent_id: uuid::Uuid::nil(),
                frame_id: uuid::Uuid::nil(),
                hook_runtime: fixture_hook_runtime(&runtime_session_id),
                catalog: agentdash_agent_runtime::ToolCatalogRevision {
                    revision: ToolSetRevision(1),
                    digest: "malformed-update".into(),
                    tools: Vec::new(),
                    mcp_servers: Vec::new(),
                },
                tool_factory: fixture_tool_factory(vec![malformed_tool]),
                surface: fixture_runtime_surface(&runtime_session_id, uuid::Uuid::nil(), 1),
                executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
                tool_names: BTreeSet::from(["malformed_update".to_string()]),
                terminal_hook_effect_binding: None,
            })
            .await
            .unwrap();
        let (updates, mut updates_rx) = tokio::sync::mpsc::unbounded_channel();
        let error = RegistryToolExecutor {
            registry,
            authorization_identity: None,
        }
        .execute(agentdash_agent_runtime::ToolExecutionRequest {
            idempotency_key: RuntimeItemId::new("malformed-update-item").unwrap(),
            invocation: agentdash_agent_runtime::ToolBrokerInvocation {
                coordinates: agentdash_agent_runtime::ToolCallCoordinates {
                    thread_id: applied.runtime_thread_id,
                    turn_id: RuntimeTurnId::new("malformed-update-turn").unwrap(),
                    item_id: RuntimeItemId::new("malformed-update-item").unwrap(),
                    presentation_item_id:
                        agentdash_agent_runtime_contract::PresentationItemId::new(
                            "turn_001:tool_001",
                        )
                        .unwrap(),
                    source_thread_id: agentdash_agent_runtime_contract::DriverThreadId::new(
                        "source-malformed-update",
                    )
                    .unwrap(),
                    source_turn_id: agentdash_agent_runtime_contract::DriverTurnId::new(
                        "source-turn-malformed-update",
                    )
                    .unwrap(),
                    source_item_id: agentdash_agent_runtime_contract::DriverItemId::new(
                        "source-item-malformed-update",
                    )
                    .unwrap(),
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
                tool_factory: fixture_tool_factory(Vec::new()),
                surface: fixture_runtime_surface("presentation-applied", uuid::Uuid::nil(), 1),
                executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
                tool_names: BTreeSet::new(),
                terminal_hook_effect_binding: None,
            })
            .await
            .unwrap();
        let pending_session = "thread-binding-atomic-adopt";
        let pending = PendingCompiledAgentRunToolBinding {
            registry: registry.clone(),
            runtime_session_id: pending_session.into(),
            run_id: uuid::Uuid::nil(),
            agent_id: uuid::Uuid::nil(),
            frame_id: uuid::Uuid::nil(),
            hook_runtime: fixture_hook_runtime(pending_session),
            catalog: agentdash_agent_runtime::ToolCatalogRevision {
                revision: ToolSetRevision(2),
                digest: "catalog-pending".into(),
                tools: Vec::new(),
                mcp_servers: Vec::new(),
            },
            tool_factory: fixture_tool_factory(Vec::new()),
            surface: fixture_runtime_surface(pending_session, uuid::Uuid::nil(), 2),
            executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
            tool_names: BTreeSet::new(),
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
        let applied = fixture_applied("binding-reservation", 1);
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
            tool_factory: fixture_tool_factory(Vec::new()),
            surface: fixture_runtime_surface(session, uuid::Uuid::nil(), 1),
            executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
            tool_names: BTreeSet::new(),
        };
        let first = pending(applied.runtime_thread_id.as_str());
        let conflicting = pending("presentation-reservation-b");

        let reservation = first.reserve(applied.clone()).await.unwrap();
        assert!(conflicting.reserve(applied.clone()).await.is_err());
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
            applied.runtime_thread_id.to_string()
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
        let tool_factory = fixture_tool_factory(Vec::new());
        let surface = fixture_runtime_surface("presentation-thread-1", frame_id, 1);
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
            tool_factory: tool_factory.clone(),
            surface: surface.clone(),
            executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
            tool_names: BTreeSet::new(),
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
        let runtime_session_id = applied.runtime_thread_id.to_string();
        let pending = PendingCompiledAgentRunToolBinding {
            registry: registry.clone(),
            runtime_session_id: runtime_session_id.clone(),
            run_id: uuid::Uuid::nil(),
            agent_id: uuid::Uuid::nil(),
            frame_id: uuid::Uuid::nil(),
            hook_runtime: fixture_hook_runtime(&runtime_session_id),
            catalog: agentdash_agent_runtime::ToolCatalogRevision {
                revision: applied.tool_set_revision,
                digest: "catalog-terminal-effect".into(),
                tools: Vec::new(),
                mcp_servers: Vec::new(),
            },
            tool_factory: fixture_tool_factory(Vec::new()),
            surface: fixture_runtime_surface(&runtime_session_id, uuid::Uuid::nil(), 3),
            executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
            tool_names: BTreeSet::new(),
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
        let tool_factory = fixture_tool_factory(Vec::new());
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
            tool_factory: tool_factory.clone(),
            surface: fixture_runtime_surface(
                "presentation-hook-fence",
                uuid::Uuid::nil(),
                revision,
            ),
            executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
            tool_names: BTreeSet::new(),
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
        let error = agentdash_application_agentrun::agent_run::project_tool_protocol(
            &MissingProjectorTool,
            "missing_projector",
        )
        .expect_err("missing projector must fail admission");
        assert!(
            error
                .to_string()
                .contains("no owner-declared protocol projector")
        );
    }

    #[test]
    fn business_surface_rejects_tool_without_main_parity_fixture() {
        let error = agentdash_application_agentrun::agent_run::project_tool_protocol(
            &MissingFixtureTool,
            "missing_fixture",
        )
        .expect_err("missing fixture must fail admission");
        assert!(
            error
                .to_string()
                .contains("no owner-declared main parity fixture")
        );
    }

    #[tokio::test]
    async fn vfs_policy_denies_before_owner_executor_dispatch() {
        let contribution = agentdash_agent_runtime::ToolContribution {
            meta: agentdash_agent_runtime::ContributionMeta {
                key: "tool:fs-read-denied".into(),
                source: agentdash_agent_runtime::SurfaceSourceRef {
                    layer: "platform".into(),
                    key: "vfs".into(),
                },
                priority: 1,
                requirement: agentdash_agent_runtime::ContributionRequirement::Required,
            },
            runtime_name: "fs_read".into(),
            description: "fixture".into(),
            parameters_schema: serde_json::json!({"type":"object"}),
            capability_key: "file_read".into(),
            tool_path: "vfs::fs_read".into(),
            allowed_channels: BTreeSet::from([ToolChannel::DirectCallback]),
            configuration_boundary: ConfigurationBoundary::Binding,
            protocol_projection: ToolProtocolProjection::FsRead,
            presentation_emitter:
                agentdash_agent_runtime_contract::ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: "main_tool_fs_read_denied".into(),
        };
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        let applied = fixture_applied("binding-vfs-deny", 1);
        let runtime_session_id = applied.runtime_thread_id.to_string();
        registry
            .put(CompiledAgentRunToolBinding {
                applied: applied.clone(),
                runtime_session_id: runtime_session_id.clone(),
                run_id: uuid::Uuid::nil(),
                agent_id: uuid::Uuid::nil(),
                frame_id: uuid::Uuid::nil(),
                hook_runtime: fixture_hook_runtime(&runtime_session_id),
                catalog: agentdash_agent_runtime::ToolCatalogRevision {
                    revision: ToolSetRevision(1),
                    digest: "vfs-deny".into(),
                    tools: vec![contribution.clone()],
                    mcp_servers: Vec::new(),
                },
                tool_factory: fixture_tool_factory(Vec::new()),
                surface: fixture_runtime_surface(&runtime_session_id, uuid::Uuid::nil(), 1),
                executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
                tool_names: BTreeSet::from(["fs_read".to_string()]),
                terminal_hook_effect_binding: None,
            })
            .await
            .unwrap();
        let invocation = agentdash_agent_runtime::ToolBrokerInvocation {
            coordinates: agentdash_agent_runtime::ToolCallCoordinates {
                thread_id: applied.runtime_thread_id,
                turn_id: RuntimeTurnId::new("turn-vfs-deny").unwrap(),
                item_id: RuntimeItemId::new("item-vfs-deny").unwrap(),
                presentation_item_id: agentdash_agent_runtime_contract::PresentationItemId::new(
                    "turn_001:tool_001",
                )
                .unwrap(),
                source_thread_id: agentdash_agent_runtime_contract::DriverThreadId::new(
                    "source-vfs-deny",
                )
                .unwrap(),
                source_turn_id: agentdash_agent_runtime_contract::DriverTurnId::new(
                    "source-turn-vfs-deny",
                )
                .unwrap(),
                source_item_id: agentdash_agent_runtime_contract::DriverItemId::new(
                    "source-item-vfs-deny",
                )
                .unwrap(),
                binding_id: applied.binding_id,
                binding_generation: applied.generation,
                tool_set_revision: applied.tool_set_revision,
            },
            tool_name: "fs_read".into(),
            arguments: serde_json::json!({"path":"secret.txt"}),
            timeout_ms: 1_000,
        };
        let policy = RegistryToolBrokerPolicy::new(
            registry,
            Arc::new(UnusedCapabilityPort),
            Arc::new(AllowAllAgentRunPermissionFacade),
        );

        let decision = agentdash_agent_runtime::ToolBrokerPolicyPort::authorize_vfs(
            &policy,
            &invocation,
            &contribution,
        )
        .await
        .unwrap();

        assert!(matches!(
            decision,
            agentdash_agent_runtime::ToolGuardDecision::Denied { .. }
        ));
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
        let (projection, fixture_id) =
            agentdash_application_agentrun::agent_run::project_tool_protocol(
                tool.as_ref(),
                tool.name(),
            )
            .unwrap();
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
            protocol_projection: projection,
            presentation_emitter:
                agentdash_agent_runtime_contract::ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: fixture_id,
        };
        let binding_id = RuntimeBindingId::new("binding-shell-owner").unwrap();
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        let applied = fixture_applied("binding-shell-owner", 1);
        let runtime_session_id = applied.runtime_thread_id.to_string();
        let tool_name = tool.name().to_string();
        registry
            .put(CompiledAgentRunToolBinding {
                applied: applied.clone(),
                runtime_session_id: runtime_session_id.clone(),
                run_id: uuid::Uuid::nil(),
                agent_id: uuid::Uuid::nil(),
                frame_id: uuid::Uuid::nil(),
                hook_runtime: fixture_hook_runtime(&runtime_session_id),
                catalog: agentdash_agent_runtime::ToolCatalogRevision {
                    revision: ToolSetRevision(1),
                    digest: "shell-owner".into(),
                    tools: vec![contribution.clone()],
                    mcp_servers: Vec::new(),
                },
                tool_factory: fixture_tool_factory(vec![tool]),
                surface: fixture_runtime_surface(&runtime_session_id, uuid::Uuid::nil(), 1),
                executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
                tool_names: BTreeSet::from([tool_name]),
                terminal_hook_effect_binding: None,
            })
            .await
            .unwrap();
        let (updates, _updates_rx) = tokio::sync::mpsc::unbounded_channel();
        let arguments = serde_json::json!({"command":"pwd"});
        let result = RegistryToolExecutor {
            registry,
            authorization_identity: None,
        }
        .execute(agentdash_agent_runtime::ToolExecutionRequest {
            idempotency_key: RuntimeItemId::new("shell-owner-item").unwrap(),
            invocation: agentdash_agent_runtime::ToolBrokerInvocation {
                coordinates: agentdash_agent_runtime::ToolCallCoordinates {
                    thread_id: applied.runtime_thread_id,
                    turn_id: RuntimeTurnId::new("shell-owner-turn").unwrap(),
                    item_id: RuntimeItemId::new("shell-owner-item").unwrap(),
                    presentation_item_id:
                        agentdash_agent_runtime_contract::PresentationItemId::new(
                            "turn_001:cmd_001",
                        )
                        .unwrap(),
                    source_thread_id: agentdash_agent_runtime_contract::DriverThreadId::new(
                        "source-shell-owner",
                    )
                    .unwrap(),
                    source_turn_id: agentdash_agent_runtime_contract::DriverTurnId::new(
                        "source-turn-shell-owner",
                    )
                    .unwrap(),
                    source_item_id: agentdash_agent_runtime_contract::DriverItemId::new(
                        "source-item-shell-owner",
                    )
                    .unwrap(),
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
        let (projection, fixture_id) =
            agentdash_application_agentrun::agent_run::project_tool_protocol(
                tool.as_ref(),
                tool.name(),
            )
            .unwrap();
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
            protocol_projection: projection,
            presentation_emitter:
                agentdash_agent_runtime_contract::ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: fixture_id,
        };
        let binding_id = RuntimeBindingId::new("binding-patch-owner").unwrap();
        let registry = Arc::new(CompiledAgentRunToolRegistry::default());
        let applied = fixture_applied("binding-patch-owner", 1);
        let runtime_session_id = applied.runtime_thread_id.to_string();
        let tool_name = tool.name().to_string();
        registry
            .put(CompiledAgentRunToolBinding {
                applied: applied.clone(),
                runtime_session_id: runtime_session_id.clone(),
                run_id: uuid::Uuid::nil(),
                agent_id: uuid::Uuid::nil(),
                frame_id: uuid::Uuid::nil(),
                hook_runtime: fixture_hook_runtime(&runtime_session_id),
                catalog: agentdash_agent_runtime::ToolCatalogRevision {
                    revision: ToolSetRevision(1),
                    digest: "patch-owner".into(),
                    tools: vec![contribution.clone()],
                    mcp_servers: Vec::new(),
                },
                tool_factory: fixture_tool_factory(vec![tool]),
                surface: fixture_runtime_surface(&runtime_session_id, uuid::Uuid::nil(), 1),
                executor: agentdash_spi::AgentConfig::new("PI_AGENT"),
                tool_names: BTreeSet::from([tool_name]),
                terminal_hook_effect_binding: None,
            })
            .await
            .unwrap();
        let patch = "*** Begin Patch\n*** Add File: main://src/new.rs\n+new\n*** Update File: main://src/lib.rs\n*** Move to: main://src/moved.rs\n@@\n-old\n+new\n*** Delete File: main://src/old.rs\n*** End Patch";
        let arguments = serde_json::json!({"patch":patch});
        let (updates, _updates_rx) = tokio::sync::mpsc::unbounded_channel();
        let execution = RegistryToolExecutor {
            registry,
            authorization_identity: None,
        }
        .execute(agentdash_agent_runtime::ToolExecutionRequest {
            idempotency_key: RuntimeItemId::new("patch-owner-item").unwrap(),
            invocation: agentdash_agent_runtime::ToolBrokerInvocation {
                coordinates: agentdash_agent_runtime::ToolCallCoordinates {
                    thread_id: applied.runtime_thread_id,
                    turn_id: RuntimeTurnId::new("patch-owner-turn").unwrap(),
                    item_id: RuntimeItemId::new("patch-owner-item").unwrap(),
                    presentation_item_id:
                        agentdash_agent_runtime_contract::PresentationItemId::new(
                            "turn_001:tool_001",
                        )
                        .unwrap(),
                    source_thread_id: agentdash_agent_runtime_contract::DriverThreadId::new(
                        "source-patch-owner",
                    )
                    .unwrap(),
                    source_turn_id: agentdash_agent_runtime_contract::DriverTurnId::new(
                        "source-turn-patch-owner",
                    )
                    .unwrap(),
                    source_item_id: agentdash_agent_runtime_contract::DriverItemId::new(
                        "source-item-patch-owner",
                    )
                    .unwrap(),
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
            agentdash_application_agentrun::agent_run::resolve_tool_capability(
                &state,
                "mounts_list",
            )
            .expect("canonical capability"),
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

        let error = agentdash_application_agentrun::agent_run::resolve_tool_capability(
            &state,
            "mounts_list",
        )
        .expect_err("excluded tool");
        assert!(error.to_string().contains("is not enabled"));
    }

    #[test]
    fn unknown_tool_does_not_inherit_the_only_enabled_capability() {
        let mut state = CapabilityState::from_clusters([ToolCluster::Read]);
        state
            .tool
            .capabilities
            .insert(ToolCapability::new(CAP_FILE_READ));

        let error = agentdash_application_agentrun::agent_run::resolve_tool_capability(
            &state,
            "unknown_runtime_tool",
        )
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
    fn compiled_context_materializes_driver_channels_and_typed_blocks_without_frames() {
        let meta = |key: &str, priority| agentdash_agent_runtime::ContributionMeta {
            key: key.to_string(),
            source: agentdash_agent_runtime::SurfaceSourceRef {
                layer: "test".to_string(),
                key: "frame".to_string(),
            },
            priority,
            requirement: agentdash_agent_runtime::ContributionRequirement::Required,
        };
        let context = agentdash_agent_runtime::ContextEnvelope {
            recipe: ContextRecipe {
                revision: ContextRecipeRevision(1),
                provenance: ContextProvenance {
                    settings_revision: ThreadSettingsRevision(1),
                    tool_set_revision: ToolSetRevision(1),
                },
                source_item_ids: Vec::new(),
            },
            instructions: agentdash_agent_runtime::InstructionPlan {
                entries: vec![
                    agentdash_agent_runtime::InstructionContribution {
                        meta: meta("system:first", 30),
                        channel: InstructionChannel::System,
                        content: "identity".to_string(),
                    },
                    agentdash_agent_runtime::InstructionContribution {
                        meta: meta("developer", 20),
                        channel: InstructionChannel::Developer,
                        content: "developer".to_string(),
                    },
                    agentdash_agent_runtime::InstructionContribution {
                        meta: meta("system:second", 10),
                        channel: InstructionChannel::System,
                        content: "guidelines".to_string(),
                    },
                ],
            },
            contributions: vec![agentdash_agent_runtime::ContextContribution {
                meta: meta("context:assignment", 0),
                blocks: vec![ContextBlock::Instruction {
                    text: "assignment".to_string(),
                }],
                minimum_strength: SemanticStrength::ObservedOnly,
            }],
            digest: "context-digest".to_string(),
        };

        let (instructions, blocks) = materialize_driver_context(&context);
        assert_eq!(
            instructions,
            vec![
                DriverInstructionSet {
                    channel: InstructionChannel::System,
                    entries: vec!["identity".to_string(), "guidelines".to_string()],
                },
                DriverInstructionSet {
                    channel: InstructionChannel::Developer,
                    entries: vec!["developer".to_string()],
                },
            ]
        );
        assert_eq!(
            blocks,
            vec![ContextBlock::Instruction {
                text: "assignment".to_string()
            }]
        );
    }

    #[test]
    fn tool_broker_hook_remains_in_runtime_plan_but_not_driver_admission() {
        let requirement = AgentFrameHookRequirement {
            definition_id: HookDefinitionId::new("workflow.tool_approval").unwrap(),
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
