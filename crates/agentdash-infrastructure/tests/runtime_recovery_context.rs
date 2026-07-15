use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use agentdash_agent_runtime::{
    AgentSurfaceCompiler, BusinessAgentSurfaceFacts, CompiledBusinessAgentSurface,
    ContextProjectionIdentity, ContributionRequirement, SurfaceSourceRef, WorkspaceRequirement,
};
use agentdash_agent_runtime_contract::*;
use agentdash_agent_runtime_host::*;
use agentdash_application_ports::agent_run_runtime::{
    AgentRunContextDeliveryTarget, AgentRunRuntimeBinding, AgentRunRuntimeBindingError,
    AgentRunRuntimeBindingRepository, AgentRunRuntimeProvisionRequest, AgentRunRuntimeProvisioner,
    AgentRunRuntimeRecoveryIntent, AgentRunRuntimeRecoveryState, AgentRunRuntimeTarget,
};
use agentdash_infrastructure::agent_runtime_composition::{
    AgentRunRuntimeSurfaceSource, AgentRunRuntimeSurfaceSourceError, AgentRunRuntimeSurfaceStore,
    HostAgentRunRuntimeProvisioner, PreparedAgentRunRuntime,
};
use agentdash_integration_api::*;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use tokio::sync::Mutex;
use uuid::Uuid;

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid recovery fixture id")
}

fn runtime_profile() -> RuntimeProfile {
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
            capabilities: BTreeSet::new(),
            mechanism: DeliveryMechanism::Native,
        },
        interactions: InteractionProfile {
            kinds: BTreeSet::new(),
            durable_correlation: true,
        },
        lifecycle: BTreeSet::from([
            LifecycleCapability::ThreadStart,
            LifecycleCapability::ThreadResume,
            LifecycleCapability::TurnStart,
        ]),
        hooks: HookProfile {
            points: Vec::new(),
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

fn service_definition(profile: RuntimeProfile) -> AgentServiceDefinition {
    let config_schema = json!({
        "type": "object",
        "additionalProperties": false
    });
    AgentServiceDefinition {
        provenance: AgentServiceProvenance {
            definition_id: AgentServiceDefinitionId::new("fixture.recovery-agent")
                .expect("definition id"),
            publisher_integration: "fixture.integration".to_string(),
            service_version: "1.0.0".to_string(),
            build_digest: AgentServiceBuildDigest::new("sha256:fixture-recovery-build")
                .expect("build digest"),
        },
        factory_key: AgentRuntimeFactoryKey::new("fixture.recovery-factory").expect("factory key"),
        supported_protocol_revisions: vec![1],
        config_schema_digest: AgentServiceSchemaDigest::new(schema_digest(&config_schema))
            .expect("schema digest"),
        config_schema,
        credential_slots: Vec::new(),
        service_profile_upper_bound: profile,
    }
}

fn materialized_surface(thread_id: RuntimeThreadId) -> MaterializedDriverSurface {
    MaterializedDriverSurface {
        runtime_thread_id: thread_id,
        revision: SurfaceRevision(1),
        digest: id("sha256:recovery-surface"),
        authorization_identity: None,
        context: DriverContextSurface {
            recipe: ContextRecipe {
                revision: ContextRecipeRevision(1),
                provenance: ContextProvenance {
                    settings_revision: ThreadSettingsRevision(1),
                    tool_set_revision: ToolSetRevision(1),
                },
                source_item_ids: Vec::new(),
            },
            instructions: Vec::new(),
            blocks: vec![ContextBlock::Instruction {
                text: "old context".to_string(),
            }],
            digest: id("sha256:old-context"),
            fidelity: ContextFidelity::PlatformExact,
        },
        tools: DriverToolSurface {
            revision: ToolSetRevision(1),
            digest: "sha256:recovery-tools".to_string(),
            tools: Vec::new(),
        },
        hooks: DriverHookSurface {
            revision: HookPlanRevision(1),
            digest: id("sha256:recovery-hooks"),
            artifact_digest: None,
            configuration_boundary: ConfigurationBoundary::HotReplace,
            bindings: Vec::new(),
        },
        workspace: DriverWorkspaceSurface {
            digest: "sha256:recovery-workspace".to_string(),
            capabilities: Vec::new(),
            roots: Vec::new(),
        },
    }
}

fn bound_surface(surface: &MaterializedDriverSurface) -> BoundAgentSurfaceReference {
    BoundAgentSurfaceReference {
        revision: surface.revision,
        digest: surface.digest.clone(),
        tool_set_revision: surface.tools.revision,
        tool_set_digest: surface.tools.digest.clone(),
        hook_plan_revision: Some(surface.hooks.revision),
        hook_plan_digest: Some(surface.hooks.digest.clone()),
        hook_artifact_digest: None,
        hook_configuration_boundary: surface.hooks.configuration_boundary,
        required_hooks: Vec::new(),
    }
}

fn hook_plan(surface: &MaterializedDriverSurface) -> BoundRuntimeHookPlan {
    BoundRuntimeHookPlan {
        revision: surface.hooks.revision,
        digest: surface.hooks.digest.clone(),
        entries: Vec::new(),
    }
}

fn surface_descriptor(surface: &MaterializedDriverSurface) -> RuntimeSurfaceDescriptor {
    RuntimeSurfaceDescriptor {
        source_frame_id: "fixture-frame".to_string(),
        surface_revision: surface.revision,
        surface_digest: surface.digest.clone(),
        vfs_digest: surface.workspace.digest.clone(),
        context_recipe_revision: surface.context.recipe.revision,
        context_digest: surface.context.digest.clone(),
        settings_revision: surface.context.recipe.provenance.settings_revision,
        tool_set_revision: surface.tools.revision,
        tool_set_digest: surface.tools.digest.clone(),
        hook_plan: hook_plan(surface),
        terminal_hook_effect_binding: None,
    }
}

fn business_surface(surface: &MaterializedDriverSurface) -> CompiledBusinessAgentSurface {
    AgentSurfaceCompiler
        .compile_business_facts(BusinessAgentSurfaceFacts {
            revision: surface.revision,
            context_recipe: surface.context.recipe.clone(),
            tool_set_revision: surface.tools.revision,
            hook_plan_revision: surface.hooks.revision,
            workspace: WorkspaceRequirement {
                capabilities: BTreeSet::new(),
                minimum_mechanism: DeliveryMechanism::Native,
                requirement: ContributionRequirement::Required,
            },
            source: SurfaceSourceRef {
                layer: "fixture".to_string(),
                key: "recovery".to_string(),
            },
            transition_phase_node: Some("recovery".to_string()),
            instructions: Vec::new(),
            tools: Vec::new(),
            hooks: Vec::new(),
            bootstrap_context: Default::default(),
            normalized_context_surface: Default::default(),
            projection_identity: ContextProjectionIdentity {
                operation_id: "fixture-recovery-surface".to_string(),
                source_frame_id: "fixture-frame".to_string(),
                source_frame_revision: surface.revision.0,
                recorded_at_ms: 1,
            },
        })
        .expect("compile recovery business surface")
}

struct FixtureCredentialBroker;

#[async_trait]
impl AgentRuntimeCredentialBroker for FixtureCredentialBroker {
    async fn resolve(
        &self,
        slot: &AgentRuntimeCredentialSlot,
        _reference: &AgentRuntimeCredentialRef,
        _purpose: &str,
    ) -> Result<CredentialLease, CredentialResolveError> {
        Err(CredentialResolveError::Unavailable {
            slot: slot.clone(),
            reason: "recovery fixture has no credentials".to_string(),
        })
    }
}

struct FixtureSurfaceBroker;

#[async_trait]
impl AgentRuntimeSurfaceBroker for FixtureSurfaceBroker {
    async fn materialize(
        &self,
        _request: DriverSurfaceRequest,
    ) -> Result<MaterializedDriverSurface, DriverSurfaceError> {
        Err(DriverSurfaceError::Unavailable {
            reason: "fixture driver does not request surfaces".to_string(),
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
            reason: "fixture driver does not request tool sets".to_string(),
            retryable: false,
        })
    }
}

struct FixtureContextBroker;

#[async_trait]
impl AgentRuntimeContextBroker for FixtureContextBroker {
    async fn load_transcript(
        &self,
        _request: DriverTranscriptRequest,
    ) -> Result<DriverTranscript, DriverContextError> {
        Err(DriverContextError::NotFound)
    }

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

struct FixtureToolCallback;

#[async_trait]
impl AgentRuntimeToolCallback for FixtureToolCallback {
    async fn invoke(
        &self,
        _request: DriverToolInvocation,
    ) -> Result<DriverToolOutcome, DriverToolCallbackError> {
        Err(DriverToolCallbackError::ProtocolViolation {
            reason: "fixture driver does not invoke tools".to_string(),
        })
    }
}

struct FixtureHookCallback;

#[async_trait]
impl AgentRuntimeHookCallback for FixtureHookCallback {
    async fn execute(
        &self,
        _request: DriverHookInvocation,
    ) -> Result<DriverHookDecision, DriverHookCallbackError> {
        Err(DriverHookCallbackError::ProtocolViolation {
            reason: "fixture driver does not invoke hooks".to_string(),
        })
    }
}

struct RecordingDriver {
    descriptor: RuntimeDescriptor,
    bind_requests: Mutex<Vec<DriverBindRequest>>,
}

#[async_trait]
impl AgentRuntimeDriver for RecordingDriver {
    async fn describe(
        &self,
        _request: DriverDescribeRequest,
    ) -> Result<RuntimeDescriptor, DriverError> {
        Ok(self.descriptor.clone())
    }

    async fn bind(&self, request: DriverBindRequest) -> Result<DriverBinding, DriverError> {
        self.bind_requests.lock().await.push(request.clone());
        let source_thread_id = match &request.intent {
            DriverBindIntent::Resume { source_thread_id }
            | DriverBindIntent::Fork {
                source_thread_id, ..
            } => source_thread_id.clone(),
            DriverBindIntent::Start => id("fixture-source-thread"),
        };
        Ok(DriverBinding {
            driver_binding_id: id(&format!("driver-{}", request.binding_id)),
            source_thread_id,
            applied_surface_revision: request.surface_revision,
            applied_surface_digest: request.surface_digest,
            applied_tool_set_revision: ToolSetRevision(1),
            applied_tool_set_digest: "sha256:recovery-tools".to_string(),
            applied_hook_plan_revision: Some(HookPlanRevision(1)),
            applied_hook_plan_digest: Some(id("sha256:recovery-hooks")),
            applied_hooks: Vec::new(),
        })
    }

    async fn dispatch(
        &self,
        _command: DriverCommandEnvelope,
        _sink: Arc<dyn DriverEventSink>,
    ) -> Result<DriverDispatchReceipt, DriverError> {
        Err(DriverError::Unsupported {
            reason: "recovery fixture does not dispatch commands".to_string(),
        })
    }

    async fn inspect(
        &self,
        _query: DriverInspectionQuery,
    ) -> Result<DriverInspection, DriverError> {
        Ok(DriverInspection::Binding { active: true })
    }
}

struct FixtureDriverFactory {
    key: AgentRuntimeFactoryKey,
    driver: Arc<RecordingDriver>,
}

#[async_trait]
impl AgentRuntimeDriverFactory for FixtureDriverFactory {
    fn factory_key(&self) -> &AgentRuntimeFactoryKey {
        &self.key
    }

    async fn create(
        &self,
        _instance: ActivatedAgentServiceInstance,
        _host: RuntimeDriverHostPorts,
    ) -> Result<Arc<dyn AgentRuntimeDriver>, DriverFactoryError> {
        Ok(self.driver.clone())
    }
}

struct BindingRepositoryState {
    current: AgentRunRuntimeBinding,
    intent: Option<AgentRunRuntimeRecoveryIntent>,
}

struct FixtureBindingRepository {
    state: Mutex<BindingRepositoryState>,
}

#[async_trait]
impl AgentRunRuntimeBindingRepository for FixtureBindingRepository {
    async fn load(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        let state = self.state.lock().await;
        Ok((state.current.target == *target).then(|| state.current.clone()))
    }

    async fn load_by_thread_id(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        let state = self.state.lock().await;
        Ok((state.current.thread_id == *thread_id).then(|| state.current.clone()))
    }

    async fn list_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        let state = self.state.lock().await;
        Ok((state.current.target.run_id == run_id)
            .then(|| state.current.clone())
            .into_iter()
            .collect())
    }

    async fn list_by_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        let state = self.state.lock().await;
        Ok((state.current.target.agent_id == agent_id)
            .then(|| state.current.clone())
            .into_iter()
            .collect())
    }

    async fn insert(
        &self,
        binding: AgentRunRuntimeBinding,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        self.state.lock().await.current = binding.clone();
        Ok(binding)
    }

    async fn append_lineage(
        &self,
        expected: &AgentRunRuntimeBinding,
        binding: AgentRunRuntimeBinding,
        recovery_intent_id: &str,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        let mut state = self.state.lock().await;
        if state.current != *expected
            || state.intent.as_ref().map(|intent| intent.id.as_str()) != Some(recovery_intent_id)
        {
            return Err(AgentRunRuntimeBindingError::Conflict);
        }
        state.current = binding.clone();
        Ok(binding)
    }

    async fn prepare_recovery(
        &self,
        intent: AgentRunRuntimeRecoveryIntent,
    ) -> Result<AgentRunRuntimeRecoveryIntent, AgentRunRuntimeBindingError> {
        let mut state = self.state.lock().await;
        if let Some(existing) = &state.intent {
            return Ok(existing.clone());
        }
        state.intent = Some(intent.clone());
        Ok(intent)
    }

    async fn advance_recovery(
        &self,
        intent_id: &str,
        expected: AgentRunRuntimeRecoveryState,
        next: AgentRunRuntimeRecoveryState,
        failure_reason: Option<String>,
    ) -> Result<AgentRunRuntimeRecoveryIntent, AgentRunRuntimeBindingError> {
        let mut state = self.state.lock().await;
        let intent = state
            .intent
            .as_mut()
            .ok_or(AgentRunRuntimeBindingError::NotFound)?;
        if intent.id != intent_id || intent.state != expected {
            return Err(AgentRunRuntimeBindingError::Conflict);
        }
        intent.state = next;
        intent.failure_reason = failure_reason;
        Ok(intent.clone())
    }
}

struct FixtureSurfaceStore {
    surfaces: Mutex<BTreeMap<RuntimeBindingId, MaterializedDriverSurface>>,
    business: CompiledBusinessAgentSurface,
}

#[async_trait]
impl AgentRunRuntimeSurfaceStore for FixtureSurfaceStore {
    async fn put_surface(
        &self,
        binding_id: &RuntimeBindingId,
        surface: &MaterializedDriverSurface,
        _business_surface: &CompiledBusinessAgentSurface,
    ) -> Result<(), AgentRunRuntimeBindingError> {
        self.surfaces
            .lock()
            .await
            .insert(binding_id.clone(), surface.clone());
        Ok(())
    }

    async fn load_surface(
        &self,
        binding_id: &RuntimeBindingId,
    ) -> Result<Option<MaterializedDriverSurface>, AgentRunRuntimeBindingError> {
        Ok(self.surfaces.lock().await.get(binding_id).cloned())
    }

    async fn load_business_surface(
        &self,
        binding_id: &RuntimeBindingId,
        surface_revision: SurfaceRevision,
        surface_digest: &SurfaceDigest,
    ) -> Result<CompiledBusinessAgentSurface, AgentRunRuntimeBindingError> {
        let surfaces = self.surfaces.lock().await;
        let surface = surfaces
            .get(binding_id)
            .ok_or(AgentRunRuntimeBindingError::NotFound)?;
        if surface.revision != surface_revision || surface.digest != *surface_digest {
            return Err(AgentRunRuntimeBindingError::Conflict);
        }
        Ok(self.business.clone())
    }
}

struct UnusedSurfaceSource;

#[async_trait]
impl AgentRunRuntimeSurfaceSource for UnusedSurfaceSource {
    async fn prepare(
        &self,
        _request: &AgentRunRuntimeProvisionRequest,
        _thread_id: &RuntimeThreadId,
        _binding_id: &RuntimeBindingId,
    ) -> Result<PreparedAgentRunRuntime, AgentRunRuntimeSurfaceSourceError> {
        Err(AgentRunRuntimeSurfaceSourceError::Invalid {
            reason: "recovery must not prepare a new business surface".to_string(),
        })
    }
}

struct FixtureGateway {
    context: RuntimeContextView,
    commands: Mutex<Vec<RuntimeCommandEnvelope>>,
}

#[async_trait]
impl AgentRuntimeGateway for FixtureGateway {
    async fn append_presentation(
        &self,
        _request: RuntimePresentationAppendRequest,
    ) -> Result<RuntimePresentationAppendReceipt, RuntimePresentationAppendError> {
        Err(RuntimePresentationAppendError::Unavailable)
    }

    async fn execute(
        &self,
        command: RuntimeCommandEnvelope,
    ) -> Result<OperationReceipt, RuntimeExecuteError> {
        self.commands.lock().await.push(command.clone());
        Ok(OperationReceipt {
            operation_id: command.meta.operation_id,
            operation_sequence: OperationSequence(1),
            thread_id: Some(self.context.thread_id.clone()),
            accepted_revision: RuntimeRevision(8),
            duplicate: false,
        })
    }

    async fn snapshot(
        &self,
        query: RuntimeSnapshotQuery,
    ) -> Result<RuntimeSnapshotResult, RuntimeSnapshotError> {
        match query {
            RuntimeSnapshotQuery::Context {
                thread_id,
                at_context_revision: None,
            } if thread_id == self.context.thread_id => Ok(RuntimeSnapshotResult::Context {
                context: Box::new(self.context.clone()),
            }),
            _ => Err(RuntimeSnapshotError::NotFound),
        }
    }

    async fn events(
        &self,
        _subscription: RuntimeEventSubscription,
    ) -> Result<Box<dyn RuntimeEventStream>, RuntimeSubscribeError> {
        Err(RuntimeSubscribeError::Unavailable {
            reason: "recovery fixture has no event stream".to_string(),
            retryable: false,
        })
    }
}

#[tokio::test]
async fn recover_materializes_active_context_checkpoint_into_the_new_binding() {
    let thread_id: RuntimeThreadId = id("recovery-thread");
    let old_binding_id: RuntimeBindingId = id("recovery-binding");
    let profile = runtime_profile();
    let profile_digest = profile_digest(&profile).expect("profile digest");
    let definition = service_definition(profile.clone());
    let instance_id: RuntimeServiceInstanceId = id("recovery-instance");
    let driver = Arc::new(RecordingDriver {
        descriptor: RuntimeDescriptor {
            protocol_revision: 1,
            service_instance_id: instance_id.clone(),
            profile: profile.clone(),
            profile_digest: profile_digest.clone(),
        },
        bind_requests: Mutex::new(Vec::new()),
    });
    let factory = Arc::new(FixtureDriverFactory {
        key: definition.factory_key.clone(),
        driver: driver.clone(),
    });
    let registry = AgentServiceDefinitionRegistry::collect([AgentRuntimeDriverContribution {
        definition: definition.clone(),
        factory,
        conversation_projection: DriverConversationProjectionProfile::full_fidelity(1),
    }])
    .expect("fixture definition registry");
    let host_repository = Arc::new(EphemeralAgentRuntimeHostRepository::new());
    let conformance = Arc::new(TrustedDriverConformanceVerifier::new(
        TrustedDriverManifestRegistry::collect([TrustedDriverManifest {
            provenance: definition.provenance.clone(),
            suite_revision: "fixture-recovery-v1".to_string(),
            driver_build_digest: "sha256:fixture-driver".to_string(),
            protocol_revision: 1,
            verified_profile_digest: profile_digest.clone(),
        }])
        .expect("fixture trust manifest"),
    ));
    let host = Arc::new(IntegrationDriverHost::new(
        registry,
        host_repository.clone(),
        RuntimeDriverHostPorts {
            credentials: Arc::new(FixtureCredentialBroker),
            surfaces: Arc::new(FixtureSurfaceBroker),
            context: Arc::new(FixtureContextBroker),
            tools: Arc::new(FixtureToolCallback),
            hooks: Arc::new(FixtureHookCallback),
        },
        conformance,
        "recovery-host",
    ));
    let instance = host
        .put_instance(PutAgentServiceInstance {
            id: instance_id,
            definition_id: definition.provenance.definition_id.clone(),
            config: json!({}),
            credentials: BTreeMap::new(),
            placement: AgentRuntimePlacement::InProcess,
            desired_state: ServiceInstanceDesiredState::Active,
            expected_revision: None,
        })
        .await
        .expect("put fixture service instance");
    let evidence = ConformanceEvidence {
        suite_revision: "fixture-recovery-v1".to_string(),
        driver_build_digest: "sha256:fixture-driver".to_string(),
        verified_profile_digest: profile_digest.clone(),
        verified_at: Utc::now(),
    };
    let offer = host
        .activate(ActivateAgentServiceInstance {
            instance_id: instance.id,
            expected_revision: instance.revision,
            transport_profile: profile.clone(),
            transport_profile_digest: profile_digest.clone(),
            host_policy_profile: profile.clone(),
            host_policy_digest: profile_digest.clone(),
            conformance: evidence,
        })
        .await
        .expect("activate fixture service instance");
    let old_surface = materialized_surface(thread_id.clone());
    let old_host_binding = host
        .bind(BindRuntimeRequest {
            binding_id: old_binding_id.clone(),
            thread_id: thread_id.clone(),
            offer_id: offer.id.clone(),
            bound_surface: bound_surface(&old_surface),
            intent: DriverBindIntent::Start,
        })
        .await
        .expect("bind original runtime");
    let target = AgentRunRuntimeTarget {
        run_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
    };
    let old = AgentRunRuntimeBinding {
        target: target.clone(),
        presentation_thread_id: id("presentation-recovery-thread"),
        thread_id: thread_id.clone(),
        binding_id: old_binding_id.clone(),
        binding_epoch: BindingEpoch(1),
        driver_generation: old_host_binding.driver_generation,
        source_thread_id: old_host_binding
            .source_thread_id
            .expect("original source thread"),
        profile_digest: offer.profile_digest.clone(),
        profile_provenance: offer.effective_profile.provenance.clone(),
        bound_profile: offer.effective_profile.profile.clone(),
        surface: surface_descriptor(&old_surface),
        settings_revision: ThreadSettingsRevision(1),
        context_delivery_target: AgentRunContextDeliveryTarget {
            connector_id: "fixture".to_string(),
            executor: "FIXTURE".to_string(),
        },
    };
    let binding_repository = Arc::new(FixtureBindingRepository {
        state: Mutex::new(BindingRepositoryState {
            current: old.clone(),
            intent: None,
        }),
    });
    let surface_store = Arc::new(FixtureSurfaceStore {
        surfaces: Mutex::new(BTreeMap::from([(old_binding_id, old_surface.clone())])),
        business: business_surface(&old_surface),
    });
    let materialized = MaterializedContext {
        recipe: ContextRecipe {
            revision: ContextRecipeRevision(2),
            provenance: ContextProvenance {
                settings_revision: ThreadSettingsRevision(2),
                tool_set_revision: ToolSetRevision(1),
            },
            source_item_ids: vec![id("checkpoint-source-item")],
        },
        blocks: vec![ContextBlock::CompactionSummary {
            summary: "active checkpoint summary".to_string(),
        }],
        digest: id("sha256:active-checkpoint-context"),
        fidelity: ContextFidelity::PlatformExact,
    };
    let checkpoint_id: ContextCheckpointId = id("active-recovery-checkpoint");
    let context_revision = ContextRevision(2);
    let gateway = Arc::new(FixtureGateway {
        context: RuntimeContextView {
            thread_id: thread_id.clone(),
            head: Some(ActiveContextHeadView {
                checkpoint_id: checkpoint_id.clone(),
                revision: context_revision,
                digest: materialized.digest.clone(),
                provenance: materialized.recipe.provenance.clone(),
                fidelity: materialized.fidelity,
            }),
            checkpoint: Some(ContextCheckpointView {
                checkpoint_id,
                thread_id,
                revision: context_revision,
                materialized: materialized.clone(),
            }),
            blocks: materialized.blocks.clone(),
            fidelity: materialized.fidelity,
        },
        commands: Mutex::new(Vec::new()),
    });
    let provisioner = HostAgentRunRuntimeProvisioner::new(
        host,
        host_repository,
        binding_repository.clone(),
        surface_store.clone(),
        Arc::new(UnusedSurfaceSource),
        gateway.clone(),
    );

    let recovered = provisioner
        .recover(&old, RuntimeRevision(7))
        .await
        .expect("recover binding from active context checkpoint");

    assert_ne!(recovered.binding_id, old.binding_id);
    assert_eq!(recovered.binding_epoch, BindingEpoch(2));
    assert_eq!(
        recovered.surface.context_recipe_revision,
        materialized.recipe.revision
    );
    assert_eq!(recovered.surface.context_digest, materialized.digest);
    assert_eq!(
        recovered.settings_revision,
        materialized.recipe.provenance.settings_revision
    );
    assert_eq!(
        recovered.surface.settings_revision,
        materialized.recipe.provenance.settings_revision
    );

    let rebound_surface = surface_store
        .load_surface(&recovered.binding_id)
        .await
        .expect("load rebound surface")
        .expect("rebound surface exists");
    assert_eq!(rebound_surface.context.recipe, materialized.recipe);
    assert_eq!(rebound_surface.context.blocks, materialized.blocks);
    assert_eq!(rebound_surface.context.digest, materialized.digest);
    assert_eq!(rebound_surface.context.fidelity, materialized.fidelity);
    assert_eq!(rebound_surface.tools, old_surface.tools);
    assert_eq!(rebound_surface.hooks, old_surface.hooks);
    assert_eq!(rebound_surface.workspace, old_surface.workspace);

    let bind_requests = driver.bind_requests.lock().await;
    assert_eq!(bind_requests.len(), 2);
    assert!(matches!(
        &bind_requests[1].intent,
        DriverBindIntent::Resume { source_thread_id }
            if source_thread_id == &old.source_thread_id
    ));
    drop(bind_requests);

    let commands = gateway.commands.lock().await;
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0].command,
        RuntimeCommand::ThreadRebind {
            expected_binding_id,
            new_binding_id,
            ..
        } if expected_binding_id == &old.binding_id && new_binding_id == &recovered.binding_id
    ));
    drop(commands);

    let state = binding_repository.state.lock().await;
    assert_eq!(state.current, recovered);
    assert_eq!(
        state.intent.as_ref().map(|intent| intent.state),
        Some(AgentRunRuntimeRecoveryState::Committed)
    );
}
