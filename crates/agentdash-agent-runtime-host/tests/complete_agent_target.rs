use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use agentdash_agent_runtime::{
    CompleteAgentStateReconciler, CompleteAgentStateRepository,
    RecordingCompleteAgentStateRepository, bind_complete_agent_surface,
};
use agentdash_agent_runtime_host::{
    CompleteAgentBinding, CompleteAgentBindingId, CompleteAgentBindingState,
    CompleteAgentCallbackBroker, CompleteAgentCallbackRoute, CompleteAgentHookHandler,
    CompleteAgentHost, CompleteAgentToolHandler, RecordingCompleteAgentHostRepository,
    RecordingCompleteAgentServiceRegistry,
};
use agentdash_agent_service_api::*;
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;

#[tokio::test]
async fn target_lane_runs_surface_command_state_sync_and_reverse_callback() {
    let source = AgentSourceCoordinate::new("source-1").expect("source");
    let service = Arc::new(FixtureService::new(source.clone()));
    let service_id = AgentServiceInstanceId::new("service-1").expect("service");
    let host = CompleteAgentHost::new(
        Arc::new(RecordingCompleteAgentHostRepository::new()),
        Arc::new(RecordingCompleteAgentServiceRegistry::new()),
    );
    let descriptor = host
        .register_service(service_id.clone(), service.clone())
        .await
        .expect("register service");
    let offer = host
        .runtime_offer(&service_id)
        .await
        .expect("runtime offer");
    let desired = desired_surface();
    let bound =
        bind_complete_agent_surface(&desired, &offer).expect("bind desired surface to offer");
    let binding_id = CompleteAgentBindingId::new("binding-1").expect("binding");
    host.register_binding(CompleteAgentBinding {
        id: binding_id.clone(),
        service_instance_id: service_id,
        generation: AgentBindingGeneration(1),
        source: source.clone(),
        profile_digest: descriptor.profile_digest,
        bound_surface: bound.clone(),
        applied_surface: None,
        state: CompleteAgentBindingState::PendingSurface,
    })
    .await
    .expect("register binding");
    let lease = host
        .acquire_binding_lease(
            &binding_id,
            AgentBindingGeneration(1),
            "worker-1",
            0,
            u64::MAX,
        )
        .await
        .expect("lease");
    let callback_binding = AgentHostCallbackBinding {
        route_id: AgentCallbackRouteId::new("callback-1").expect("route"),
        binding_generation: AgentBindingGeneration(1),
        delivery: AgentSurfaceRoute::AgentNativeCallback,
        default_deadline_ms: u64::MAX,
    };
    let apply = ApplyBoundAgentSurface {
        command_id: AgentCommandId::new("apply-command").expect("command"),
        effect_id: AgentEffectIdentity::new("apply-effect").expect("effect"),
        idempotency_key: AgentIdempotencyKey::new("apply-idem").expect("idempotency"),
        source: source.clone(),
        bound_surface: bound.clone(),
        callbacks: callback_binding.clone(),
    };
    let applied = host
        .apply_bound_surface(&lease, &binding_id, apply)
        .await
        .expect("apply surface");
    assert!(bound.accepts_applied(&applied.applied));
    assert_eq!(
        host.binding(&binding_id)
            .await
            .expect("read binding")
            .expect("binding")
            .state,
        CompleteAgentBindingState::Available
    );

    let receipt = host
        .dispatch_execute(
            &lease,
            &binding_id,
            AgentCommandEnvelope {
                meta: AgentCommandMeta {
                    command_id: AgentCommandId::new("input-command").expect("command"),
                    effect_id: AgentEffectIdentity::new("input-effect").expect("effect"),
                    idempotency_key: AgentIdempotencyKey::new("input-idem").expect("idempotency"),
                    binding_generation: AgentBindingGeneration(1),
                    expected_snapshot_revision: None,
                },
                source: source.clone(),
                command: AgentCommand::SubmitInput {
                    input: AgentInput {
                        content: vec![AgentInputContent::Text {
                            text: "hello".to_owned(),
                        }],
                    },
                },
            },
        )
        .await
        .expect("execute");
    assert!(matches!(
        receipt.state,
        AgentReceiptState::AlreadyApplied { .. }
    ));

    let state_repository = Arc::new(RecordingCompleteAgentStateRepository::new());
    let reconciler = CompleteAgentStateReconciler::new(state_repository.clone());
    let sync = reconciler
        .synchronize_source(service.as_ref(), source.clone(), 32)
        .await
        .expect("source sync");
    assert!(sync.reloaded_snapshot);
    assert_eq!(sync.projection.applied_surface, Some(applied.applied));
    assert_eq!(
        state_repository
            .platform_changes(&source, 0, 32)
            .await
            .expect("platform changes")
            .changes
            .len(),
        1
    );

    let tool_handler = Arc::new(CountingToolHandler::default());
    let callback_broker =
        CompleteAgentCallbackBroker::new(tool_handler.clone(), Arc::new(AllowHookHandler));
    callback_broker
        .register_route(
            CompleteAgentCallbackRoute::from_binding(callback_binding, source.clone(), bound)
                .expect("callback route"),
        )
        .await
        .expect("register callback route");
    let tool_call = AgentToolInvocation {
        meta: AgentHostCallbackMeta {
            route_id: AgentCallbackRouteId::new("callback-1").expect("route"),
            binding_generation: AgentBindingGeneration(1),
            source,
            turn_id: AgentTurnId::new("turn-1").expect("turn"),
            item_id: Some(AgentItemId::new("item-1").expect("item")),
            interaction_id: None,
            effect_id: AgentEffectIdentity::new("tool-effect").expect("effect"),
            idempotency_key: AgentIdempotencyKey::new("tool-idem").expect("idempotency"),
            deadline_at_ms: u64::MAX,
        },
        tool: AgentToolName::new("echo").expect("tool"),
        arguments: json!({"text": "hello"}),
    };
    let first = callback_broker
        .invoke_tool(tool_call.clone())
        .await
        .expect("tool callback");
    let replay = callback_broker
        .invoke_tool(tool_call)
        .await
        .expect("tool callback replay");
    assert_eq!(first, replay);
    assert_eq!(tool_handler.calls.load(Ordering::SeqCst), 1);
}

struct FixtureService {
    descriptor: AgentServiceDescriptor,
    source: AgentSourceCoordinate,
    applied_surface: Mutex<Option<AppliedAgentSurface>>,
}

impl FixtureService {
    fn new(source: AgentSourceCoordinate) -> Self {
        let tool = AgentToolSemanticFacet {
            delivery: AgentToolDelivery::AgentNativeCallback,
            invocation: SemanticFidelity::Exact,
            update: AgentToolUpdateSemantics::BindingOnly,
        };
        Self {
            descriptor: AgentServiceDescriptor {
                definition_id: AgentServiceDefinitionId::new("fixture").expect("definition"),
                title: "Fixture".to_owned(),
                protocol_revision: 1,
                profile: AgentCapabilityProfile {
                    lifecycle: BTreeSet::from([
                        AgentLifecycleCapability::Create,
                        AgentLifecycleCapability::Resume,
                    ]),
                    commands: BTreeSet::from([AgentCommandCapability::SubmitInput]),
                    fork: AgentForkCapability {
                        cutoffs: BTreeMap::new(),
                        lineage_fidelity: SemanticFidelity::Unsupported,
                        native_durability: SemanticFidelity::Unsupported,
                    },
                    compaction: BTreeMap::new(),
                    source_changes: AgentSourceChangeLevel::SnapshotOnly,
                    initial_context: InitialContextProfile {
                        contribution_fidelity: BTreeMap::new(),
                        applied_evidence: InitialContextAppliedEvidence::PackageDigest,
                        renderer_versions: BTreeSet::new(),
                    },
                    surface: AgentSurfaceProfile {
                        facets: vec![AgentSurfaceCapabilityFacet {
                            semantics: AgentSurfaceSemanticFacet::Tool(tool),
                            routes: BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
                            fidelity: SemanticFidelity::Exact,
                            configuration_boundary: AgentConfigurationBoundary::Binding,
                        }],
                    },
                    inspect_effects: SemanticFidelity::Exact,
                },
                profile_digest: AgentProfileDigest::new("profile-1").expect("profile"),
                configuration_boundary: AgentConfigurationBoundary::Binding,
            },
            source,
            applied_surface: Mutex::new(None),
        }
    }
}

#[async_trait]
impl CompleteAgentService for FixtureService {
    async fn describe(&self) -> Result<AgentServiceDescriptor, AgentServiceError> {
        Ok(self.descriptor.clone())
    }

    async fn create(
        &self,
        _command: CreateAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        Err(unsupported())
    }

    async fn resume(
        &self,
        _command: ResumeAgentCommand,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        Err(unsupported())
    }

    async fn fork(
        &self,
        _command: ForkAgentCommand,
    ) -> Result<ForkAgentReceipt, AgentServiceError> {
        Err(unsupported())
    }

    async fn execute(
        &self,
        command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        Ok(AgentCommandReceipt {
            command_id: command.meta.command_id,
            effect_id: command.meta.effect_id,
            source: command.source,
            state: AgentReceiptState::AlreadyApplied { terminal: None },
            snapshot_revision: Some(AgentSnapshotRevision(2)),
            initial_context: None,
        })
    }

    async fn read(&self, query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError> {
        Ok(AgentSnapshot {
            source: query.source,
            revision: AgentSnapshotRevision(2),
            lifecycle: AgentLifecycleStatus::Active,
            active_turn_id: None,
            turns: Vec::new(),
            interactions: Vec::new(),
            source_info: AgentSnapshotSource {
                authority: AgentSnapshotAuthority::AgentAuthoritative,
                source_revision: Some(
                    AgentSourceRevision::new("source-revision-2").expect("revision"),
                ),
                fidelity: SemanticFidelity::Exact,
                observed_at_ms: 2,
            },
            applied_surface: self.applied_surface.lock().await.clone(),
            initial_context: None,
        })
    }

    async fn changes(
        &self,
        _query: AgentChangesQuery,
    ) -> Result<AgentChangePage, AgentServiceError> {
        Err(unsupported())
    }

    async fn inspect(
        &self,
        identity: AgentEffectIdentity,
    ) -> Result<AgentEffectInspection, AgentServiceError> {
        Ok(AgentEffectInspection {
            effect_id: identity,
            command_id: None,
            state: AgentEffectInspectionState::Applied {
                source: self.source.clone(),
                terminal: None,
                initial_context: None,
                child_source: None,
            },
        })
    }

    async fn apply_surface(
        &self,
        command: ApplyBoundAgentSurface,
    ) -> Result<AppliedAgentSurfaceReceipt, AgentServiceError> {
        let applied = AppliedAgentSurface {
            revision: command.bound_surface.revision,
            digest: command.bound_surface.digest.clone(),
            contributions: command
                .bound_surface
                .contributions
                .iter()
                .map(|contribution| AppliedAgentSurfaceContribution {
                    key: contribution.key.clone(),
                    route: contribution.route,
                    fidelity: contribution.fidelity,
                    semantics: contribution.semantics.clone(),
                    payload_digest: contribution.payload_digest.clone(),
                    status: AppliedContributionStatus::Applied,
                    evidence: Some("fixture".to_owned()),
                })
                .collect(),
        };
        *self.applied_surface.lock().await = Some(applied.clone());
        Ok(AppliedAgentSurfaceReceipt {
            command_id: command.command_id,
            effect_id: command.effect_id,
            source: command.source,
            applied,
        })
    }

    async fn revoke_surface(
        &self,
        _command: RevokeBoundAgentSurface,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        Err(unsupported())
    }
}

fn desired_surface() -> AgentSurfaceSnapshot {
    AgentSurfaceSnapshot {
        revision: AgentSurfaceRevision(1),
        digest: AgentSurfaceDigest::new("surface-1").expect("surface"),
        requirements: vec![AgentSurfaceRequirement {
            key: "tool:echo".to_owned(),
            required: true,
            minimum_fidelity: SemanticFidelity::Exact,
            allowed_routes: BTreeSet::from([AgentSurfaceRoute::AgentNativeCallback]),
            semantics: AgentSurfaceSemanticFacet::Tool(AgentToolSemanticFacet {
                delivery: AgentToolDelivery::AgentNativeCallback,
                invocation: SemanticFidelity::Exact,
                update: AgentToolUpdateSemantics::BindingOnly,
            }),
            payload: AgentSurfaceContributionPayload::Tool {
                name: AgentToolName::new("echo").expect("tool"),
                description: "Echo".to_owned(),
                input_schema: json!({"type": "object"}),
                output_schema: Some(json!({"type": "object"})),
            },
            payload_digest: AgentPayloadDigest::new("tool-payload").expect("payload"),
        }],
    }
}

#[derive(Default)]
struct CountingToolHandler {
    calls: AtomicUsize,
}

#[async_trait]
impl CompleteAgentToolHandler for CountingToolHandler {
    async fn invoke(
        &self,
        invocation: AgentToolInvocation,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(AgentToolResult::Completed {
            output: invocation.arguments,
        })
    }
}

struct AllowHookHandler;

#[async_trait]
impl CompleteAgentHookHandler for AllowHookHandler {
    async fn invoke(
        &self,
        _invocation: AgentHookInvocation,
    ) -> Result<AgentHookDecision, AgentHostCallbackError> {
        Ok(AgentHookDecision::Allow)
    }
}

fn unsupported() -> AgentServiceError {
    AgentServiceError::new(
        AgentServiceErrorCode::Unsupported,
        "not used by tracer",
        false,
    )
}
