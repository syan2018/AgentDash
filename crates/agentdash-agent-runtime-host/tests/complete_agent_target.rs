use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use agentdash_agent_runtime::{
    CompleteAgentRuntimeIdentityMap, CompleteAgentStateReconciler, ManagedRuntimeStateCommit,
    ManagedRuntimeStateRepository, ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError,
    apply_managed_runtime_state_commit, bind_complete_agent_surface,
};
use agentdash_agent_runtime_contract::{
    ManagedRuntimeAvailabilityEvidence, ManagedRuntimeCommandAvailability,
    ManagedRuntimeCommandKind, RuntimeProjectionRevision, RuntimeThreadId, SurfaceRevision,
};
use agentdash_agent_runtime_host::{
    CompleteAgentBinding, CompleteAgentBindingId, CompleteAgentBindingState,
    CompleteAgentCallbackBroker, CompleteAgentCallbackCommit, CompleteAgentCallbackRepository,
    CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError, CompleteAgentHookHandler,
    CompleteAgentHost, CompleteAgentHostCommit, CompleteAgentHostRepository,
    CompleteAgentHostSnapshot, CompleteAgentHostStoreError, CompleteAgentPlacement,
    CompleteAgentRuntimeTarget, CompleteAgentServiceRegistry, CompleteAgentServiceVerification,
    CompleteAgentToolHandler, CompleteAgentVerificationMethod, CompleteAgentVerifiedBuildEvidence,
    CompleteAgentVerifiedServiceRegistration, ResolvedCompleteAgentHookCallback,
    ResolvedCompleteAgentToolCallback, apply_complete_agent_callback_commit,
    apply_complete_agent_host_commit,
};
use agentdash_agent_service_api::*;
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::{Mutex, RwLock};

#[derive(Default)]
struct FixtureHostRepository {
    snapshot: Mutex<CompleteAgentHostSnapshot>,
}

#[async_trait]
impl CompleteAgentHostRepository for FixtureHostRepository {
    async fn load(&self) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
        Ok(self.snapshot.lock().await.clone())
    }

    async fn commit(
        &self,
        commit: CompleteAgentHostCommit,
    ) -> Result<CompleteAgentHostSnapshot, CompleteAgentHostStoreError> {
        let mut snapshot = self.snapshot.lock().await;
        apply_complete_agent_host_commit(&mut snapshot, commit)
    }
}

#[derive(Default)]
struct FixtureServiceRegistry {
    handles: RwLock<BTreeMap<AgentServiceInstanceId, Arc<dyn CompleteAgentService>>>,
}

#[async_trait]
impl CompleteAgentServiceRegistry for FixtureServiceRegistry {
    async fn attach(
        &self,
        instance_id: AgentServiceInstanceId,
        service: Arc<dyn CompleteAgentService>,
    ) {
        self.handles.write().await.insert(instance_id, service);
    }

    async fn resolve(
        &self,
        instance_id: &AgentServiceInstanceId,
    ) -> Option<Arc<dyn CompleteAgentService>> {
        self.handles.read().await.get(instance_id).cloned()
    }
}

#[derive(Default)]
struct FixtureManagedRuntimeStateRepository {
    snapshot: Mutex<ManagedRuntimeStateSnapshot>,
}

#[async_trait]
impl ManagedRuntimeStateRepository for FixtureManagedRuntimeStateRepository {
    async fn load(
        &self,
        _thread_id: &RuntimeThreadId,
    ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
        Ok(self.snapshot.lock().await.clone())
    }

    async fn commit(
        &self,
        commit: ManagedRuntimeStateCommit,
    ) -> Result<ManagedRuntimeStateSnapshot, ManagedRuntimeStateStoreError> {
        let mut snapshot = self.snapshot.lock().await;
        apply_managed_runtime_state_commit(&mut snapshot, commit)
    }
}

#[derive(Default)]
struct FixtureCallbackRepository {
    snapshot: Mutex<CompleteAgentCallbackSnapshot>,
}

#[async_trait]
impl CompleteAgentCallbackRepository for FixtureCallbackRepository {
    async fn load(&self) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError> {
        Ok(self.snapshot.lock().await.clone())
    }

    async fn commit(
        &self,
        commit: CompleteAgentCallbackCommit,
    ) -> Result<CompleteAgentCallbackSnapshot, CompleteAgentCallbackStoreError> {
        let mut snapshot = self.snapshot.lock().await;
        apply_complete_agent_callback_commit(&mut snapshot, commit)
    }
}

#[tokio::test]
async fn target_lane_runs_surface_command_state_sync_and_reverse_callback() {
    let source = AgentSourceCoordinate::new("source-1").expect("source");
    let service = Arc::new(FixtureService::new(source.clone()));
    let service_id = AgentServiceInstanceId::new("service-1").expect("service");
    let host_repository = Arc::new(FixtureHostRepository::default());
    let host = CompleteAgentHost::new(
        host_repository.clone(),
        Arc::new(FixtureServiceRegistry::default()),
    );
    let descriptor = host
        .register_verified_service(
            CompleteAgentVerifiedServiceRegistration {
                instance_id: service_id.clone(),
                placement: CompleteAgentPlacement::InProcess {
                    host_incarnation_id: "fixture-host".to_owned(),
                },
                verification: CompleteAgentServiceVerification {
                    service_instance_id: service_id.clone(),
                    publisher_integration: "fixture-integration".to_owned(),
                    service_version: "fixture-version".to_owned(),
                    verifier_identity: "fixture-verifier".to_owned(),
                    verifier_revision: "fixture-verifier-revision".to_owned(),
                    method: CompleteAgentVerificationMethod::PinnedBuiltin,
                    verified_profile_digest: service.descriptor.profile_digest.clone(),
                    claimed_conformance_suite_revision: "fixture-conformance".to_owned(),
                    verified_build: CompleteAgentVerifiedBuildEvidence {
                        claimed_build_digest: AgentPayloadDigest::new("fixture-build")
                            .expect("build digest"),
                        evidence_digest: AgentPayloadDigest::new("fixture-evidence")
                            .expect("evidence digest"),
                    },
                },
                remote_binding: None,
            },
            service.clone(),
        )
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
        service_instance_id: service_id.clone(),
        generation: AgentBindingGeneration(1),
        source: source.clone(),
        profile_digest: descriptor.profile_digest.clone(),
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
    let runtime_thread_id = RuntimeThreadId::new("runtime-thread").expect("runtime thread");
    host.register_runtime_target(CompleteAgentRuntimeTarget {
        runtime_thread_id: runtime_thread_id.clone(),
        service_instance_id: service_id,
        generation: AgentBindingGeneration(1),
        profile_digest: descriptor.profile_digest.clone(),
        bound_surface: bound.clone(),
        callbacks: callback_binding.clone(),
    })
    .await
    .expect("register Runtime target");
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
    let applied_host_snapshot = host_repository
        .load()
        .await
        .expect("Host snapshot after apply");
    let persisted_route = applied_host_snapshot
        .facts
        .callback_routes
        .get(&callback_binding.route_id)
        .expect("atomic callback route");
    assert_eq!(persisted_route.binding_id, binding_id);
    assert_eq!(
        persisted_route.generation,
        callback_binding.binding_generation
    );
    assert_eq!(persisted_route.bound_surface.digest, bound.digest);
    assert!(
        !applied_host_snapshot
            .facts
            .revoked_callback_routes
            .contains(&callback_binding.route_id)
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

    let state_repository = Arc::new(FixtureManagedRuntimeStateRepository::default());
    let mut identities =
        CompleteAgentRuntimeIdentityMap::new(source.clone(), runtime_thread_id.clone());
    identities
        .bind_surface_revision(AgentSurfaceRevision(1), SurfaceRevision(1))
        .expect("surface identity");
    let command_availability = ManagedRuntimeCommandKind::ALL
        .into_iter()
        .map(|command| {
            (
                command,
                ManagedRuntimeCommandAvailability::Available {
                    evidence: ManagedRuntimeAvailabilityEvidence {
                        decided_at_revision: RuntimeProjectionRevision(1),
                        blocking_operation_id: None,
                        bound_surface_revision: Some(SurfaceRevision(1)),
                        applied_surface_revision: Some(SurfaceRevision(1)),
                    },
                },
            )
        })
        .collect();
    let reconciler = CompleteAgentStateReconciler::new(
        state_repository.clone(),
        identities,
        command_availability,
    );
    let sync = reconciler
        .synchronize_source(service.as_ref(), source.clone(), 32)
        .await
        .expect("source sync");
    assert!(sync.reloaded_snapshot);
    assert_eq!(
        sync.projection.applied_surface,
        Some(applied.applied.clone())
    );
    assert_eq!(
        state_repository
            .load(&runtime_thread_id)
            .await
            .expect("managed Runtime state")
            .facts
            .source_changes
            .len(),
        1
    );

    let tool_handler = Arc::new(CountingToolHandler::default());
    let callback_repository = Arc::new(FixtureCallbackRepository::default());
    let callback_broker = CompleteAgentCallbackBroker::new(
        tool_handler.clone(),
        Arc::new(AllowHookHandler),
        host_repository.clone(),
        callback_repository.clone(),
    );
    let tool_call = AgentToolInvocation {
        meta: AgentHostCallbackMeta {
            route_id: AgentCallbackRouteId::new("callback-1").expect("route"),
            binding_generation: AgentBindingGeneration(1),
            source: source.clone(),
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
        .invoke_tool(tool_call.clone())
        .await
        .expect("tool callback replay");
    assert_eq!(first, replay);
    assert_eq!(tool_handler.calls.load(Ordering::SeqCst), 1);
    let callbacks = tool_handler.callbacks.lock().await;
    assert_eq!(callbacks.len(), 1);
    assert_eq!(callbacks[0].context.runtime_thread_id, runtime_thread_id);
    assert_eq!(callbacks[0].context.binding_id, binding_id);
    assert_eq!(
        callbacks[0].context.binding_generation,
        AgentBindingGeneration(1)
    );
    assert_eq!(callbacks[0].context.source, source);
    assert_eq!(
        callbacks[0].context.service_instance_id,
        AgentServiceInstanceId::new("service-1").expect("service")
    );
    assert_eq!(
        callbacks[0].context.profile_digest,
        descriptor.profile_digest
    );
    assert_eq!(callbacks[0].context.bound_surface_revision, bound.revision);
    assert_eq!(callbacks[0].context.bound_surface_digest, bound.digest);
    assert_eq!(
        callbacks[0].context.applied_surface_revision,
        applied.applied.revision
    );
    assert_eq!(
        callbacks[0].context.applied_surface_digest,
        applied.applied.digest
    );
    drop(callbacks);

    host.revoke_bound_surface(
        &lease,
        &binding_id,
        RevokeBoundAgentSurface {
            command_id: AgentCommandId::new("revoke-command").expect("command"),
            effect_id: AgentEffectIdentity::new("revoke-effect").expect("effect"),
            idempotency_key: AgentIdempotencyKey::new("revoke-idem").expect("idempotency"),
            binding_generation: AgentBindingGeneration(1),
            source,
            expected_revision: bound.revision,
        },
    )
    .await
    .expect("revoke surface");
    let revoked_host_snapshot = host_repository
        .load()
        .await
        .expect("Host snapshot after revoke");
    assert!(
        revoked_host_snapshot
            .facts
            .callback_routes
            .contains_key(&callback_binding.route_id),
        "the immutable route fence remains durable"
    );
    assert!(
        revoked_host_snapshot
            .facts
            .revoked_callback_routes
            .contains(&callback_binding.route_id),
        "revoke atomically tombstones the route"
    );

    let restarted_broker = CompleteAgentCallbackBroker::new(
        tool_handler.clone(),
        Arc::new(AllowHookHandler),
        host_repository,
        callback_repository,
    );
    let rejected = restarted_broker
        .invoke_tool(tool_call)
        .await
        .expect_err("old callback after revoke");
    assert_eq!(
        rejected.code,
        AgentHostCallbackErrorCode::StaleBindingGeneration
    );
    assert_eq!(tool_handler.calls.load(Ordering::SeqCst), 1);
}

struct FixtureService {
    descriptor: AgentServiceDescriptor,
    applied_surface: Mutex<Option<AppliedAgentSurface>>,
}

impl FixtureService {
    fn new(_source: AgentSourceCoordinate) -> Self {
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
            thread_name: None,
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
            state: AgentEffectInspectionState::Unknown,
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
        command: RevokeBoundAgentSurface,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        *self.applied_surface.lock().await = None;
        Ok(AgentCommandReceipt {
            command_id: command.command_id,
            effect_id: command.effect_id,
            source: command.source,
            state: AgentReceiptState::AlreadyApplied { terminal: None },
            snapshot_revision: None,
            initial_context: None,
        })
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
    callbacks: Mutex<Vec<ResolvedCompleteAgentToolCallback>>,
}

#[async_trait]
impl CompleteAgentToolHandler for CountingToolHandler {
    async fn invoke(
        &self,
        callback: ResolvedCompleteAgentToolCallback,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.callbacks.lock().await.push(callback.clone());
        Ok(AgentToolResult::Completed {
            output: callback.invocation.arguments,
        })
    }
}

struct AllowHookHandler;

#[async_trait]
impl CompleteAgentHookHandler for AllowHookHandler {
    async fn invoke(
        &self,
        _callback: ResolvedCompleteAgentHookCallback,
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
