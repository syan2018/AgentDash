use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use agentdash_agent_runtime_contract::RuntimeThreadId;
use agentdash_agent_runtime_host::{
    AgentCallbackClock, CompleteAgentCallbackBroker, CompleteAgentHookHandler, CompleteAgentHost,
    CompleteAgentPlacement, CompleteAgentRuntimeTargetProvisioningRequest,
    CompleteAgentRuntimeTargetRecoveryRequest, CompleteAgentServiceVerification,
    CompleteAgentToolHandler, CompleteAgentVerificationMethod, CompleteAgentVerifiedBuildEvidence,
    CompleteAgentVerifiedServiceRegistration, ProcessCompleteAgentLiveCatalog,
    ResolvedCompleteAgentHookCallback, ResolvedCompleteAgentToolCallback,
};
use agentdash_agent_service_api::*;
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;

#[tokio::test]
async fn route_is_process_local_and_restart_fences_old_callback() {
    let catalog = Arc::new(ProcessCompleteAgentLiveCatalog::new());
    let host = Arc::new(CompleteAgentHost::new(catalog.clone()));
    let source = AgentSourceCoordinate::new("source-1").unwrap();
    let service = Arc::new(FixtureService::new());
    let descriptor = service.descriptor.clone();
    let selection = host
        .attach_verified_service(
            verified_registration(
                AgentServiceInstanceId::new("service-1").unwrap(),
                &descriptor,
                "host-1",
            ),
            service,
        )
        .await
        .unwrap();
    let thread = RuntimeThreadId::new("thread-1").unwrap();
    let target = host
        .provision_runtime_target(CompleteAgentRuntimeTargetProvisioningRequest {
            idempotency_key: AgentIdempotencyKey::new("provision-1").unwrap(),
            request_digest: AgentPayloadDigest::new("request-1").unwrap(),
            runtime_thread_id: thread.clone(),
            target: selection.target,
            desired_surface: desired_surface(),
            callback_deadline_ms: 5_000,
        })
        .await
        .unwrap()
        .target;
    host.restore_runtime_source_route(
        &thread,
        source.clone(),
        AgentEffectIdentity::new("restore-1").unwrap(),
        "test".to_owned(),
        5_000,
    )
    .await
    .unwrap();

    let handler = Arc::new(CountingToolHandler::default());
    let callbacks = CompleteAgentCallbackBroker::with_clock(
        handler.clone(),
        Arc::new(AllowHookHandler),
        host,
        Arc::new(FixedClock(100)),
    );
    let call = tool_call(&target, source.clone());
    let result = AgentHostCallbacks::invoke_tool(&callbacks, call.clone())
        .await
        .unwrap();
    assert_eq!(
        result,
        AgentToolResult::Completed {
            output: json!({"message": "hello"})
        }
    );
    assert_eq!(handler.calls.load(Ordering::SeqCst), 1);

    let restarted = Arc::new(CompleteAgentHost::new(catalog));
    let restarted_callbacks = CompleteAgentCallbackBroker::with_clock(
        handler,
        Arc::new(AllowHookHandler),
        restarted,
        Arc::new(FixedClock(100)),
    );
    let error = AgentHostCallbacks::invoke_tool(&restarted_callbacks, call)
        .await
        .expect_err("old route must not survive Host restart");
    assert_eq!(error.code, AgentHostCallbackErrorCode::UnknownRoute);
}

#[tokio::test]
async fn surface_rebind_keeps_the_previous_generation_available_to_the_active_turn() {
    let catalog = Arc::new(ProcessCompleteAgentLiveCatalog::new());
    let host = Arc::new(CompleteAgentHost::new(catalog));
    let source = AgentSourceCoordinate::new("source-rebind").unwrap();
    let service = Arc::new(FixtureService::new());
    let selection = host
        .attach_verified_service(
            verified_registration(
                AgentServiceInstanceId::new("service-rebind").unwrap(),
                &service.descriptor,
                "host-rebind",
            ),
            service,
        )
        .await
        .unwrap();
    let thread = RuntimeThreadId::new("thread-rebind").unwrap();
    let first = host
        .provision_runtime_target(CompleteAgentRuntimeTargetProvisioningRequest {
            idempotency_key: AgentIdempotencyKey::new("provision-rebind").unwrap(),
            request_digest: AgentPayloadDigest::new("request-rebind").unwrap(),
            runtime_thread_id: thread.clone(),
            target: selection.target.clone(),
            desired_surface: desired_surface(),
            callback_deadline_ms: 5_000,
        })
        .await
        .unwrap()
        .target;
    host.restore_runtime_source_route(
        &thread,
        source.clone(),
        AgentEffectIdentity::new("restore-rebind").unwrap(),
        "test".to_owned(),
        5_000,
    )
    .await
    .unwrap();

    let old_call = tool_call(&first, source.clone());
    let mut next_surface = desired_surface();
    next_surface.revision = AgentSurfaceRevision(2);
    next_surface.digest = AgentSurfaceDigest::new("surface-2").unwrap();
    let recovered = host
        .prepare_runtime_surface_rebind(CompleteAgentRuntimeTargetRecoveryRequest {
            idempotency_key: AgentIdempotencyKey::new("prepare-rebind").unwrap(),
            request_digest: AgentPayloadDigest::new("request-rebind-2").unwrap(),
            runtime_thread_id: thread.clone(),
            expected_generation: first.generation,
            target: selection.target,
            desired_surface: next_surface,
            callback_deadline_ms: 5_000,
        })
        .await
        .unwrap();
    host.apply_prepared_runtime_surface(
        &thread,
        AgentEffectIdentity::new("apply-rebind").unwrap(),
        "test".to_owned(),
        5_000,
    )
    .await
    .unwrap();

    let handler = Arc::new(CountingToolHandler::default());
    let callbacks = CompleteAgentCallbackBroker::with_clock(
        handler.clone(),
        Arc::new(AllowHookHandler),
        host,
        Arc::new(FixedClock(100)),
    );
    AgentHostCallbacks::invoke_tool(&callbacks, old_call)
        .await
        .expect("the active turn keeps using its previous route");
    AgentHostCallbacks::invoke_tool(&callbacks, tool_call(&recovered.recovered_target, source))
        .await
        .expect("the next turn uses the new route");
    assert_eq!(handler.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn lost_attachment_reprovision_advances_only_process_generation() {
    let catalog = Arc::new(ProcessCompleteAgentLiveCatalog::new());
    let host = CompleteAgentHost::new(catalog);
    let service = Arc::new(FixtureService::new());
    let selection = host
        .attach_verified_service(
            verified_registration(
                AgentServiceInstanceId::new("service-1").unwrap(),
                &service.descriptor,
                "host-1",
            ),
            service,
        )
        .await
        .unwrap();
    let thread = RuntimeThreadId::new("thread-1").unwrap();
    let request = CompleteAgentRuntimeTargetProvisioningRequest {
        idempotency_key: AgentIdempotencyKey::new("provision-1").unwrap(),
        request_digest: AgentPayloadDigest::new("request-1").unwrap(),
        runtime_thread_id: thread.clone(),
        target: selection.target.clone(),
        desired_surface: desired_surface(),
        callback_deadline_ms: 5_000,
    };
    let first = host
        .provision_runtime_target(request.clone())
        .await
        .unwrap();
    assert_eq!(first.target.generation, AgentBindingGeneration(1));

    host.mark_target_bindings_lost(&selection.target)
        .await
        .unwrap();
    assert_eq!(
        host.lost_runtime_threads_for_profile(&selection.target.offer_profile_digest)
            .await
            .unwrap(),
        vec![thread]
    );
    let second = host.provision_runtime_target(request).await.unwrap();
    assert_eq!(second.target.generation, AgentBindingGeneration(2));
}

fn verified_registration(
    service_instance_id: AgentServiceInstanceId,
    descriptor: &AgentServiceDescriptor,
    host_incarnation_id: &str,
) -> CompleteAgentVerifiedServiceRegistration {
    CompleteAgentVerifiedServiceRegistration {
        instance_id: service_instance_id.clone(),
        descriptor: descriptor.clone(),
        placement: CompleteAgentPlacement::InProcess {
            host_incarnation_id: host_incarnation_id.to_owned(),
        },
        verification: CompleteAgentServiceVerification {
            service_instance_id,
            publisher_integration: "fixture-integration".to_owned(),
            service_version: "fixture-version".to_owned(),
            verifier_identity: "fixture-verifier".to_owned(),
            verifier_revision: "fixture-verifier-revision".to_owned(),
            method: CompleteAgentVerificationMethod::PinnedBuiltin,
            verified_profile_digest: descriptor.profile_digest.clone(),
            claimed_conformance_suite_revision: "fixture-conformance".to_owned(),
            verified_build: CompleteAgentVerifiedBuildEvidence {
                claimed_build_digest: AgentPayloadDigest::new("fixture-build").unwrap(),
                evidence_digest: AgentPayloadDigest::new("fixture-evidence").unwrap(),
            },
        },
        remote_binding: None,
    }
}

struct FixtureService {
    descriptor: AgentServiceDescriptor,
    applied_surface: Mutex<Option<AppliedAgentSurface>>,
}

impl FixtureService {
    fn new() -> Self {
        let tool = AgentToolSemanticFacet {
            delivery: AgentToolDelivery::AgentNativeCallback,
            invocation: SemanticFidelity::Exact,
            update: AgentToolUpdateSemantics::BindingOnly,
        };
        Self {
            descriptor: AgentServiceDescriptor {
                definition_id: AgentServiceDefinitionId::new("fixture").unwrap(),
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
                profile_digest: AgentProfileDigest::new("profile-1").unwrap(),
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
        _command: AgentCommandEnvelope,
    ) -> Result<AgentCommandReceipt, AgentServiceError> {
        Err(unsupported())
    }

    async fn read(&self, _query: AgentReadQuery) -> Result<AgentSnapshot, AgentServiceError> {
        Err(unsupported())
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
            state: AgentEffectInspectionState::NotApplied,
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
        digest: AgentSurfaceDigest::new("surface-1").unwrap(),
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
                name: AgentToolName::new("echo").unwrap(),
                description: "Echo".to_owned(),
                input_schema: json!({"type": "object"}),
                output_schema: Some(json!({"type": "object"})),
                protocol_projector: agentdash_agent_runtime::ToolProtocolProjector::Dynamic,
            },
            payload_digest: AgentPayloadDigest::new("tool-payload").unwrap(),
        }],
    }
}

fn tool_call(
    target: &agentdash_agent_runtime_host::CompleteAgentRuntimeTarget,
    source: AgentSourceCoordinate,
) -> AgentToolInvocation {
    AgentToolInvocation {
        meta: AgentHostCallbackMeta {
            route_id: target.callbacks.route_id.clone(),
            binding_generation: target.generation,
            source,
            turn_id: AgentTurnId::new("turn-1").unwrap(),
            item_id: None,
            interaction_id: None,
            effect_id: AgentEffectIdentity::new("tool-effect-1").unwrap(),
            idempotency_key: AgentIdempotencyKey::new("tool-idempotency-1").unwrap(),
            deadline_at_ms: 1_000,
        },
        tool: AgentToolName::new("echo").unwrap(),
        arguments: json!({"message": "hello"}),
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
        callback: ResolvedCompleteAgentToolCallback,
    ) -> Result<AgentToolResult, AgentHostCallbackError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
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

struct FixedClock(u64);

impl AgentCallbackClock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

fn unsupported() -> AgentServiceError {
    AgentServiceError::new(
        AgentServiceErrorCode::Unsupported,
        "not used by test",
        false,
    )
}
