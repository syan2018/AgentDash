use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use agentdash_agent_runtime::{ManagedAgentRuntime, RuntimeRepository, RuntimeStoreFixture};
use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_host::*;
use agentdash_integration_api::*;
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::json;
use tokio::sync::{Mutex, Notify};

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid test id")
}

#[tokio::test]
async fn coordinate_index_failure_degrades_host_without_replaying_committed_runtime_event() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    let binding = fixture
        .host
        .bind(BindRuntimeRequest {
            binding_id: id("binding-coordinate-health"),
            thread_id: id("thread-coordinate-health"),
            offer_id: offer.id,
            bound_surface: bound_surface(false),
            intent: DriverBindIntent::Start,
        })
        .await
        .expect("bind");
    let source_thread_id = binding.source_thread_id.clone().expect("source thread");
    let runtime_store = Arc::new(RuntimeStoreFixture::default());
    let runtime = Arc::new(ManagedAgentRuntime::new(runtime_store.clone()));
    runtime
        .execute(RuntimeCommandEnvelope {
            meta: OperationMeta {
                operation_id: id("runtime-coordinate-thread-start"),
                idempotency_key: id("runtime-coordinate-thread-start-key"),
                expected_thread_revision: None,
                actor: RuntimeActor::System {
                    component: "host-coordinate-test".to_string(),
                },
            },
            command: RuntimeCommand::ThreadStart {
                thread_id: binding.thread_id.clone(),
                binding_id: binding.id.clone(),
                driver_generation: binding.driver_generation,
                source_thread_id: source_thread_id.clone(),
                profile_digest: binding.profile_digest.clone(),
                bound_profile: Box::new(fixture.full_profile.clone()),
                input: Vec::new(),
                surface_digest: binding.bound_surface.digest.clone(),
                settings_revision: ThreadSettingsRevision(0),
                tool_set_revision: binding.bound_surface.tool_set_revision,
                hook_plan: BoundRuntimeHookPlan {
                    revision: HookPlanRevision(1),
                    digest: id("runtime-coordinate-hook-plan"),
                    entries: Vec::new(),
                },
            },
        })
        .await
        .expect("runtime thread start");
    runtime
        .execute(RuntimeCommandEnvelope {
            meta: OperationMeta {
                operation_id: id("runtime-coordinate"),
                idempotency_key: id("runtime-coordinate-key"),
                expected_thread_revision: Some(RuntimeRevision(3)),
                actor: RuntimeActor::System {
                    component: "host-coordinate-test".to_string(),
                },
            },
            command: RuntimeCommand::TurnStart {
                thread_id: binding.thread_id.clone(),
                input: Vec::new(),
            },
        })
        .await
        .expect("runtime turn start");
    *fixture.factory.driver.extra_events.lock().await = vec![
        DriverEventEnvelope {
            binding_id: binding.id.clone(),
            generation: binding.driver_generation,
            source_thread_id: source_thread_id.clone(),
            source_turn_id: Some(id("source-turn-coordinate")),
            source_item_id: Some(id("source-item-collision")),
            event: RuntimeEvent::ItemStarted {
                turn_id: id("turn-runtime-coordinate"),
                item_id: id("runtime-item-coordinate-a"),
                initial_content: RuntimeItemContent::agent_message(
                    "runtime-item-coordinate-a",
                    "first",
                ),
            },
        },
        DriverEventEnvelope {
            binding_id: binding.id.clone(),
            generation: binding.driver_generation,
            source_thread_id: source_thread_id.clone(),
            source_turn_id: Some(id("source-turn-coordinate")),
            source_item_id: Some(id("source-item-collision")),
            event: RuntimeEvent::ItemStarted {
                turn_id: id("turn-runtime-coordinate"),
                item_id: id("runtime-item-coordinate-b"),
                initial_content: RuntimeItemContent::agent_message(
                    "runtime-item-coordinate-b",
                    "second",
                ),
            },
        },
    ];
    let lease = fixture
        .host
        .acquire_driver_lease(&binding.id)
        .await
        .expect("lease");
    let sink = Arc::new(ManagedRuntimeSink {
        runtime: runtime.clone(),
        accepted: AtomicUsize::new(0),
    });
    let command = RouteDriverCommand {
        envelope: DriverCommandEnvelope {
            request_id: id("request-coordinate-health"),
            binding_id: binding.id.clone(),
            generation: binding.driver_generation,
            source_thread_id,
            runtime_turn_id: Some(id("turn-coordinate-health")),
            command: RuntimeCommand::TurnStart {
                thread_id: binding.thread_id.clone(),
                input: Vec::new(),
            },
        },
        lease_owner: lease.owner.clone(),
        lease_token: lease.token.clone(),
    };
    fixture
        .host
        .dispatch(command.clone(), sink.clone())
        .await
        .expect("authoritative events remain accepted");
    assert_eq!(sink.accepted.load(Ordering::SeqCst), 4);
    assert_eq!(
        fixture
            .repository
            .load_binding(&binding.id)
            .await
            .expect("binding")
            .expect("binding row")
            .state,
        RuntimeBindingState::Failed
    );
    assert!(matches!(
        fixture.host.dispatch(command, sink.clone()).await,
        Err(AgentRuntimeHostError::DispatchRejected { .. })
    ));
    assert_eq!(sink.accepted.load(Ordering::SeqCst), 4);
    let projection = runtime_store
        .load_thread(&binding.thread_id)
        .await
        .expect("runtime thread")
        .expect("runtime thread row");
    assert_eq!(projection.status, RuntimeThreadStatus::Lost);
    assert!(projection.active_turn_id.is_none());
    assert!(projection.items.values().all(|item| matches!(
        item.phase,
        agentdash_agent_runtime::EntityPhase::Terminal(RuntimeItemTerminal::Lost { .. })
    )));
    let operation = runtime_store
        .find_operation(&id("runtime-coordinate"))
        .await
        .expect("operation")
        .expect("operation row");
    assert!(matches!(
        operation.terminal,
        Some(RuntimeOperationTerminal::Lost { .. })
    ));
    let events = runtime_store
        .events_after(&binding.thread_id, None)
        .await
        .expect("runtime events")
        .events;
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                &event.event,
                RuntimeEvent::OperationTerminal { operation_id, .. }
                    if operation_id == &id("runtime-coordinate")
            ))
            .count(),
        1
    );
}

fn profile(hooks: Vec<HookPointCapability>) -> RuntimeProfile {
    RuntimeProfile {
        reference_class: ReferenceRuntimeClass::ManagedThread,
        input: InputProfile {
            modalities: BTreeSet::from([InputModality::Text]),
        },
        instruction: InstructionProfile {
            channels: BTreeSet::from([InstructionChannel::System]),
            configuration_boundary: ConfigurationBoundary::HotReplace,
        },
        tools: ToolProfile {
            channels: BTreeSet::from([ToolChannel::DirectCallback]),
            configuration_boundary: ConfigurationBoundary::HotReplace,
            cancellation: true,
        },
        workspace: WorkspaceProfile {
            capabilities: BTreeSet::from([WorkspaceCapability::Read]),
            mechanism: DeliveryMechanism::Native,
        },
        interactions: InteractionProfile {
            kinds: BTreeSet::new(),
            durable_correlation: true,
        },
        lifecycle: BTreeSet::from([
            LifecycleCapability::ThreadStart,
            LifecycleCapability::TurnStart,
        ]),
        hooks: HookProfile {
            points: hooks,
            configuration_boundary: ConfigurationBoundary::HotReplace,
        },
        context: ContextProfile {
            capabilities: BTreeSet::new(),
            fidelity: ContextFidelity::PlatformExact,
            activation_idempotent: true,
        },
        telemetry_config: BTreeSet::new(),
    }
}

fn exact_before_tool() -> HookPointCapability {
    HookPointCapability {
        point: HookPoint::BeforeTool,
        actions: BTreeSet::from([HookAction::Block, HookAction::ContinueTurn]),
        strength: SemanticStrength::ExactSynchronous,
        mechanism: DeliveryMechanism::Native,
        failure_policies: BTreeSet::from([HookFailurePolicy::FailClosed]),
        acknowledged: true,
    }
}

fn definition(factory_key: &str, runtime_profile: RuntimeProfile) -> AgentServiceDefinition {
    let schema = json!({
        "type": "object",
        "properties": { "endpoint": { "type": "string" } },
        "required": ["endpoint"],
        "additionalProperties": false
    });
    AgentServiceDefinition {
        provenance: AgentServiceProvenance {
            definition_id: AgentServiceDefinitionId::new("corp.agent").expect("definition id"),
            publisher_integration: "corp.integration".to_string(),
            service_version: "1.0.0".to_string(),
            build_digest: AgentServiceBuildDigest::new("sha256:build").expect("build digest"),
        },
        factory_key: AgentRuntimeFactoryKey::new(factory_key).expect("factory key"),
        supported_protocol_revisions: vec![1],
        config_schema_digest: AgentServiceSchemaDigest::new(schema_digest(&schema))
            .expect("schema digest"),
        config_schema: schema,
        credential_slots: vec![CredentialSlotDefinition {
            slot: AgentRuntimeCredentialSlot::new("endpoint_auth").expect("slot"),
            purpose: "runtime_transport".to_string(),
            required: true,
        }],
        service_profile_upper_bound: runtime_profile,
    }
}

struct TestCredentialBroker;

#[async_trait]
impl AgentRuntimeCredentialBroker for TestCredentialBroker {
    async fn resolve(
        &self,
        slot: &AgentRuntimeCredentialSlot,
        _reference: &AgentRuntimeCredentialRef,
        purpose: &str,
    ) -> Result<CredentialLease, CredentialResolveError> {
        Ok(CredentialLease {
            slot: slot.clone(),
            purpose: purpose.to_string(),
            secret: "secret".to_string(),
        })
    }
}

struct RejectingCredentialBroker;

#[async_trait]
impl AgentRuntimeCredentialBroker for RejectingCredentialBroker {
    async fn resolve(
        &self,
        slot: &AgentRuntimeCredentialSlot,
        _reference: &AgentRuntimeCredentialRef,
        _purpose: &str,
    ) -> Result<CredentialLease, CredentialResolveError> {
        Err(CredentialResolveError::Unavailable {
            slot: slot.clone(),
            reason: "test credential is unavailable".to_string(),
        })
    }
}

struct CountingCredentialBroker(AtomicUsize);

#[async_trait]
impl AgentRuntimeCredentialBroker for CountingCredentialBroker {
    async fn resolve(
        &self,
        slot: &AgentRuntimeCredentialSlot,
        _reference: &AgentRuntimeCredentialRef,
        purpose: &str,
    ) -> Result<CredentialLease, CredentialResolveError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(CredentialLease {
            slot: slot.clone(),
            purpose: purpose.to_string(),
            secret: "secret".to_string(),
        })
    }
}

struct UnexpectedSurfaceBroker;

#[async_trait]
impl AgentRuntimeSurfaceBroker for UnexpectedSurfaceBroker {
    async fn materialize(
        &self,
        _request: DriverSurfaceRequest,
    ) -> Result<MaterializedDriverSurface, DriverSurfaceError> {
        Err(DriverSurfaceError::Unavailable {
            reason: "test driver does not request a materialized surface".to_string(),
            retryable: false,
        })
    }

    async fn materialize_tool_set(
        &self,
        _binding_id: RuntimeBindingId,
        _revision: ToolSetRevision,
        _digest: &str,
    ) -> Result<DriverToolSurface, DriverSurfaceError> {
        Err(DriverSurfaceError::Unavailable {
            reason: "test driver does not request a tool surface".to_string(),
            retryable: false,
        })
    }
}

struct UnexpectedToolCallback;

struct UnexpectedContextBroker;

#[async_trait]
impl AgentRuntimeContextBroker for UnexpectedContextBroker {
    async fn load_checkpoint(
        &self,
        _request: DriverContextCheckpointRequest,
    ) -> Result<DriverContextActivation, DriverContextError> {
        Err(DriverContextError::NotFound)
    }

    async fn compaction_activation(
        &self,
        _request: DriverCompactionActivationRequest,
    ) -> Result<DriverContextActivation, DriverContextError> {
        Err(DriverContextError::NotFound)
    }
}

#[async_trait]
impl AgentRuntimeToolCallback for UnexpectedToolCallback {
    async fn invoke(
        &self,
        _request: DriverToolInvocation,
    ) -> Result<DriverToolOutcome, DriverToolCallbackError> {
        Err(DriverToolCallbackError::ProtocolViolation {
            reason: "test driver does not invoke tools".to_string(),
        })
    }
}

struct UnexpectedHookCallback;

#[async_trait]
impl AgentRuntimeHookCallback for UnexpectedHookCallback {
    async fn execute(
        &self,
        _request: DriverHookInvocation,
    ) -> Result<DriverHookDecision, DriverHookCallbackError> {
        Err(DriverHookCallbackError::ProtocolViolation {
            reason: "test driver does not execute hooks".to_string(),
        })
    }
}

fn test_host_ports(credentials: Arc<dyn AgentRuntimeCredentialBroker>) -> RuntimeDriverHostPorts {
    RuntimeDriverHostPorts {
        credentials,
        surfaces: Arc::new(UnexpectedSurfaceBroker),
        context: Arc::new(UnexpectedContextBroker),
        tools: Arc::new(UnexpectedToolCallback),
        hooks: Arc::new(UnexpectedHookCallback),
    }
}

struct TestDriver {
    descriptor: RuntimeDescriptor,
    bind_count: AtomicUsize,
    dispatch_count: AtomicUsize,
    emit_barrier: Mutex<Option<(Arc<Notify>, Arc<Notify>)>>,
    extra_events: Mutex<Vec<DriverEventEnvelope>>,
    mismatch_resume_source: AtomicBool,
}

#[async_trait]
impl AgentRuntimeDriver for TestDriver {
    async fn describe(
        &self,
        _request: DriverDescribeRequest,
    ) -> Result<RuntimeDescriptor, DriverError> {
        Ok(self.descriptor.clone())
    }

    async fn bind(&self, request: DriverBindRequest) -> Result<DriverBinding, DriverError> {
        self.bind_count.fetch_add(1, Ordering::SeqCst);
        let source_thread_id = match &request.intent {
            DriverBindIntent::Resume { .. }
                if self.mismatch_resume_source.load(Ordering::SeqCst) =>
            {
                id("mismatched-resume-source")
            }
            DriverBindIntent::Resume { source_thread_id } => source_thread_id.clone(),
            _ => id(&format!("source-{}", request.binding_id)),
        };
        Ok(DriverBinding {
            driver_binding_id: id(&format!("driver-{}", request.binding_id)),
            source_thread_id,
            applied_surface_revision: request.surface_revision,
            applied_surface_digest: request.surface_digest,
            applied_tool_set_revision: ToolSetRevision(1),
            applied_tool_set_digest: "sha256:tools".to_string(),
            applied_hook_plan_revision: Some(HookPlanRevision(2)),
            applied_hook_plan_digest: Some(id("sha256:hook-plan")),
            applied_hooks: Vec::new(),
        })
    }

    async fn dispatch(
        &self,
        command: DriverCommandEnvelope,
        sink: Arc<dyn DriverEventSink>,
    ) -> Result<DriverDispatchReceipt, DriverError> {
        self.dispatch_count.fetch_add(1, Ordering::SeqCst);
        if let Some((started, proceed)) = self.emit_barrier.lock().await.clone() {
            started.notify_one();
            proceed.notified().await;
        }
        sink.emit(DriverEventEnvelope {
            binding_id: command.binding_id.clone(),
            generation: command.generation,
            source_thread_id: command.source_thread_id.clone(),
            source_turn_id: None,
            source_item_id: None,
            event: RuntimeEvent::BindingEstablished {
                binding_id: command.binding_id.clone(),
            },
        })
        .await?;
        for mut event in self.extra_events.lock().await.clone() {
            event.binding_id = command.binding_id.clone();
            event.generation = command.generation;
            event.source_thread_id = command.source_thread_id.clone();
            sink.emit(event).await?;
        }
        Ok(DriverDispatchReceipt {
            request_id: command.request_id,
            duplicate: false,
            applied_tool_set: None,
        })
    }

    async fn inspect(
        &self,
        _query: DriverInspectionQuery,
    ) -> Result<DriverInspection, DriverError> {
        Ok(DriverInspection::Binding { active: true })
    }
}

struct TestFactory {
    key: AgentRuntimeFactoryKey,
    driver: Arc<TestDriver>,
    creates: AtomicUsize,
    credential_probe: Option<(
        AgentRuntimeCredentialSlot,
        AgentRuntimeCredentialRef,
        String,
    )>,
}

#[async_trait]
impl AgentRuntimeDriverFactory for TestFactory {
    fn factory_key(&self) -> &AgentRuntimeFactoryKey {
        &self.key
    }

    async fn create(
        &self,
        _instance: ActivatedAgentServiceInstance,
        host: RuntimeDriverHostPorts,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, DriverFactoryError> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        if let Some((slot, reference, purpose)) = &self.credential_probe {
            host.credentials
                .resolve(slot, reference, purpose)
                .await
                .map_err(|error| DriverFactoryError::CredentialUnavailable {
                    slot: slot.clone(),
                    reason: error.to_string(),
                })?;
        }
        Ok(self.driver.clone())
    }
}

struct RecordingSink(AtomicUsize);

#[async_trait]
impl DriverEventSink for RecordingSink {
    async fn emit(&self, _event: DriverEventEnvelope) -> Result<(), DriverError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct ManagedRuntimeSink {
    runtime: Arc<ManagedAgentRuntime<RuntimeStoreFixture>>,
    accepted: AtomicUsize,
}

#[async_trait]
impl DriverEventSink for ManagedRuntimeSink {
    async fn emit(&self, event: DriverEventEnvelope) -> Result<(), DriverError> {
        self.runtime
            .ingest_driver_event(event)
            .await
            .map_err(|error| DriverError::Unavailable {
                reason: error.to_string(),
                retryable: true,
            })?;
        self.accepted.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct Fixture {
    host: Arc<IntegrationDriverHost>,
    registry: AgentServiceDefinitionRegistry,
    repository: Arc<EphemeralAgentRuntimeHostRepository>,
    factory: Arc<TestFactory>,
    conformance: Arc<dyn DriverConformanceVerifier>,
    full_profile: RuntimeProfile,
}

async fn fixture() -> Fixture {
    fixture_with_broker(Arc::new(TestCredentialBroker)).await
}

async fn fixture_with_broker(credential_broker: Arc<dyn AgentRuntimeCredentialBroker>) -> Fixture {
    fixture_with_broker_and_probe(credential_broker, None).await
}

async fn fixture_with_broker_and_probe(
    credential_broker: Arc<dyn AgentRuntimeCredentialBroker>,
    credential_probe: Option<(
        AgentRuntimeCredentialSlot,
        AgentRuntimeCredentialRef,
        String,
    )>,
) -> Fixture {
    let full_profile = profile(vec![exact_before_tool()]);
    let service_instance_id: RuntimeServiceInstanceId = id("instance-1");
    let descriptor_profile_digest = profile_digest(&full_profile).expect("profile digest");
    let driver = Arc::new(TestDriver {
        descriptor: RuntimeDescriptor {
            protocol_revision: 1,
            service_instance_id,
            profile: full_profile.clone(),
            profile_digest: descriptor_profile_digest.clone(),
        },
        bind_count: AtomicUsize::new(0),
        dispatch_count: AtomicUsize::new(0),
        emit_barrier: Mutex::new(None),
        extra_events: Mutex::new(Vec::new()),
        mismatch_resume_source: AtomicBool::new(false),
    });
    let factory = Arc::new(TestFactory {
        key: AgentRuntimeFactoryKey::new("corp.factory").expect("factory key"),
        driver,
        creates: AtomicUsize::new(0),
        credential_probe,
    });
    let service_definition = definition("corp.factory", full_profile.clone());
    let conformance: Arc<dyn DriverConformanceVerifier> =
        Arc::new(TrustedDriverConformanceVerifier::new(
            TrustedDriverManifestRegistry::collect([TrustedDriverManifest {
                provenance: service_definition.provenance.clone(),
                suite_revision: "runtime-driver-v1".to_string(),
                driver_build_digest: "sha256:driver".to_string(),
                protocol_revision: 1,
                verified_profile_digest: descriptor_profile_digest,
            }])
            .expect("trusted driver manifests"),
        ));
    let registry = AgentServiceDefinitionRegistry::collect([AgentRuntimeDriverContribution {
        definition: service_definition,
        factory: factory.clone(),
        conversation_projection: DriverConversationProjectionProfile::full_fidelity(1),
    }])
    .expect("registry");
    let repository = Arc::new(EphemeralAgentRuntimeHostRepository::new());
    let host = Arc::new(IntegrationDriverHost::new(
        registry.clone(),
        repository.clone(),
        test_host_ports(credential_broker),
        conformance.clone(),
        "host-a",
    ));
    Fixture {
        host,
        registry,
        repository,
        factory,
        conformance,
        full_profile,
    }
}

fn put_instance(config: serde_json::Value) -> PutAgentServiceInstance {
    PutAgentServiceInstance {
        id: id("instance-1"),
        definition_id: AgentServiceDefinitionId::new("corp.agent").expect("definition"),
        config,
        credentials: BTreeMap::from([(
            AgentRuntimeCredentialSlot::new("endpoint_auth").expect("slot"),
            AgentRuntimeCredentialRef::new("credential-1").expect("ref"),
        )]),
        placement: AgentRuntimePlacement::InProcess,
        desired_state: ServiceInstanceDesiredState::Active,
        expected_revision: None,
    }
}

async fn activate(fixture: &Fixture) -> RuntimeOffer {
    let instance = fixture
        .host
        .put_instance(put_instance(json!({"endpoint": "local"})))
        .await
        .expect("instance");
    let descriptor_digest = profile_digest(&fixture.full_profile).expect("profile digest");
    fixture
        .host
        .activate(ActivateAgentServiceInstance {
            instance_id: instance.id,
            expected_revision: instance.revision,
            transport_profile: fixture.full_profile.clone(),
            transport_profile_digest: descriptor_digest.clone(),
            host_policy_profile: fixture.full_profile.clone(),
            host_policy_digest: descriptor_digest.clone(),
            conformance: ConformanceEvidence {
                suite_revision: "runtime-driver-v1".to_string(),
                driver_build_digest: "sha256:driver".to_string(),
                verified_profile_digest: descriptor_digest,
                verified_at: Utc::now(),
            },
        })
        .await
        .expect("activation")
}

#[tokio::test]
async fn definition_registry_rejects_duplicate_service_identity() {
    let fixture = fixture().await;
    let contribution = AgentRuntimeDriverContribution {
        definition: definition("corp.factory", fixture.full_profile.clone()),
        factory: fixture.factory.clone(),
        conversation_projection: DriverConversationProjectionProfile::full_fidelity(1),
    };
    let error = match AgentServiceDefinitionRegistry::collect([contribution.clone(), contribution])
    {
        Ok(_) => panic!("duplicate definition must fail fast"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        DefinitionRegistryError::DuplicateDefinition { .. }
    ));
}

#[tokio::test]
async fn definition_registry_rejects_incomplete_conversation_projection_profile() {
    let fixture = fixture().await;
    let mut projection = DriverConversationProjectionProfile::full_fidelity(1);
    projection
        .item_families
        .remove(&DriverConversationItemFamily::Mcp);
    let contribution = AgentRuntimeDriverContribution {
        definition: definition("corp.factory", fixture.full_profile.clone()),
        factory: fixture.factory.clone(),
        conversation_projection: projection,
    };
    let error = match AgentServiceDefinitionRegistry::collect([contribution]) {
        Ok(_) => panic!("incomplete conversation projection must fail fast"),
        Err(error) => error,
    };
    assert!(
        matches!(error, DefinitionRegistryError::InvalidDefinition { reason, .. } if reason.contains("missing required conversation family Mcp"))
    );
}

fn bound_surface(required: bool) -> BoundAgentSurfaceReference {
    BoundAgentSurfaceReference {
        revision: SurfaceRevision(3),
        digest: id("sha256:surface"),
        tool_set_revision: ToolSetRevision(1),
        tool_set_digest: "sha256:tools".to_string(),
        hook_plan_revision: Some(HookPlanRevision(2)),
        hook_plan_digest: Some(id("sha256:hook-plan")),
        hook_artifact_digest: Some("sha256:artifact".to_string()),
        hook_configuration_boundary: ConfigurationBoundary::HotReplace,
        required_hooks: vec![HookRequirement {
            point: HookPoint::BeforeTool,
            actions: BTreeSet::from([HookAction::Block]),
            minimum_strength: SemanticStrength::ExactSynchronous,
            failure_policy: HookFailurePolicy::FailClosed,
            required,
        }],
    }
}

#[tokio::test]
async fn invalid_configuration_is_rejected_before_factory_side_effect() {
    let fixture = fixture().await;
    let error = fixture
        .host
        .put_instance(put_instance(json!({})))
        .await
        .expect_err("invalid config");
    assert!(matches!(
        error,
        AgentRuntimeHostError::InvalidConfiguration { .. }
    ));
    assert_eq!(fixture.factory.creates.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn unavailable_credential_is_rejected_before_factory_side_effect() {
    let fixture = fixture_with_broker(Arc::new(RejectingCredentialBroker)).await;
    let instance = fixture
        .host
        .put_instance(put_instance(json!({"endpoint": "local"})))
        .await
        .expect("instance");
    let digest = profile_digest(&fixture.full_profile).expect("profile digest");
    let error = fixture
        .host
        .activate(ActivateAgentServiceInstance {
            instance_id: instance.id,
            expected_revision: instance.revision,
            transport_profile: fixture.full_profile.clone(),
            transport_profile_digest: digest.clone(),
            host_policy_profile: fixture.full_profile.clone(),
            host_policy_digest: digest.clone(),
            conformance: ConformanceEvidence {
                suite_revision: "runtime-driver-v1".to_string(),
                driver_build_digest: "sha256:driver".to_string(),
                verified_profile_digest: digest,
                verified_at: Utc::now(),
            },
        })
        .await
        .expect_err("credential must fail before factory creation");
    assert!(matches!(
        error,
        AgentRuntimeHostError::InvalidCredentialBinding { .. }
    ));
    assert_eq!(fixture.factory.creates.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn factory_credential_access_is_scoped_to_declared_slot_reference_and_purpose() {
    let broker = Arc::new(CountingCredentialBroker(AtomicUsize::new(0)));
    let fixture = fixture_with_broker_and_probe(
        broker.clone(),
        Some((
            AgentRuntimeCredentialSlot::new("endpoint_auth").expect("slot"),
            AgentRuntimeCredentialRef::new("credential-1").expect("ref"),
            "unapproved-purpose".to_string(),
        )),
    )
    .await;
    let instance = fixture
        .host
        .put_instance(put_instance(json!({"endpoint": "local"})))
        .await
        .expect("instance");
    let digest = profile_digest(&fixture.full_profile).expect("profile digest");
    let error = fixture
        .host
        .activate(ActivateAgentServiceInstance {
            instance_id: instance.id,
            expected_revision: instance.revision,
            transport_profile: fixture.full_profile.clone(),
            transport_profile_digest: digest.clone(),
            host_policy_profile: fixture.full_profile.clone(),
            host_policy_digest: digest.clone(),
            conformance: ConformanceEvidence {
                suite_revision: "runtime-driver-v1".to_string(),
                driver_build_digest: "sha256:driver".to_string(),
                verified_profile_digest: digest,
                verified_at: Utc::now(),
            },
        })
        .await
        .expect_err("factory cannot widen credential purpose");
    assert!(matches!(error, AgentRuntimeHostError::Factory { .. }));
    assert_eq!(broker.0.load(Ordering::SeqCst), 1);

    let lease = CredentialLease {
        slot: AgentRuntimeCredentialSlot::new("endpoint_auth").expect("slot"),
        purpose: "runtime_transport".to_string(),
        secret: "must-not-leak".to_string(),
    };
    assert!(!format!("{lease:?}").contains("must-not-leak"));
}

#[tokio::test]
async fn activation_persists_an_evidence_backed_effective_offer() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    assert!(offer.available);
    assert_eq!(offer.generation, RuntimeDriverGeneration(1));
    assert_eq!(offer.effective_profile.profile, fixture.full_profile);
    assert_eq!(fixture.factory.creates.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.host.offers().await.expect("offers"), vec![offer]);
}

#[tokio::test]
async fn offer_is_the_intersection_of_service_transport_and_host_policy() {
    let fixture = fixture().await;
    let instance = fixture
        .host
        .put_instance(put_instance(json!({"endpoint": "local"})))
        .await
        .expect("instance");
    let descriptor_digest = profile_digest(&fixture.full_profile).expect("descriptor digest");
    let mut transport = fixture.full_profile.clone();
    transport.reference_class = ReferenceRuntimeClass::Turn;
    transport.tools.channels.clear();
    transport.hooks.points.clear();
    let transport_digest = profile_digest(&transport).expect("transport digest");
    let host_policy_digest = profile_digest(&fixture.full_profile).expect("host policy digest");
    let offer = fixture
        .host
        .activate(ActivateAgentServiceInstance {
            instance_id: instance.id,
            expected_revision: instance.revision,
            transport_profile: transport,
            transport_profile_digest: transport_digest.clone(),
            host_policy_profile: fixture.full_profile.clone(),
            host_policy_digest: host_policy_digest.clone(),
            conformance: ConformanceEvidence {
                suite_revision: "runtime-driver-v1".to_string(),
                driver_build_digest: "sha256:driver".to_string(),
                verified_profile_digest: descriptor_digest,
                verified_at: Utc::now(),
            },
        })
        .await
        .expect("activation");
    assert_eq!(
        offer.effective_profile.profile.reference_class,
        ReferenceRuntimeClass::Turn
    );
    assert!(offer.effective_profile.profile.tools.channels.is_empty());
    assert!(offer.effective_profile.profile.hooks.points.is_empty());
    assert_eq!(
        offer.effective_profile.provenance.transport_digest,
        transport_digest
    );
    assert_eq!(
        offer.effective_profile.provenance.host_policy_digest,
        host_policy_digest
    );
}

#[tokio::test]
async fn required_hook_must_be_acknowledged_before_dispatch() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    let binding = fixture
        .host
        .bind(BindRuntimeRequest {
            binding_id: id("binding-1"),
            thread_id: id("thread-1"),
            offer_id: offer.id,
            bound_surface: bound_surface(true),
            intent: DriverBindIntent::Start,
        })
        .await
        .expect("bind");
    let lease = fixture
        .host
        .acquire_driver_lease(&binding.id)
        .await
        .expect("lease");
    let envelope = DriverCommandEnvelope {
        request_id: id("request-1"),
        binding_id: binding.id.clone(),
        generation: binding.driver_generation,
        source_thread_id: binding.source_thread_id.clone().expect("source"),
        runtime_turn_id: Some(id("turn-1")),
        command: RuntimeCommand::TurnStart {
            thread_id: binding.thread_id.clone(),
            input: vec![],
        },
    };
    let sink = Arc::new(RecordingSink(AtomicUsize::new(0)));
    let error = fixture
        .host
        .dispatch(
            RouteDriverCommand {
                envelope: envelope.clone(),
                lease_owner: lease.owner.clone(),
                lease_token: lease.token.clone(),
            },
            sink.clone(),
        )
        .await
        .expect_err("required hook is not acked");
    assert!(matches!(
        error,
        AgentRuntimeHostError::DispatchRejected { .. }
    ));

    let applied = AppliedSurface {
        revision: binding.bound_surface.revision,
        digest: binding.bound_surface.digest.clone(),
        tool_set_revision: binding.bound_surface.tool_set_revision,
        tool_set_digest: binding.bound_surface.tool_set_digest.clone(),
        hook_plan_revision: binding.bound_surface.hook_plan_revision,
        hook_plan_digest: binding.bound_surface.hook_plan_digest.clone(),
        hooks: vec![HookApplyStatus {
            point: HookPoint::BeforeTool,
            acknowledged: true,
            artifact_digest: Some("sha256:artifact".to_string()),
        }],
    };
    fixture
        .host
        .record_apply_receipt(&binding.id, binding.driver_generation, applied)
        .await
        .expect("apply ack");
    let mut stale_envelope = envelope.clone();
    stale_envelope.generation = RuntimeDriverGeneration(binding.driver_generation.0 + 1);
    let stale = fixture
        .host
        .dispatch(
            RouteDriverCommand {
                envelope: stale_envelope,
                lease_owner: lease.owner.clone(),
                lease_token: lease.token.clone(),
            },
            sink.clone(),
        )
        .await
        .expect_err("stale generation must be fenced before driver dispatch");
    assert!(matches!(
        stale,
        AgentRuntimeHostError::DispatchRejected { .. }
    ));
    assert_eq!(
        fixture.factory.driver.dispatch_count.load(Ordering::SeqCst),
        0
    );
    fixture
        .host
        .dispatch(
            RouteDriverCommand {
                envelope,
                lease_owner: lease.owner,
                lease_token: lease.token,
            },
            sink.clone(),
        )
        .await
        .expect("dispatch");
    assert_eq!(sink.0.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn event_sink_revalidates_lease_after_takeover() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    let binding = fixture
        .host
        .bind(BindRuntimeRequest {
            binding_id: id("binding-event-fence"),
            thread_id: id("thread-event-fence"),
            offer_id: offer.id,
            bound_surface: bound_surface(false),
            intent: DriverBindIntent::Start,
        })
        .await
        .expect("bind");
    let lease = fixture
        .host
        .acquire_driver_lease(&binding.id)
        .await
        .expect("lease");
    let started = Arc::new(Notify::new());
    let proceed = Arc::new(Notify::new());
    *fixture.factory.driver.emit_barrier.lock().await = Some((started.clone(), proceed.clone()));
    let host = fixture.host.clone();
    let sink = Arc::new(RecordingSink(AtomicUsize::new(0)));
    let dispatch_sink = sink.clone();
    let binding_id = binding.id.clone();
    let dispatch_binding_id = binding_id.clone();
    let generation = binding.driver_generation;
    let takeover_now = lease.expires_at + ChronoDuration::seconds(1);
    let lease_owner = lease.owner.clone();
    let lease_token = lease.token.clone();
    let dispatch = tokio::spawn(async move {
        host.dispatch(
            RouteDriverCommand {
                envelope: DriverCommandEnvelope {
                    request_id: id("request-event-fence"),
                    binding_id: dispatch_binding_id,
                    generation,
                    source_thread_id: binding.source_thread_id.expect("source"),
                    runtime_turn_id: Some(id("turn-event-fence")),
                    command: RuntimeCommand::TurnStart {
                        thread_id: binding.thread_id,
                        input: vec![],
                    },
                },
                lease_owner,
                lease_token,
            },
            dispatch_sink,
        )
        .await
    });
    started.notified().await;
    fixture
        .repository
        .acquire_lease(
            &binding_id,
            generation,
            "host-b",
            takeover_now,
            takeover_now + ChronoDuration::seconds(30),
        )
        .await
        .expect("take over expired lease");
    proceed.notify_one();
    let error = dispatch
        .await
        .expect("dispatch task")
        .expect_err("old event producer is fenced after takeover");
    assert!(matches!(
        error,
        AgentRuntimeHostError::Driver(DriverError::StaleGeneration)
    ));
    assert_eq!(sink.0.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn binding_loss_atomically_invalidates_lease_and_fences_late_event() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    let binding = fixture
        .host
        .bind(BindRuntimeRequest {
            binding_id: id("binding-loss-event-fence"),
            thread_id: id("thread-loss-event-fence"),
            offer_id: offer.id,
            bound_surface: bound_surface(false),
            intent: DriverBindIntent::Start,
        })
        .await
        .expect("bind");
    let lease = fixture
        .host
        .acquire_driver_lease(&binding.id)
        .await
        .expect("lease");
    let started = Arc::new(Notify::new());
    let proceed = Arc::new(Notify::new());
    *fixture.factory.driver.emit_barrier.lock().await = Some((started.clone(), proceed.clone()));
    let host = fixture.host.clone();
    let sink = Arc::new(RecordingSink(AtomicUsize::new(0)));
    let dispatch_sink = sink.clone();
    let binding_id = binding.id.clone();
    let generation = binding.driver_generation;
    let lease_owner = lease.owner.clone();
    let lease_token = lease.token.clone();
    let dispatch = tokio::spawn(async move {
        host.dispatch(
            RouteDriverCommand {
                envelope: DriverCommandEnvelope {
                    request_id: id("request-loss-event-fence"),
                    binding_id: binding.id,
                    generation,
                    source_thread_id: binding.source_thread_id.expect("source"),
                    runtime_turn_id: Some(id("turn-loss-event-fence")),
                    command: RuntimeCommand::TurnStart {
                        thread_id: binding.thread_id,
                        input: vec![],
                    },
                },
                lease_owner,
                lease_token,
            },
            dispatch_sink,
        )
        .await
    });
    started.notified().await;
    fixture
        .repository
        .mark_binding_lost(&binding_id, generation)
        .await
        .expect("mark lost");
    fixture
        .repository
        .validate_lease(
            &binding_id,
            generation,
            &lease.owner,
            &lease.token,
            Utc::now(),
        )
        .await
        .expect_err("lost binding lease must be invalid");
    proceed.notify_one();
    let error = dispatch
        .await
        .expect("dispatch task")
        .expect_err("late event fenced");
    assert!(matches!(
        error,
        AgentRuntimeHostError::Driver(DriverError::StaleGeneration)
    ));
    assert_eq!(sink.0.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn driver_lease_can_be_renewed_and_explicitly_released() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    let binding = fixture
        .host
        .bind(BindRuntimeRequest {
            binding_id: id("binding-lease-lifecycle"),
            thread_id: id("thread-lease-lifecycle"),
            offer_id: offer.id,
            bound_surface: bound_surface(false),
            intent: DriverBindIntent::Start,
        })
        .await
        .expect("bind");
    let lease = fixture
        .host
        .acquire_driver_lease(&binding.id)
        .await
        .expect("lease");
    let renewed = fixture
        .host
        .renew_driver_lease(&lease)
        .await
        .expect("renew lease");
    assert_eq!(renewed.binding_id, lease.binding_id);
    assert_eq!(renewed.generation, lease.generation);
    assert_eq!(renewed.owner, lease.owner);
    assert_eq!(renewed.token, lease.token);
    assert_eq!(renewed.epoch, lease.epoch);
    assert!(renewed.expires_at >= lease.expires_at);

    fixture
        .host
        .release_driver_lease(&renewed)
        .await
        .expect("release lease");
    let error = fixture
        .repository
        .validate_lease(
            &renewed.binding_id,
            renewed.generation,
            &renewed.owner,
            &renewed.token,
            Utc::now(),
        )
        .await
        .expect_err("released lease must be fenced");
    assert!(matches!(error, HostStoreError::NotFound { .. }));
}

#[tokio::test]
async fn thread_binding_is_sticky_and_source_generation_is_fenced() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    let bind_request = BindRuntimeRequest {
        binding_id: id("binding-1"),
        thread_id: id("thread-1"),
        offer_id: offer.id.clone(),
        bound_surface: bound_surface(false),
        intent: DriverBindIntent::Start,
    };
    let binding = fixture.host.bind(bind_request.clone()).await.expect("bind");
    assert_eq!(
        fixture
            .host
            .bind(bind_request)
            .await
            .expect("idempotent bind"),
        binding
    );
    assert_eq!(fixture.factory.driver.bind_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture
            .repository
            .find_binding_by_thread(&binding.thread_id)
            .await
            .expect("lookup"),
        Some(binding.clone())
    );
    let conflict = fixture
        .host
        .bind(BindRuntimeRequest {
            binding_id: id("binding-2"),
            thread_id: binding.thread_id,
            offer_id: offer.id,
            bound_surface: bound_surface(false),
            intent: DriverBindIntent::Start,
        })
        .await
        .expect_err("sticky binding conflict");
    assert!(matches!(
        conflict,
        AgentRuntimeHostError::Store(HostStoreError::Conflict { .. })
    ));
    assert!(
        fixture
            .repository
            .find_source(&binding.id, RuntimeDriverGeneration(999))
            .await
            .expect("stale source")
            .is_none()
    );

    let coordinate = RuntimeDriverCoordinate::Turn {
        runtime_turn_id: id("turn-1"),
        source_turn_id: id("source-turn-1"),
    };
    fixture
        .repository
        .record_driver_coordinate(&binding.id, binding.driver_generation, coordinate.clone())
        .await
        .expect("record source turn");
    fixture
        .repository
        .record_driver_coordinate(&binding.id, binding.driver_generation, coordinate)
        .await
        .expect("idempotent source turn");
    let collision = fixture
        .repository
        .record_driver_coordinate(
            &binding.id,
            binding.driver_generation,
            RuntimeDriverCoordinate::Turn {
                runtime_turn_id: id("turn-2"),
                source_turn_id: id("source-turn-1"),
            },
        )
        .await
        .expect_err("source turn coordinate is bijective");
    assert!(matches!(collision, HostStoreError::Conflict { .. }));

    let first_lease = fixture
        .host
        .acquire_driver_lease(&binding.id)
        .await
        .expect("first lease");
    let replayed_lease = fixture
        .host
        .acquire_driver_lease(&binding.id)
        .await
        .expect("same owner lease replay");
    assert_eq!(replayed_lease, first_lease);
}

#[tokio::test]
async fn resume_rejects_mismatched_source_before_binding_activation() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    fixture
        .factory
        .driver
        .mismatch_resume_source
        .store(true, Ordering::SeqCst);
    let binding_id: RuntimeBindingId = id("binding-resume-source-mismatch");
    let expected_source: DriverThreadId = id("canonical-old-source-thread");

    let error = fixture
        .host
        .bind(BindRuntimeRequest {
            binding_id: binding_id.clone(),
            thread_id: id("thread-resume-source-mismatch"),
            offer_id: offer.id,
            bound_surface: bound_surface(false),
            intent: DriverBindIntent::Resume {
                source_thread_id: expected_source,
            },
        })
        .await
        .expect_err("Resume source mismatch must be rejected");
    assert!(matches!(
        error,
        AgentRuntimeHostError::DispatchRejected { .. }
    ));
    let binding = fixture
        .repository
        .load_binding(&binding_id)
        .await
        .expect("load failed binding")
        .expect("failed binding remains auditable");
    assert_eq!(binding.state, RuntimeBindingState::Failed);
    assert!(binding.source_thread_id.is_none());
    assert!(
        fixture
            .repository
            .find_source(&binding_id, binding.driver_generation)
            .await
            .expect("source lookup")
            .is_none()
    );
}

#[tokio::test]
async fn deactivation_withdraws_offers_without_rebinding_existing_thread() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    let instance = fixture
        .repository
        .load_instance(&offer.service_instance_id)
        .await
        .expect("instance query")
        .expect("instance");
    let deactivated = fixture
        .host
        .deactivate(&instance.id, instance.revision)
        .await
        .expect("deactivate");
    assert_eq!(
        deactivated.desired_state,
        ServiceInstanceDesiredState::Inactive
    );
    assert!(fixture.host.offers().await.expect("offers").is_empty());
}

#[tokio::test]
async fn durable_offer_recovers_the_same_driver_generation_after_host_restart() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    assert_eq!(fixture.factory.creates.load(Ordering::SeqCst), 1);
    let restarted = IntegrationDriverHost::new(
        fixture.registry.clone(),
        fixture.repository.clone(),
        test_host_ports(Arc::new(TestCredentialBroker)),
        fixture.conformance.clone(),
        "host-b",
    );
    assert_eq!(
        restarted
            .recover_available_drivers()
            .await
            .expect("recover"),
        1
    );
    assert_eq!(fixture.factory.creates.load(Ordering::SeqCst), 2);
    assert_eq!(restarted.offers().await.expect("offers"), vec![offer]);
}

#[tokio::test]
async fn host_restart_recovers_orphaned_pending_binding_from_durable_intent() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    let pending = RuntimeBinding {
        id: id("binding-orphaned"),
        thread_id: id("thread-orphaned"),
        offer_id: offer.id,
        service_instance_id: offer.service_instance_id,
        instance_revision: offer.instance_revision,
        driver_generation: offer.generation,
        profile_digest: offer.profile_digest,
        bound_surface: bound_surface(false),
        bind_intent: DriverBindIntent::Start,
        applied_surface: None,
        driver_binding_id: None,
        source_thread_id: None,
        state: RuntimeBindingState::Pending,
        lease_epoch: 0,
    };
    fixture
        .repository
        .reserve_binding(pending.clone())
        .await
        .expect("reserve durable pending binding");
    let current = fixture
        .repository
        .load_instance(&pending.service_instance_id)
        .await
        .expect("instance query")
        .expect("instance");
    let mut update = put_instance(json!({"endpoint": "changed-after-reserve"}));
    update.expected_revision = Some(current.revision);
    fixture
        .host
        .put_instance(update)
        .await
        .expect("update instance after binding reservation");
    let restarted = IntegrationDriverHost::new(
        fixture.registry.clone(),
        fixture.repository.clone(),
        test_host_ports(Arc::new(TestCredentialBroker)),
        fixture.conformance.clone(),
        "host-b",
    );
    assert_eq!(
        restarted
            .recover_pending_bindings()
            .await
            .expect("recover pending binding"),
        1
    );
    let recovered = restarted.binding(&pending.id).await.expect("binding");
    assert_eq!(recovered.state, RuntimeBindingState::Active);
    assert!(recovered.driver_binding_id.is_some());
    assert!(recovered.source_thread_id.is_some());
}

#[tokio::test]
async fn old_binding_recovers_from_activation_snapshot_after_instance_config_update() {
    let fixture = fixture().await;
    let offer = activate(&fixture).await;
    let binding = fixture
        .host
        .bind(BindRuntimeRequest {
            binding_id: id("binding-snapshot"),
            thread_id: id("thread-snapshot"),
            offer_id: offer.id,
            bound_surface: bound_surface(false),
            intent: DriverBindIntent::Start,
        })
        .await
        .expect("bind");
    let current = fixture
        .repository
        .load_instance(&binding.service_instance_id)
        .await
        .expect("instance query")
        .expect("instance");
    let mut update = put_instance(json!({"endpoint": "changed"}));
    update.expected_revision = Some(current.revision);
    fixture
        .host
        .put_instance(update)
        .await
        .expect("update instance");
    assert!(fixture.host.offers().await.expect("offers").is_empty());
    assert!(matches!(
        fixture
            .host
            .bind(BindRuntimeRequest {
                binding_id: id("binding-stale-offer"),
                thread_id: id("thread-stale-offer"),
                offer_id: binding.offer_id.clone(),
                bound_surface: bound_surface(false),
                intent: DriverBindIntent::Start,
            })
            .await,
        Err(AgentRuntimeHostError::OfferUnavailable { .. })
    ));

    let restarted = IntegrationDriverHost::new(
        fixture.registry.clone(),
        fixture.repository.clone(),
        test_host_ports(Arc::new(TestCredentialBroker)),
        fixture.conformance.clone(),
        "host-b",
    );
    restarted
        .driver_endpoint(&binding.service_instance_id, binding.driver_generation)
        .await
        .expect("old generation endpoint remains resolvable for its sticky binding");
    let lease = restarted
        .acquire_driver_lease(&binding.id)
        .await
        .expect("lease");
    restarted
        .dispatch(
            RouteDriverCommand {
                envelope: DriverCommandEnvelope {
                    request_id: id("request-snapshot"),
                    binding_id: binding.id,
                    generation: binding.driver_generation,
                    source_thread_id: binding.source_thread_id.expect("source"),
                    runtime_turn_id: Some(id("turn-snapshot")),
                    command: RuntimeCommand::TurnStart {
                        thread_id: binding.thread_id,
                        input: vec![],
                    },
                },
                lease_owner: lease.owner,
                lease_token: lease.token,
            },
            Arc::new(RecordingSink(AtomicUsize::new(0))),
        )
        .await
        .expect("old generation dispatch after restart");
    assert_eq!(fixture.factory.creates.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn reactivation_withdraws_old_offer_but_keeps_generation_monotonic() {
    let fixture = fixture().await;
    let old_offer = activate(&fixture).await;
    let instance = fixture
        .repository
        .load_instance(&old_offer.service_instance_id)
        .await
        .expect("instance query")
        .expect("instance");
    let digest = profile_digest(&fixture.full_profile).expect("profile digest");
    let new_offer = fixture
        .host
        .activate(ActivateAgentServiceInstance {
            instance_id: instance.id,
            expected_revision: instance.revision,
            transport_profile: fixture.full_profile.clone(),
            transport_profile_digest: digest.clone(),
            host_policy_profile: fixture.full_profile.clone(),
            host_policy_digest: digest.clone(),
            conformance: ConformanceEvidence {
                suite_revision: "runtime-driver-v1".to_string(),
                driver_build_digest: "sha256:driver".to_string(),
                verified_profile_digest: digest,
                verified_at: Utc::now(),
            },
        })
        .await
        .expect("reactivation");
    assert_eq!(new_offer.generation, RuntimeDriverGeneration(2));
    assert_eq!(
        fixture.host.offers().await.expect("offers"),
        vec![new_offer]
    );
    assert!(
        !fixture
            .repository
            .load_offer(&old_offer.id)
            .await
            .expect("old offer")
            .expect("old offer row")
            .available
    );
}
