use std::{
    collections::BTreeSet,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use agentdash_agent_runtime::{ManagedAgentRuntime, RuntimeRepository, RuntimeStoreFixture};
use agentdash_agent_runtime_contract::*;
use agentdash_application_agentrun::agent_run::{
    AcceptAgentRunMessage, AgentRunMessageAcceptedDelivery, AgentRunMessageAdmission,
    AgentRunMessageDeliveryPreference, AgentRunPresentationDraft, AgentRunRuntime,
    AgentRunRuntimeApplicationPresentationProjector, AgentRunRuntimeError,
    CoordinateExecutionProfileRequest, CurrentAgentFrameExecutionProfileCoordinator,
    ExecutionProfileCoordination, ExecutionProfileCoordinationError, ExecutionProfileCoordinator,
    LaunchPresentationSource, ManagedAgentRunRuntime, SendAgentRunMessage,
};
use agentdash_application_ports::agent_run_runtime::*;
use agentdash_domain::workflow::{AgentFrame, AgentFrameRepository};
use agentdash_domain::{DomainError, common::AgentConfig};
use async_trait::async_trait;
use tokio::sync::{Barrier, Mutex};
use uuid::Uuid;

#[derive(Default)]
struct RecordingExecutionProfileCoordinator {
    requests: Mutex<Vec<CoordinateExecutionProfileRequest>>,
}

struct CurrentFrameRepository {
    frame: Mutex<AgentFrame>,
}

#[async_trait]
impl AgentFrameRepository for CurrentFrameRepository {
    async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
        *self.frame.lock().await = frame.clone();
        Ok(())
    }

    async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        let frame = self.frame.lock().await;
        Ok((frame.id == frame_id).then(|| frame.clone()))
    }

    async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        let frame = self.frame.lock().await;
        Ok((frame.agent_id == agent_id).then(|| frame.clone()))
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
        let frame = self.frame.lock().await;
        Ok((frame.agent_id == agent_id)
            .then(|| vec![frame.clone()])
            .unwrap_or_default())
    }
}

#[async_trait]
impl ExecutionProfileCoordinator for RecordingExecutionProfileCoordinator {
    async fn coordinate_started_turn(
        &self,
        request: CoordinateExecutionProfileRequest,
    ) -> Result<ExecutionProfileCoordination, ExecutionProfileCoordinationError> {
        self.requests.lock().await.push(request);
        Ok(ExecutionProfileCoordination::Unchanged)
    }
}

fn id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid id")
}

fn profile() -> RuntimeProfile {
    RuntimeProfile {
        reference_class: ReferenceRuntimeClass::ManagedThread,
        input: InputProfile {
            modalities: [InputModality::Text].into(),
        },
        instruction: InstructionProfile {
            channels: BTreeSet::new(),
            configuration_boundary: ConfigurationBoundary::Binding,
        },
        tools: ToolProfile {
            channels: BTreeSet::new(),
            configuration_boundary: ConfigurationBoundary::Binding,
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
        lifecycle: [
            LifecycleCapability::ThreadStart,
            LifecycleCapability::TurnStart,
            LifecycleCapability::TurnSteer,
            LifecycleCapability::TurnInterrupt,
        ]
        .into(),
        hooks: HookProfile {
            points: Vec::new(),
            configuration_boundary: ConfigurationBoundary::Binding,
        },
        context: ContextProfile {
            capabilities: BTreeSet::new(),
            fidelity: ContextFidelity::Opaque,
            activation_idempotent: false,
        },
        telemetry_config: BTreeSet::new(),
    }
}

#[derive(Default)]
struct CompositionFixture {
    binding: Mutex<Option<AgentRunRuntimeBinding>>,
    provisions: Mutex<usize>,
    backend_selection: Mutex<Option<agentdash_application_ports::launch::BackendSelectionInput>>,
    bootstrap_frames: Mutex<Vec<agentdash_agent_protocol::ContextFrame>>,
    turn_start_facts:
        Mutex<agentdash_application_ports::agent_run_runtime::AgentRunTurnStartContextFacts>,
    fail_turn_start_ack: Mutex<bool>,
}

impl CompositionFixture {
    async fn provision_count(&self) -> usize {
        *self.provisions.lock().await
    }

    async fn set_bootstrap_frames(&self, frames: Vec<agentdash_agent_protocol::ContextFrame>) {
        *self.bootstrap_frames.lock().await = frames;
    }

    async fn set_turn_start_facts(
        &self,
        facts: agentdash_application_ports::agent_run_runtime::AgentRunTurnStartContextFacts,
    ) {
        *self.turn_start_facts.lock().await = facts;
    }

    async fn fail_turn_start_ack(&self) {
        *self.fail_turn_start_ack.lock().await = true;
    }
}

#[async_trait]
impl AgentRunRuntimeBindingRepository for CompositionFixture {
    async fn load(
        &self,
        target: &AgentRunRuntimeTarget,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(self
            .binding
            .lock()
            .await
            .clone()
            .filter(|binding| &binding.target == target))
    }

    async fn load_by_thread_id(
        &self,
        thread_id: &RuntimeThreadId,
    ) -> Result<Option<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(self
            .binding
            .lock()
            .await
            .clone()
            .filter(|binding| &binding.thread_id == thread_id))
    }

    async fn list_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(self
            .binding
            .lock()
            .await
            .clone()
            .into_iter()
            .filter(|binding| binding.target.run_id == run_id)
            .collect())
    }

    async fn list_by_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<AgentRunRuntimeBinding>, AgentRunRuntimeBindingError> {
        Ok(self
            .binding
            .lock()
            .await
            .clone()
            .into_iter()
            .filter(|binding| binding.target.agent_id == agent_id)
            .collect())
    }

    async fn insert(
        &self,
        binding: AgentRunRuntimeBinding,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        let mut current = self.binding.lock().await;
        match current.as_ref() {
            Some(existing) if existing != &binding => Err(AgentRunRuntimeBindingError::Conflict),
            Some(existing) => Ok(existing.clone()),
            None => {
                *current = Some(binding.clone());
                Ok(binding)
            }
        }
    }
}

#[async_trait]
impl AgentRunRuntimeProvisioner for CompositionFixture {
    async fn provision(
        &self,
        request: &AgentRunRuntimeProvisionRequest,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeBindingError> {
        *self.provisions.lock().await += 1;
        *self.backend_selection.lock().await = request.backend_selection.clone();
        self.insert(AgentRunRuntimeBinding {
            target: request.target.clone(),
            presentation_thread_id: request.presentation_thread_id.clone(),
            thread_id: id("thread-facade"),
            binding_id: id("binding-facade"),
            binding_epoch: agentdash_agent_runtime_contract::BindingEpoch(1),
            driver_generation: RuntimeDriverGeneration(3),
            source_thread_id: id("source-thread-facade"),
            profile_digest: id("profile-facade"),
            profile_provenance: ProfileProvenance {
                service_digest: id("profile-service-facade"),
                transport_digest: id("profile-transport-facade"),
                host_policy_digest: id("profile-host-facade"),
            },
            bound_profile: profile(),
            surface: RuntimeSurfaceDescriptor {
                source_frame_id: "frame-facade".to_string(),
                surface_revision: SurfaceRevision(1),
                surface_digest: id("surface-facade"),
                vfs_digest: "vfs-facade".to_string(),
                context_recipe_revision: ContextRecipeRevision(1),
                context_digest: id("context-facade"),
                settings_revision: ThreadSettingsRevision(0),
                tool_set_revision: ToolSetRevision(0),
                tool_set_digest: "tools-facade".to_string(),
                hook_plan: BoundRuntimeHookPlan {
                    revision: HookPlanRevision(1),
                    digest: id("hook-plan-facade"),
                    entries: Vec::new(),
                },
                terminal_hook_effect_binding: None,
            },
            settings_revision: ThreadSettingsRevision(0),
            context_delivery_target:
                agentdash_application_ports::agent_run_runtime::AgentRunContextDeliveryTarget {
                    connector_id: "pi-agent".to_string(),
                    executor: "PI_AGENT".to_string(),
                },
        })
        .await
    }
}

#[async_trait]
impl agentdash_application_ports::agent_run_runtime::AgentRunRuntimePresentationPlanStore
    for CompositionFixture
{
    async fn load_exact_presentation_plan(
        &self,
        _binding_id: &RuntimeBindingId,
        surface_revision: SurfaceRevision,
        _surface_digest: &SurfaceDigest,
    ) -> Result<agentdash_agent_runtime::RuntimeSurfacePresentationPlan, AgentRunRuntimeBindingError>
    {
        Ok(agentdash_agent_runtime::RuntimeSurfacePresentationPlan {
            digest: format!("fixture-plan-{}", surface_revision.0),
            source_frame_id: "frame-facade".to_string(),
            source_frame_revision: surface_revision.0,
            transition_phase_node: Some("fixture".to_string()),
            bootstrap_frames: self.bootstrap_frames.lock().await.clone(),
            adoption_frames: Vec::new(),
        })
    }
}

#[async_trait]
impl agentdash_application_ports::agent_run_runtime::AgentRunTurnStartContextSource
    for CompositionFixture
{
    async fn take_turn_start_context(
        &self,
        _binding_id: &RuntimeBindingId,
    ) -> Result<
        agentdash_application_ports::agent_run_runtime::AgentRunTurnStartContextFacts,
        AgentRunRuntimeBindingError,
    > {
        Ok(self.turn_start_facts.lock().await.clone())
    }

    async fn acknowledge_turn_start_context(
        &self,
        _binding_id: &RuntimeBindingId,
        notice_ids: &[String],
    ) -> Result<(), AgentRunRuntimeBindingError> {
        if *self.fail_turn_start_ack.lock().await {
            return Err(AgentRunRuntimeBindingError::Unavailable {
                reason: "fixture turn-start acknowledgement failed".to_string(),
                retryable: true,
            });
        }
        self.turn_start_facts
            .lock()
            .await
            .notices
            .retain(|notice| !notice_ids.contains(&notice.id));
        Ok(())
    }
}

struct OperationPrecheckBarrierGateway {
    inner: Arc<dyn AgentRuntimeGateway>,
    barrier: Barrier,
    enabled: AtomicBool,
    missing_operation_prechecks: AtomicUsize,
}

impl OperationPrecheckBarrierGateway {
    fn new(inner: Arc<dyn AgentRuntimeGateway>) -> Self {
        Self {
            inner,
            barrier: Barrier::new(2),
            enabled: AtomicBool::new(false),
            missing_operation_prechecks: AtomicUsize::new(0),
        }
    }

    fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl AgentRuntimeGateway for OperationPrecheckBarrierGateway {
    async fn append_presentation(
        &self,
        request: RuntimePresentationAppendRequest,
    ) -> Result<RuntimePresentationAppendReceipt, RuntimePresentationAppendError> {
        self.inner.append_presentation(request).await
    }

    async fn execute(
        &self,
        command: RuntimeCommandEnvelope,
    ) -> Result<OperationReceipt, RuntimeExecuteError> {
        self.inner.execute(command).await
    }

    async fn snapshot(
        &self,
        query: RuntimeSnapshotQuery,
    ) -> Result<RuntimeSnapshotResult, RuntimeSnapshotError> {
        let operation_query = matches!(&query, RuntimeSnapshotQuery::Operation { .. });
        let result = self.inner.snapshot(query).await;
        if self.enabled.load(Ordering::SeqCst)
            && operation_query
            && matches!(&result, Err(RuntimeSnapshotError::NotFound))
            && self
                .missing_operation_prechecks
                .fetch_add(1, Ordering::SeqCst)
                < 2
        {
            self.barrier.wait().await;
        }
        result
    }

    async fn events(
        &self,
        subscription: RuntimeEventSubscription,
    ) -> Result<Box<dyn RuntimeEventStream>, RuntimeSubscribeError> {
        self.inner.events(subscription).await
    }
}

fn target() -> AgentRunRuntimeTarget {
    AgentRunRuntimeTarget {
        run_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("run id"),
        agent_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").expect("agent id"),
    }
}

fn send(text: &str) -> SendAgentRunMessage {
    SendAgentRunMessage {
        target: target(),
        presentation_thread_id: id("presentation-facade"),
        presentation: AgentRunPresentationDraft {
            content: agentdash_agent_protocol::text_user_input_blocks(text),
            source: agentdash_agent_protocol::UserInputSource::core_composer(),
            launch_source: LaunchPresentationSource::HttpPrompt,
            submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        },
        client_command_id: "client-command-1".to_string(),
        input: vec![RuntimeInput::text(text.to_string())],
        actor: RuntimeActor::User {
            subject: "subject-1".to_string(),
        },
        identity: None,
        backend_selection: None,
    }
}

fn event_presentation_turn_id(event: &serde_json::Value) -> &str {
    event
        .pointer("/payload/turnId")
        .or_else(|| event.pointer("/payload/data/value/turn_id"))
        .and_then(serde_json::Value::as_str)
        .expect("presentation event carries its turn identity")
}

fn rewrite_golden_admission_identity(
    value: &serde_json::Value,
    symbolic_turn_id: &str,
    actual_turn_id: &str,
) -> serde_json::Value {
    let mut rewritten = serde_json::from_str::<serde_json::Value>(
        &value.to_string().replace(symbolic_turn_id, actual_turn_id),
    )
    .expect("rewrite symbolic golden turn identity");
    if rewritten.get("type").and_then(serde_json::Value::as_str) == Some("turn_started") {
        let started_at = actual_turn_id
            .strip_prefix('t')
            .and_then(|millis| millis.parse::<i64>().ok())
            .expect("admission turn uses t<millis> identity")
            .div_euclid(1_000);
        rewritten["payload"]["turn"]["startedAt"] = serde_json::json!(started_at);
    }
    rewritten
}

fn accept_message(command: SendAgentRunMessage) -> AcceptAgentRunMessage {
    AcceptAgentRunMessage {
        target: command.target,
        presentation_thread_id: command.presentation_thread_id,
        presentation: command.presentation,
        client_command_id: command.client_command_id,
        input: command.input,
        actor: command.actor,
        identity: command.identity,
        execution_profile_override: None,
        backend_selection: command.backend_selection,
        delivery_preference: AgentRunMessageDeliveryPreference::StartWhenIdle,
    }
}

#[tokio::test]
async fn current_frame_profile_coordinator_noops_for_an_equivalent_partial_override() {
    let agent_id = Uuid::new_v4();
    let mut frame = AgentFrame::new_revision(agent_id, 3, "test");
    let mut current = AgentConfig::new("PI_AGENT");
    current.provider_id = Some("provider-current".to_string());
    current.model_id = Some("gpt-5.5".to_string());
    current.agent_id = Some("general".to_string());
    current.thinking_level = Some(agentdash_domain::common::ThinkingLevel::Minimal);
    current.system_prompt = Some("persisted frame instructions".to_string());
    frame.execution_profile_json = Some(serde_json::to_value(&current).expect("profile"));
    let frames = Arc::new(CurrentFrameRepository {
        frame: Mutex::new(frame),
    });
    let coordinator = CurrentAgentFrameExecutionProfileCoordinator::new(frames);
    let target = AgentRunRuntimeTarget {
        run_id: Uuid::new_v4(),
        agent_id,
    };

    let same = coordinator
        .coordinate_started_turn(CoordinateExecutionProfileRequest {
            target: target.clone(),
            binding: None,
            execution_profile_override: Some(AgentConfig {
                executor: "pi_agent".to_string(),
                provider_id: None,
                model_id: Some("gpt-5.5".to_string()),
                agent_id: None,
                thinking_level: Some(agentdash_domain::common::ThinkingLevel::Minimal),
                system_prompt: None,
            }),
        })
        .await
        .expect("omitted persisted fields must inherit the current effective profile");
    assert_eq!(same, ExecutionProfileCoordination::Unchanged);

    let applied = coordinator
        .coordinate_started_turn(CoordinateExecutionProfileRequest {
            target: target.clone(),
            binding: None,
            execution_profile_override: Some(AgentConfig::new("codex")),
        })
        .await
        .expect("unbound run can apply the profile before provisioning");
    assert!(matches!(
        applied,
        ExecutionProfileCoordination::FrameRevisionApplied { .. }
    ));

    let composition = CompositionFixture::default();
    let binding = composition
        .provision(&AgentRunRuntimeProvisionRequest {
            target: target.clone(),
            presentation_thread_id: id("profile-coordinator-thread"),
            identity: None,
            backend_selection: None,
            fork: None,
            terminal_hook_effect_binding: None,
        })
        .await
        .expect("binding fixture");
    let error = coordinator
        .coordinate_started_turn(CoordinateExecutionProfileRequest {
            target,
            binding: Some(binding),
            execution_profile_override: Some(AgentConfig::new("PI_AGENT")),
        })
        .await
        .expect_err("bound run requires planned service rebind");
    assert!(matches!(
        error,
        ExecutionProfileCoordinationError::RebindUnsupported { .. }
    ));
}

#[tokio::test]
async fn started_override_is_coordinated_once_after_stable_replay_precheck() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
        store,
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let composition = Arc::new(CompositionFixture::default());
    let coordinator = Arc::new(RecordingExecutionProfileCoordinator::default());
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition,
    )
    .with_execution_profile_coordinator(coordinator.clone());
    let mut command = accept_message(send("profile-override"));
    command.execution_profile_override = Some(agentdash_spi::AgentConfig::new("PI_AGENT"));

    let first = facade
        .accept_message(command.clone())
        .await
        .expect("first start");
    assert!(matches!(
        first,
        AgentRunMessageAdmission::Accepted {
            delivery: AgentRunMessageAcceptedDelivery::Started,
            ..
        }
    ));
    assert_eq!(coordinator.requests.lock().await.len(), 1);

    let replay = facade.accept_message(command).await.expect("stable replay");
    assert!(matches!(
        replay,
        AgentRunMessageAdmission::Accepted {
            delivery: AgentRunMessageAcceptedDelivery::Started,
            ..
        }
    ));
    assert_eq!(
        coordinator.requests.lock().await.len(),
        1,
        "stable operation replay must not reapply execution profile"
    );
}

#[tokio::test]
async fn steer_ignores_execution_profile_override() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
        store,
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let composition = Arc::new(CompositionFixture::default());
    let coordinator = Arc::new(RecordingExecutionProfileCoordinator::default());
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition,
    )
    .with_execution_profile_coordinator(coordinator.clone());
    facade
        .accept_message(accept_message(send("establish active turn")))
        .await
        .expect("start active turn");
    coordinator.requests.lock().await.clear();

    let mut steer = accept_message(send("steer with ignored override"));
    steer.client_command_id = "steer-profile-override".to_string();
    steer.delivery_preference = AgentRunMessageDeliveryPreference::PreferSteer;
    steer.execution_profile_override = Some(agentdash_spi::AgentConfig::new("OTHER"));
    let admission = facade.accept_message(steer).await.expect("steer");

    assert!(matches!(
        admission,
        AgentRunMessageAdmission::Accepted {
            delivery: AgentRunMessageAcceptedDelivery::Steered,
            ..
        }
    ));
    assert!(
        coordinator.requests.lock().await.is_empty(),
        "steering must not mutate or validate the run execution profile"
    );
}

#[tokio::test]
async fn message_admission_generates_launch_time_when_consumed_and_replays_it_exactly() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let composition = Arc::new(CompositionFixture::default());
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition,
    );
    let command = accept_message(send("consume-time"));
    let consume_not_before = chrono::Utc::now().timestamp_millis();

    let first = facade
        .accept_message(command.clone())
        .await
        .expect("first admission");
    let AgentRunMessageAdmission::Accepted { receipt, delivery } = first else {
        panic!("idle message must start")
    };
    assert_eq!(delivery, AgentRunMessageAcceptedDelivery::Started);
    assert!(!receipt.duplicate);
    let first_records = store
        .journal_records_after(&id("thread-facade"), None)
        .await
        .expect("presentation journal")
        .records;
    let first_turn_id = first_records
        .iter()
        .find_map(|record| record.carrier().coordinate.presentation_turn_id.clone())
        .expect("admission presentation turn");
    let admitted_at = first_turn_id
        .as_str()
        .strip_prefix('t')
        .and_then(|millis| millis.parse::<i64>().ok())
        .expect("launch identity uses admission millis");
    assert!(admitted_at >= consume_not_before);

    let mut replay_command = command;
    replay_command.delivery_preference = AgentRunMessageDeliveryPreference::PreferSteer;
    let replay = facade
        .accept_message(replay_command)
        .await
        .expect("reconciliation replays before applying the current delivery policy");
    let AgentRunMessageAdmission::Accepted { receipt, delivery } = replay else {
        panic!("existing operation must replay")
    };
    assert_eq!(
        delivery,
        AgentRunMessageAcceptedDelivery::Started,
        "Promote/claim policy changes cannot turn an accepted start into a new steer"
    );
    assert!(receipt.duplicate);
    let replay_records = store
        .journal_records_after(&id("thread-facade"), None)
        .await
        .expect("presentation journal")
        .records;
    assert_eq!(replay_records.len(), first_records.len());
    assert_eq!(
        replay_records.iter().find_map(|record| record
            .carrier()
            .coordinate
            .presentation_turn_id
            .clone()),
        Some(first_turn_id)
    );
}

#[tokio::test]
async fn accepted_message_is_not_reversed_when_turn_start_context_acknowledgement_fails() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let composition = Arc::new(CompositionFixture::default());
    composition
        .set_turn_start_facts(
            agentdash_application_ports::agent_run_runtime::AgentRunTurnStartContextFacts {
                runtime_snapshot: None,
                pending_actions: Vec::new(),
                notices: vec![agentdash_spi::HookTurnStartNotice {
                    id: "ack-failure-notice".to_string(),
                    created_at_ms: 42,
                    source: agentdash_spi::RuntimeEventSource::RuntimeContextUpdate,
                    content: "retry acknowledgement".to_string(),
                    presentation: None,
                }],
            },
        )
        .await;
    composition.fail_turn_start_ack().await;
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition,
    );
    let command = accept_message(send("ack-failure"));

    let first = facade
        .accept_message(command.clone())
        .await
        .expect("durable acceptance wins over acknowledgement failure");
    let AgentRunMessageAdmission::Accepted { receipt, .. } = first else {
        panic!("idle message must be accepted")
    };
    assert!(!receipt.duplicate);
    let record_count = store
        .journal_records_after(&id("thread-facade"), None)
        .await
        .expect("presentation journal")
        .records
        .len();

    let replay = facade
        .accept_message(command)
        .await
        .expect("accepted operation remains replayable");
    let AgentRunMessageAdmission::Accepted { receipt, .. } = replay else {
        panic!("accepted operation must replay")
    };
    assert!(receipt.duplicate);
    assert_eq!(
        store
            .journal_records_after(&id("thread-facade"), None)
            .await
            .expect("presentation journal")
            .records
            .len(),
        record_count
    );
}

#[tokio::test]
async fn concurrent_message_attempts_reconcile_after_both_miss_the_operation_precheck() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let runtime: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let gateway = Arc::new(OperationPrecheckBarrierGateway::new(runtime));
    let composition = Arc::new(CompositionFixture::default());
    let facade = Arc::new(ManagedAgentRunRuntime::new(
        gateway.clone(),
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition,
    ));
    facade
        .accept_message(accept_message(send("establish active turn")))
        .await
        .expect("initial turn");
    gateway.enable();
    let mut command = accept_message(send("lease race steer"));
    command.client_command_id = "mailbox-race-message".to_string();
    command.delivery_preference = AgentRunMessageDeliveryPreference::PreferSteer;

    let (left, right) = tokio::join!(
        facade.accept_message(command.clone()),
        facade.accept_message(command)
    );
    let accepted = [left.unwrap(), right.unwrap()]
        .into_iter()
        .map(|admission| match admission {
            AgentRunMessageAdmission::Accepted { receipt, delivery } => {
                assert_eq!(delivery, AgentRunMessageAcceptedDelivery::Steered);
                receipt
            }
            AgentRunMessageAdmission::Deferred => panic!("active preferred steer must be accepted"),
        })
        .collect::<Vec<_>>();
    assert_eq!(accepted[0].operation_id, accepted[1].operation_id);
    assert_ne!(accepted[0].duplicate, accepted[1].duplicate);
    assert_eq!(
        store
            .journal_records_after(&id("thread-facade"), None)
            .await
            .expect("presentation journal")
            .records
            .into_iter()
            .filter(|record| record.as_presentation().is_some())
            .count(),
        3,
        "the lease race must persist one steer presentation"
    );
}

#[tokio::test]
async fn first_send_provisions_once_and_retry_replays_the_original_thread_start() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let runtime = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let gateway: Arc<dyn AgentRuntimeGateway> = runtime.clone();
    let composition = Arc::new(CompositionFixture::default());
    let bootstrap_frame = agentdash_agent_protocol::ContextFrame {
        id: "bootstrap-capability-frame".to_string(),
        kind: agentdash_agent_protocol::ContextFrameKind::CapabilityStateDelta,
        source: agentdash_agent_protocol::ContextFrameSource::RuntimeContextUpdate,
        phase_node: Some("bootstrap".to_string()),
        apply_mode: Some("initial".to_string()),
        delivery_status: agentdash_agent_protocol::ContextDeliveryStatus::QueuedForTransformContext,
        delivery_channel: agentdash_agent_protocol::ContextDeliveryChannel::TurnStart,
        message_role: agentdash_agent_protocol::ContextMessageRole::User,
        delivery_metadata: agentdash_agent_protocol::ContextDeliveryMetadata::for_frame(
            agentdash_agent_protocol::ContextFrameKind::CapabilityStateDelta,
            agentdash_agent_protocol::ContextDeliveryChannel::TurnStart,
            agentdash_agent_protocol::ContextMessageRole::User,
        ),
        rendered_text: "capability bootstrap".to_string(),
        sections: Vec::new(),
        created_at_ms: 1_783_684_800_000,
    };
    composition
        .set_bootstrap_frames(vec![bootstrap_frame.clone()])
        .await;
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition.clone(),
    );

    let accepted = facade.send_message(send("hello")).await.expect("send");
    assert!(!accepted.duplicate);
    let presentation = store
        .journal_records_after(&id("thread-facade"), None)
        .await
        .expect("presentation journal")
        .records
        .into_iter()
        .filter(|record| record.as_presentation().is_some())
        .collect::<Vec<_>>();
    assert_eq!(presentation.len(), 3);
    assert_eq!(
        presentation[0].carrier().coordinate.source_entry_index,
        Some(0)
    );
    assert_eq!(
        presentation[1].carrier().coordinate.source_entry_index,
        None
    );
    assert_eq!(
        presentation[2].carrier().coordinate.source_entry_index,
        Some(1)
    );
    assert_eq!(
        presentation[0]
            .carrier()
            .coordinate
            .source_request_id
            .as_deref(),
        Some("client-command-1")
    );
    assert_eq!(
        presentation[1]
            .carrier()
            .coordinate
            .source_request_id
            .as_deref(),
        Some("client-command-1")
    );
    let source_turn_id = presentation[0]
        .carrier()
        .coordinate
        .source_turn_id
        .clone()
        .expect("admission presentation turn");
    let source_started_at = source_turn_id
        .strip_prefix('t')
        .and_then(|millis| millis.parse::<i64>().ok())
        .expect("admission presentation turn uses t<millis>")
        .div_euclid(1_000);
    for record in &presentation {
        assert_eq!(
            record.carrier().coordinate.source_thread_id.as_deref(),
            Some("presentation-facade")
        );
        assert_eq!(
            record.carrier().coordinate.source_turn_id.as_deref(),
            Some(source_turn_id.as_str())
        );
    }
    let canonical_turn_id =
        RuntimeTurnId::new(format!("turn-{}", accepted.operation_id)).expect("canonical turn id");
    assert_eq!(
        presentation[0].carrier().coordinate.runtime_turn_id,
        Some(canonical_turn_id.clone())
    );
    assert_eq!(
        presentation[1].carrier().coordinate.runtime_turn_id,
        Some(canonical_turn_id)
    );
    assert!(matches!(
        &presentation[0]
            .as_presentation()
            .expect("user submission presentation")
            .event,
        agentdash_agent_protocol::BackboneEvent::UserInputSubmitted(_)
    ));
    assert!(matches!(
        &presentation[2]
            .as_presentation()
            .expect("context frame presentation")
            .event,
        agentdash_agent_protocol::BackboneEvent::Platform(
            agentdash_agent_protocol::PlatformEvent::ContextFrameChanged(_)
        )
    ));
    let actual_bootstrap_frame = serde_json::to_value(
        &presentation[2]
            .as_presentation()
            .expect("context frame presentation")
            .event,
    )
    .unwrap()
    .pointer("/payload/data/frame")
    .cloned()
    .unwrap();
    let mut expected_bootstrap_frame = serde_json::to_value(&bootstrap_frame).unwrap();
    expected_bootstrap_frame["delivery_metadata"] =
        actual_bootstrap_frame["delivery_metadata"].clone();
    assert_eq!(actual_bootstrap_frame, expected_bootstrap_frame);
    assert_eq!(
        actual_bootstrap_frame
            .pointer("/delivery_metadata/agent_consumption/target")
            .and_then(serde_json::Value::as_str),
        Some("pi-agent:PI_AGENT")
    );
    assert!(matches!(
        &presentation[1]
            .as_presentation()
            .expect("turn started presentation")
            .event,
        agentdash_agent_protocol::BackboneEvent::TurnStarted(_)
    ));
    assert_eq!(
        presentation[1]
            .carrier()
            .sequence
            .expect("turn started durable sequence")
            .0,
        presentation[0]
            .carrier()
            .sequence
            .expect("user submission durable sequence")
            .0
            + 1,
        "同一提交批次中的 presentation facts 必须保持原子顺序"
    );
    let protected_bodies = presentation
        .iter()
        .take(2)
        .map(|record| {
            serde_json::to_value(&record.as_presentation().expect("presentation body").event)
                .expect("serialize protected body")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        protected_bodies,
        vec![
            serde_json::json!({
                "type": "user_input_submitted",
                "payload": {
                    "threadId": "presentation-facade",
                    "turnId": source_turn_id,
                    "itemId": format!("{source_turn_id}:user-input:0"),
                    "submissionKind": "prompt",
                    "source": {
                        "namespace": "core",
                        "kind": "composer",
                        "actor": "user",
                        "displayLabelKey": "mailbox.source.core.composer"
                    },
                    "content": [{
                        "type": "text",
                        "text": "hello",
                        "text_elements": []
                    }]
                }
            }),
            serde_json::json!({
                "type": "turn_started",
                "payload": {
                    "threadId": "presentation-facade",
                    "turn": {
                        "id": source_turn_id,
                        "items": [],
                        "itemsView": "notLoaded",
                        "status": "inProgress",
                        "error": null,
                        "startedAt": source_started_at,
                        "completedAt": null,
                        "durationMs": null
                    }
                }
            }),
        ],
        "坐标补充不得改变 Main 受保护事件体或 UserInputSubmitted → TurnStarted 顺序"
    );
    let replayed = facade.send_message(send("hello")).await.expect("retry");
    assert!(replayed.duplicate);
    assert_eq!(replayed.operation_id, accepted.operation_id);
    assert_eq!(composition.provision_count().await, 1);

    let view = facade.inspect(target()).await.expect("inspect");
    let binding = view.binding.as_ref().expect("runtime binding");
    assert_eq!(
        binding.presentation_thread_id.as_str(),
        "presentation-facade"
    );
    assert_ne!(
        binding.presentation_thread_id.as_str(),
        binding.thread_id.as_str()
    );
    assert_eq!(
        view.binding_epoch,
        Some(agentdash_agent_runtime_contract::BindingEpoch(1))
    );
    assert_eq!(
        view.recovery,
        agentdash_application_agentrun::agent_run::AgentRunRuntimeRecoverySummary::Active
    );
    assert_eq!(
        view.snapshot.expect("snapshot").thread_id,
        id("thread-facade")
    );

    let mut presentation_conflict = send("hello");
    presentation_conflict.presentation.source.actor = "another-user".to_string();
    let conflict = facade
        .send_message(presentation_conflict)
        .await
        .expect_err("client command identity cannot be reused with another presentation");
    assert!(matches!(
        conflict,
        AgentRunRuntimeError::ClientCommandConflict
    ));

    let conflict = facade
        .send_message(send("different"))
        .await
        .expect_err("client command identity cannot be reused with another input");
    assert!(matches!(
        conflict,
        AgentRunRuntimeError::ClientCommandConflict
    ));
}

#[tokio::test]
async fn compiled_full_bootstrap_is_committed_by_real_thread_start_in_main_order() {
    use agentdash_agent_protocol::ContextFrameKind;
    use agentdash_agent_runtime::{
        AgentSurfaceCompiler, AssignmentContextFacts, AssignmentFragmentFacts,
        BootstrapContextFacts, BusinessAgentSurfaceFacts, ContributionRequirement,
        DiscoveredGuidelineFacts, EnvironmentContextFacts, GuidelinesContextFacts,
        IdentityContextFacts, MemoryContextFacts, MemorySourceFacts, NormalizedAssignmentContext,
        NormalizedContextSurfaceState, SurfaceSourceRef, UserContextFacts, WorkspaceRequirement,
    };

    let source = SurfaceSourceRef {
        layer: "agent_frame".to_string(),
        key: "frame-facade".to_string(),
    };
    let assignment_fragment = AssignmentFragmentFacts {
        slot: "task".to_string(),
        label: "Task".to_string(),
        order: 10,
        runtime_agent_scope: true,
        source: "task".to_string(),
        content: "## Task\nRestore ContextFrame".to_string(),
        context_usage_kind: Some("system_developer".to_string()),
    };
    let bootstrap_context = BootstrapContextFacts {
        include_startup_context: true,
        identity: IdentityContextFacts {
            base_system_prompt: "base identity".to_string(),
            agent_identity_markdown: Some("## Agent Identity\n- preset: `general`".to_string()),
            agent_system_prompt: Some("agent rules".to_string()),
        },
        user: Some(UserContextFacts {
            user_id: "user-1".to_string(),
            display_name: Some("User One".to_string()),
            email: None,
            groups: vec!["Developers".to_string()],
            provider: Some("oidc".to_string()),
            extra: serde_json::Value::Null,
        }),
        environment: EnvironmentContextFacts {
            date_utc: "2026-07-14".to_string(),
            platform: "windows".to_string(),
            arch: "x86_64".to_string(),
            model_id: Some("model-1".to_string()),
            executor: "PI_AGENT".to_string(),
            working_directory: Some("D:/workspace".to_string()),
        },
        guidelines: GuidelinesContextFacts {
            user_preferences: vec!["使用中文".to_string()],
            discovered_guidelines: vec![DiscoveredGuidelineFacts {
                path: "AGENTS.md".to_string(),
                content: "项目约定".to_string(),
            }],
        },
        memory: MemoryContextFacts {
            sources: vec![MemorySourceFacts {
                provider_key: "builtin".to_string(),
                source_key: "agent".to_string(),
                display_name: "Agent Memory".to_string(),
                source_uri: "agent://".to_string(),
                index_uri: "agent://MEMORY.md".to_string(),
                mount_id: "agent".to_string(),
                scope: "agent".to_string(),
                capabilities: vec!["read".to_string()],
                index_status: "present".to_string(),
                trust_level: "first_party".to_string(),
                revision: "memory-revision".to_string(),
                summary: None,
                bounded_index_content: Some("- [Decision](topics/decision.md)".to_string()),
                context_usage_kind: Some("memory".to_string()),
            }],
            diagnostics: Vec::new(),
        },
        assignment: AssignmentContextFacts {
            phase_tag: Some("bootstrap".to_string()),
            apply_mode: None,
            fragments: vec![assignment_fragment.clone()],
        },
    };
    let normalized_context_surface = NormalizedContextSurfaceState {
        capability_keys: ["file_read".to_string()].into(),
        assignment: Some(NormalizedAssignmentContext {
            revision: 1,
            fragments: vec![agentdash_agent_protocol::RuntimeContextFragmentEntry {
                slot: assignment_fragment.slot,
                label: assignment_fragment.label,
                source: assignment_fragment.source,
                content: assignment_fragment.content,
                context_usage_kind: assignment_fragment.context_usage_kind,
            }],
        }),
        ..Default::default()
    };
    let artifact = AgentSurfaceCompiler
        .compile_business_facts(BusinessAgentSurfaceFacts {
            revision: SurfaceRevision(1),
            context_recipe: ContextRecipe {
                revision: ContextRecipeRevision(1),
                provenance: ContextProvenance {
                    settings_revision: ThreadSettingsRevision(0),
                    tool_set_revision: ToolSetRevision(1),
                },
                source_item_ids: Vec::new(),
            },
            tool_set_revision: ToolSetRevision(1),
            hook_plan_revision: HookPlanRevision(1),
            workspace: WorkspaceRequirement {
                capabilities: BTreeSet::new(),
                minimum_mechanism: DeliveryMechanism::Native,
                requirement: ContributionRequirement::Required,
            },
            source,
            transition_phase_node: Some("bootstrap".to_string()),
            instructions: Vec::new(),
            tools: Vec::new(),
            hooks: Vec::new(),
            bootstrap_context,
            normalized_context_surface,
            projection_identity: agentdash_agent_runtime::ContextProjectionIdentity {
                operation_id: "fixture-context-projection".to_string(),
                source_frame_id: "frame-facade".to_string(),
                source_frame_revision: 1,
                recorded_at_ms: 1_783_684_800_000,
            },
        })
        .expect("compile full business surface");
    assert_eq!(
        artifact
            .snapshot
            .context
            .instructions
            .entries
            .iter()
            .map(|entry| (entry.meta.key.as_str(), entry.channel))
            .collect::<Vec<_>>(),
        vec![
            ("bootstrap:identity", InstructionChannel::System),
            ("bootstrap:user_context", InstructionChannel::System),
            ("bootstrap:environment", InstructionChannel::System),
            ("bootstrap:system_guidelines", InstructionChannel::System),
        ]
    );
    assert_eq!(
        artifact
            .snapshot
            .context
            .contributions
            .iter()
            .map(|entry| entry.meta.key.as_str())
            .collect::<Vec<_>>(),
        vec!["bootstrap:assignment_context", "bootstrap:memory_context"]
    );
    let stable_frame_texts = artifact
        .presentation
        .bootstrap_frames
        .iter()
        .filter(|frame| {
            matches!(
                frame.kind,
                ContextFrameKind::Identity
                    | ContextFrameKind::UserContext
                    | ContextFrameKind::Environment
                    | ContextFrameKind::SystemGuidelines
            )
        })
        .map(|frame| frame.rendered_text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        artifact
            .snapshot
            .context
            .instructions
            .entries
            .iter()
            .map(|entry| entry.content.as_str())
            .collect::<Vec<_>>(),
        stable_frame_texts,
        "driver instructions and presentation frames must materialize from the same facts"
    );
    let context_frame_texts = [
        ContextFrameKind::AssignmentContext,
        ContextFrameKind::MemoryContext,
    ]
    .into_iter()
    .map(|kind| {
        artifact
            .presentation
            .bootstrap_frames
            .iter()
            .find(|frame| frame.kind == kind)
            .expect("context frame")
            .rendered_text
            .as_str()
    })
    .collect::<Vec<_>>();
    assert_eq!(
        artifact
            .snapshot
            .context
            .contributions
            .iter()
            .flat_map(|entry| entry.blocks.iter())
            .map(|block| match block {
                ContextBlock::Instruction { text } => text.as_str(),
                block => panic!("bootstrap model context must be typed instruction: {block:?}"),
            })
            .collect::<Vec<_>>(),
        context_frame_texts,
        "driver context blocks and presentation frames must materialize from the same facts"
    );
    assert_eq!(
        artifact
            .presentation
            .bootstrap_frames
            .iter()
            .map(|frame| frame.kind)
            .collect::<Vec<_>>(),
        vec![
            ContextFrameKind::CapabilityStateDelta,
            ContextFrameKind::AssignmentContext,
            ContextFrameKind::Identity,
            ContextFrameKind::UserContext,
            ContextFrameKind::Environment,
            ContextFrameKind::SystemGuidelines,
            ContextFrameKind::MemoryContext,
        ]
    );

    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let composition = Arc::new(CompositionFixture::default());
    composition
        .set_bootstrap_frames(artifact.presentation.bootstrap_frames)
        .await;
    composition
        .set_turn_start_facts(
            agentdash_application_ports::agent_run_runtime::AgentRunTurnStartContextFacts {
                runtime_snapshot: Some(agentdash_spi::hooks::AgentFrameRuntimeSnapshot {
                    revision: 1,
                    ..Default::default()
                }),
                pending_actions: vec![agentdash_spi::HookPendingAction {
                    id: "bootstrap-pending".to_string(),
                    created_at_ms: 1_783_684_800_001,
                    title: "Bootstrap pending".to_string(),
                    summary: "pending action".to_string(),
                    action_type: "blocking_review".to_string(),
                    turn_id: None,
                    source: agentdash_spi::RuntimeEventSource::RuntimeContextUpdate,
                    status: agentdash_spi::HookPendingActionStatus::Pending,
                    last_injected_at_ms: None,
                    resolved_at_ms: None,
                    resolution_kind: None,
                    resolution_note: None,
                    resolution_turn_id: None,
                    injections: Vec::new(),
                }],
                notices: Vec::new(),
            },
        )
        .await;
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition,
    );
    let mut bootstrap = send("bootstrap");
    bootstrap.presentation.launch_source = LaunchPresentationSource::WorkflowOrchestrator;
    facade
        .send_message(bootstrap)
        .await
        .expect("commit real ThreadStart");

    let durable_kinds = store
        .journal_records_after(&id("thread-facade"), None)
        .await
        .expect("runtime journal")
        .records
        .into_iter()
        .filter_map(
            |record| match record.as_presentation().map(|record| &record.event) {
                Some(agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::ContextFrameChanged(changed),
                )) => Some(changed.frame.kind),
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(
        durable_kinds,
        vec![
            ContextFrameKind::CapabilityStateDelta,
            ContextFrameKind::AssignmentContext,
            ContextFrameKind::SystemDelivery,
            ContextFrameKind::Identity,
            ContextFrameKind::UserContext,
            ContextFrameKind::Environment,
            ContextFrameKind::SystemGuidelines,
            ContextFrameKind::MemoryContext,
            ContextFrameKind::PendingAction,
        ]
    );
}

#[tokio::test]
async fn turn_start_pending_and_system_delivery_match_main_stream_family_and_order() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let runtime = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let gateway: Arc<dyn AgentRuntimeGateway> = runtime.clone();
    let composition = Arc::new(CompositionFixture::default());
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition.clone(),
    );
    let accepted_start = facade.send_message(send("first")).await.unwrap();
    let snapshot = facade.inspect(target()).await.unwrap().snapshot.unwrap();
    runtime
        .ingest_driver_event(DriverEventEnvelope {
            binding_id: id("binding-facade"),
            generation: RuntimeDriverGeneration(3),
            operation_id: Some(accepted_start.operation_id),
            source_thread_id: id("source-thread-facade"),
            source_turn_id: None,
            source_item_id: None,
            source_request_id: None,
            source_entry_index: None,
            facts: vec![RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                turn_id: snapshot.active_turn_id.unwrap(),
                terminal: RuntimeTurnTerminal::Completed,
                message: None,
                diagnostic: None,
            })],
        })
        .await
        .unwrap();
    composition
        .set_turn_start_facts(
            agentdash_application_ports::agent_run_runtime::AgentRunTurnStartContextFacts {
                runtime_snapshot: Some(agentdash_spi::hooks::AgentFrameRuntimeSnapshot {
                    revision: 42,
                    ..Default::default()
                }),
                pending_actions: vec![agentdash_spi::HookPendingAction {
                    id: "a1".to_string(),
                    created_at_ms: 9,
                    title: "Review".to_string(),
                    summary: "result".to_string(),
                    action_type: "blocking_review".to_string(),
                    turn_id: None,
                    source: agentdash_spi::RuntimeEventSource::CompanionResult,
                    status: agentdash_spi::HookPendingActionStatus::Pending,
                    last_injected_at_ms: None,
                    resolved_at_ms: None,
                    resolution_kind: None,
                    resolution_note: None,
                    resolution_turn_id: None,
                    injections: Vec::new(),
                }],
                notices: vec![agentdash_spi::HookTurnStartNotice {
                    id: "notice-1".to_string(),
                    created_at_ms: 10,
                    source: agentdash_spi::RuntimeEventSource::RuntimeContextUpdate,
                    content: "notice".to_string(),
                    presentation: None,
                }],
            },
        )
        .await;
    let mut second = send("continue");
    second.client_command_id = "client-command-2".to_string();
    second.presentation.launch_source = LaunchPresentationSource::HookAutoResume;
    facade.send_message(second).await.unwrap();
    let frames = store
        .journal_records_after(&id("thread-facade"), None)
        .await
        .unwrap()
        .records
        .into_iter()
        .filter_map(
            |record| match record.as_presentation().map(|event| &event.event) {
                Some(agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::ContextFrameChanged(changed),
                )) => Some(changed.frame.clone()),
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 3);
    assert_eq!(
        frames.iter().map(|frame| frame.kind).collect::<Vec<_>>(),
        vec![
            agentdash_agent_protocol::ContextFrameKind::SystemDelivery,
            agentdash_agent_protocol::ContextFrameKind::SystemNotice,
            agentdash_agent_protocol::ContextFrameKind::PendingAction,
        ]
    );
    assert_eq!(
        frames[0].delivery_channel,
        agentdash_agent_protocol::ContextDeliveryChannel::ConnectorContext
    );
    assert!(frames[0].rendered_text.contains("kind: hook_auto_resume"));
    assert!(matches!(
        frames[2].sections.as_slice(),
        [agentdash_agent_protocol::ContextFrameSection::PendingAction { revision: 42, .. }]
    ));
    for frame in &frames {
        assert_eq!(
            frame.delivery_metadata.connector_profile.profile_id,
            "pi-agent:PI_AGENT"
        );
        assert_eq!(
            frame
                .delivery_metadata
                .connector_profile
                .declared_consumption_modes,
            vec![
                agentdash_agent_protocol::ContextAgentConsumptionMode::Consume,
                agentdash_agent_protocol::ContextAgentConsumptionMode::Ignore,
                agentdash_agent_protocol::ContextAgentConsumptionMode::ConnectorNative,
                agentdash_agent_protocol::ContextAgentConsumptionMode::SystemAppend,
            ]
        );
        assert_eq!(
            frame.delivery_metadata.agent_consumption.target,
            "pi-agent:PI_AGENT"
        );
        assert_eq!(
            frame.delivery_metadata.agent_consumption.mode,
            agentdash_agent_protocol::ContextAgentConsumptionMode::Consume
        );
        assert_eq!(
            frame.delivery_metadata.agent_consumption.reason,
            format!("pi-agent:PI_AGENT_{}_delivery", frame.kind.as_key())
        );
    }
}

#[tokio::test]
async fn runtime_facade_prompt_matches_main_user_submit_golden_exactly() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../agentdash-agent-runtime-test-support/fixtures/session-parity/main/user-submit.json"
    ))
    .expect("Main user-submit golden");
    assert_eq!(
        golden["provenance"]["oracle_commit"],
        "957fa9d60ea3d67efa1bb278fe5b376cf0c34598"
    );

    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let composition = Arc::new(CompositionFixture::default());
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition,
    );
    let mut command = send("hello");
    command.presentation_thread_id = id("session-main-0001");

    facade
        .send_message(command)
        .await
        .expect("send through facade");

    let current = store
        .journal_records_after(&id("thread-facade"), None)
        .await
        .expect("runtime journal")
        .records
        .into_iter()
        .filter_map(|record| {
            record.as_presentation().map(|presentation| {
                assert_eq!(presentation.durability, PresentationDurability::Durable);
                serde_json::to_value(&presentation.event).expect("serialize protected body")
            })
        })
        .collect::<Vec<_>>();
    let actual_turn_id = event_presentation_turn_id(&current[0]);
    let main = golden["frames"]
        .as_array()
        .expect("golden frames")
        .iter()
        .map(|frame| {
            rewrite_golden_admission_identity(
                &frame["notification"]["event"],
                "turn-main-0001",
                actual_turn_id,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        current, main,
        "only the transport wrapper may differ from Main"
    );
}

#[tokio::test]
async fn runtime_facade_steer_matches_main_input_steer_golden_exactly() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/input-steer.json"
    ))
    .expect("Main input-steer golden");
    assert_eq!(
        golden["provenance"]["oracle_commit"],
        "957fa9d60ea3d67efa1bb278fe5b376cf0c34598"
    );

    let store = Arc::new(RuntimeStoreFixture::default());
    let runtime = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let gateway: Arc<dyn AgentRuntimeGateway> = runtime.clone();
    let composition = Arc::new(CompositionFixture::default());
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition,
    );
    let mut initial = send("hello");
    initial.presentation_thread_id = id("session-main-0001");
    let accepted_start = facade
        .send_message(initial)
        .await
        .expect("establish active turn");
    let snapshot = facade
        .inspect(target())
        .await
        .expect("inspect active turn")
        .snapshot
        .expect("runtime snapshot");
    let active_presentation_turn_id = snapshot
        .active_presentation_turn_id
        .clone()
        .expect("active presentation turn");

    let steer = AcceptAgentRunMessage {
        target: target(),
        presentation_thread_id: id("session-main-0001"),
        presentation: AgentRunPresentationDraft {
            content: agentdash_agent_protocol::text_user_input_blocks("steer now"),
            source: agentdash_agent_protocol::UserInputSource::core_composer(),
            launch_source: LaunchPresentationSource::LifecycleAgentUserMessage,
            submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        },
        input: vec![RuntimeInput::text("steer now".to_string())],
        client_command_id: "mailbox-33333333-3333-3333-3333-333333333333".to_string(),
        actor: RuntimeActor::User {
            subject: "subject-1".to_string(),
        },
        identity: None,
        execution_profile_override: None,
        backend_selection: None,
        delivery_preference: AgentRunMessageDeliveryPreference::PreferSteer,
    };
    let accepted = facade
        .accept_message(steer.clone())
        .await
        .expect("steer active turn");
    let AgentRunMessageAdmission::Accepted {
        receipt: accepted,
        delivery,
    } = accepted
    else {
        panic!("active preferred steering must be accepted")
    };
    assert_eq!(delivery, AgentRunMessageAcceptedDelivery::Steered);
    let current = store
        .journal_records_after(&id("thread-facade"), None)
        .await
        .expect("runtime journal")
        .records
        .into_iter()
        .filter_map(|record| {
            record.as_presentation().map(|presentation| {
                assert_eq!(presentation.durability, PresentationDurability::Durable);
                serde_json::to_value(&presentation.event).expect("serialize protected body")
            })
        })
        .collect::<Vec<_>>();

    runtime
        .ingest_driver_event(DriverEventEnvelope {
            binding_id: id("binding-facade"),
            generation: RuntimeDriverGeneration(3),
            operation_id: Some(accepted_start.operation_id),
            source_thread_id: id("source-thread-facade"),
            source_turn_id: None,
            source_item_id: None,
            source_request_id: None,
            source_entry_index: None,
            facts: vec![RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                turn_id: snapshot
                    .active_turn_id
                    .clone()
                    .expect("canonical active turn"),
                terminal: RuntimeTurnTerminal::Completed,
                message: None,
                diagnostic: None,
            })],
        })
        .await
        .expect("terminalize active turn");
    let replayed = facade
        .accept_message(steer.clone())
        .await
        .expect("replay after terminal");
    let AgentRunMessageAdmission::Accepted {
        receipt: replayed,
        delivery,
    } = replayed
    else {
        panic!("existing operation must replay after terminal")
    };
    assert_eq!(delivery, AgentRunMessageAcceptedDelivery::Steered);
    assert!(replayed.duplicate);
    assert_eq!(replayed.operation_id, accepted.operation_id);

    let mut conflicting = steer;
    conflicting.presentation.content =
        agentdash_agent_protocol::text_user_input_blocks("conflicting steer");
    assert!(matches!(
        facade.accept_message(conflicting).await,
        Err(AgentRunRuntimeError::ClientCommandConflict)
    ));

    assert_eq!(
        current.len(),
        3,
        "steer must append exactly one presentation fact"
    );
    let main = golden["frames"]
        .as_array()
        .expect("golden frames")
        .iter()
        .map(|frame| {
            let item_nonce = current[2]
                .pointer("/payload/itemId")
                .and_then(serde_json::Value::as_str)
                .and_then(|item_id| item_id.rsplit(':').next())
                .expect("steer item carries a dynamic nonce");
            serde_json::from_str::<serde_json::Value>(
                &frame["notification"]["event"]
                    .to_string()
                    .replace("turn-main-0001", active_presentation_turn_id.as_str())
                    .replace("44444444-4444-4444-4444-444444444444", item_nonce),
            )
            .expect("rewrite symbolic golden identities")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        &current[2..],
        main,
        "steer must preserve the Main body without manufacturing TurnStarted"
    );
}

#[tokio::test]
async fn runtime_facade_modalities_match_main_input_modalities_golden_exactly() {
    use agentdash_agent_protocol::codex_app_server_protocol as codex;

    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/input-modalities.json"
    ))
    .expect("Main input modalities golden");
    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let composition = Arc::new(CompositionFixture::default());
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition,
    );
    let mut command = send("modalities");
    command.presentation_thread_id = id("session-modalities-0001");
    command.presentation.content = vec![
        codex::UserInput::Image {
            detail: Some(None),
            url: "data:image/png;base64,AQID".to_string(),
        },
        codex::UserInput::LocalImage {
            detail: None,
            path: "D:/workspace/reference.png".to_string(),
        },
        codex::UserInput::Skill {
            name: "review".to_string(),
            path: "D:/workspace/.agents/skills/review/SKILL.md".to_string(),
        },
        codex::UserInput::Mention {
            name: "requirements".to_string(),
            path: "D:/workspace/docs/requirements.md".to_string(),
        },
    ];
    facade
        .send_message(command)
        .await
        .expect("send modalities through facade");

    let current = store
        .journal_records_after(&id("thread-facade"), None)
        .await
        .expect("runtime journal")
        .records
        .into_iter()
        .filter_map(|record| {
            record.as_presentation().map(|presentation| {
                assert_eq!(presentation.durability, PresentationDurability::Durable);
                serde_json::to_value(&presentation.event).expect("serialize protected body")
            })
        })
        .collect::<Vec<_>>();
    let actual_turn_id = event_presentation_turn_id(&current[0]);
    assert_eq!(
        current,
        golden["protected_events"]
            .as_array()
            .expect("protected events")
            .iter()
            .map(|event| {
                rewrite_golden_admission_identity(event, "turn-modalities-0001", actual_turn_id)
            })
            .collect::<Vec<_>>(),
        "image/localImage nullable detail and skill/mention payloads must remain byte-semantic"
    );
}

#[tokio::test]
async fn runtime_facade_delivery_sources_match_main_delivery_golden_exactly() {
    async fn capture(
        presentation_thread_id: &str,
        presentation: AgentRunPresentationDraft,
    ) -> Vec<serde_json::Value> {
        let store = Arc::new(RuntimeStoreFixture::default());
        let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
            store.clone(),
            Arc::new(AgentRunRuntimeApplicationPresentationProjector),
        ));
        let composition = Arc::new(CompositionFixture::default());
        let facade = ManagedAgentRunRuntime::new(
            gateway,
            composition.clone(),
            composition.clone(),
            composition.clone(),
            composition,
        );
        let mut command = send("delivery");
        command.presentation_thread_id = id(presentation_thread_id);
        command.presentation = presentation;
        facade
            .send_message(command)
            .await
            .expect("deliver through facade");
        store
            .journal_records_after(&id("thread-facade"), None)
            .await
            .expect("runtime journal")
            .records
            .into_iter()
            .filter_map(|record| {
                record.as_presentation().map(|presentation| {
                    assert_eq!(presentation.durability, PresentationDurability::Durable);
                    serde_json::to_value(&presentation.event).expect("serialize protected body")
                })
            })
            .collect()
    }

    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/session-parity/main-957fa9d/delivery-sources.json"
    ))
    .expect("Main delivery-source golden");
    for (case, thread_id, turn_id) in [
        ("system", "session-system-0001", "turn-system-0001"),
        ("workflow", "session-workflow-0001", "turn-workflow-0001"),
        ("routine", "session-routine-0001", "turn-routine-0001"),
    ] {
        let (launch_source, message) = match case {
            "workflow" => (
                LaunchPresentationSource::WorkflowOrchestrator,
                "workflow continue",
            ),
            "routine" => (LaunchPresentationSource::RoutineExecutor, "routine wake"),
            _ => (LaunchPresentationSource::SystemDelivery, "system wake"),
        };
        let events = capture(
            thread_id,
            AgentRunPresentationDraft {
                content: agentdash_agent_protocol::text_user_input_blocks(message),
                source: agentdash_agent_protocol::UserInputSource::new(
                    "runtime",
                    "system_delivery",
                    "system",
                ),
                launch_source,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            },
        )
        .await;
        assert_eq!(events.len(), 3);
        let expected_first = rewrite_golden_admission_identity(
            &golden["cases"][case]["first_event"],
            turn_id,
            event_presentation_turn_id(&events[0]),
        );
        assert_eq!(events[0], expected_first);
        assert_eq!(events[1]["type"], "turn_started");
        assert_eq!(events[2]["type"], "platform");
        assert_eq!(
            events[2]["payload"]["data"]["frame"]["kind"],
            "system_delivery"
        );
    }

    let companion = capture(
        "session-companion-0001",
        AgentRunPresentationDraft {
            content: agentdash_agent_protocol::text_user_input_blocks("companion dispatch"),
            source: agentdash_agent_protocol::UserInputSource::new(
                "companion",
                "dispatch",
                "agent",
            )
            .with_route("sub"),
            launch_source: LaunchPresentationSource::CompanionDispatch,
            submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        },
    )
    .await;
    assert_eq!(companion.len(), 2);
    let expected_companion = rewrite_golden_admission_identity(
        &golden["cases"]["companion"]["first_event"],
        "turn-companion-0001",
        event_presentation_turn_id(&companion[0]),
    );
    assert_eq!(companion[0], expected_companion);
    assert_eq!(companion[1]["type"], "turn_started");

    let companion_parent_resume = capture(
        "session-companion-marker-0001",
        AgentRunPresentationDraft {
            content: agentdash_agent_protocol::text_user_input_blocks(
                "<subagent_notification>{\"status\":\"completed\"}</subagent_notification>",
            ),
            source: agentdash_agent_protocol::UserInputSource::new(
                "companion",
                "parent_resume",
                "agent",
            ),
            launch_source: LaunchPresentationSource::CompanionParentResume,
            submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        },
    )
    .await;
    assert_eq!(companion_parent_resume.len(), 3);
    let expected_marker = rewrite_golden_admission_identity(
        &golden["cases"]["companion_marker"]["first_event"],
        "turn-companion-marker-0001",
        event_presentation_turn_id(&companion_parent_resume[0]),
    );
    assert_eq!(companion_parent_resume[0], expected_marker);
    assert_eq!(
        companion_parent_resume[0]["payload"]["data"]["value"]["source"]["actor"],
        "agent"
    );
    assert_eq!(companion_parent_resume[1]["type"], "turn_started");
    assert_eq!(
        companion_parent_resume[2]["payload"]["data"]["frame"]["kind"],
        "system_delivery"
    );
}

#[tokio::test]
async fn runtime_facade_append_presentation_uses_bound_runtime_thread_and_replays_idempotently() {
    use agentdash_agent_protocol::{BackboneEvent, PlatformEvent};
    use agentdash_application_agentrun::agent_run::AppendAgentRunPresentation;

    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let composition = Arc::new(CompositionFixture::default());
    let facade = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition,
    );
    facade
        .send_message(send("append-port-bootstrap"))
        .await
        .expect("bootstrap bound runtime");
    let command = AppendAgentRunPresentation {
        target: target(),
        producer: "application:test".into(),
        idempotency_key: id("append-port-idempotency"),
        events: vec![RuntimePresentationInput {
            coordinate: RuntimePresentationCoordinate {
                runtime_turn_id: None,
                presentation_turn_id: None,
                runtime_item_id: None,
                interaction_id: None,
                source_thread_id: Some("session-append-port".into()),
                source_turn_id: Some("turn-append-port".into()),
                source_item_id: None,
                source_request_id: Some("append-port-idempotency".into()),
                source_entry_index: None,
            },
            event: ImmutablePresentationEvent::new(
                PresentationDurability::Durable,
                BackboneEvent::Platform(PlatformEvent::SessionMetaUpdate {
                    key: "append_port_probe".into(),
                    value: serde_json::json!({"status": "recorded"}),
                }),
            ),
        }],
    };
    let accepted = facade
        .append_presentation(command.clone())
        .await
        .expect("append through AgentRun facade");
    assert!(!accepted.duplicate);
    let replayed = facade
        .append_presentation(command)
        .await
        .expect("replay append through AgentRun facade");
    assert!(replayed.duplicate);
    assert_eq!(accepted.first_sequence, replayed.first_sequence);
    assert_eq!(accepted.last_sequence, replayed.last_sequence);
}

#[tokio::test]
async fn first_send_forwards_explicit_backend_selection_to_runtime_provisioning() {
    use agentdash_application_ports::launch::{BackendSelectionInput, BackendSelectionInputMode};

    let store = Arc::new(RuntimeStoreFixture::default());
    let gateway: Arc<dyn AgentRuntimeGateway> = Arc::new(ManagedAgentRuntime::new(
        store,
        Arc::new(AgentRunRuntimeApplicationPresentationProjector),
    ));
    let composition = Arc::new(CompositionFixture::default());
    let runtime = ManagedAgentRunRuntime::new(
        gateway,
        composition.clone(),
        composition.clone(),
        composition.clone(),
        composition.clone(),
    );
    let mut command = send("backend selected");
    command.backend_selection = Some(BackendSelectionInput {
        mode: BackendSelectionInputMode::Explicit,
        backend_id: Some("backend-local".to_string()),
    });

    runtime.send_message(command).await.expect("send succeeds");

    assert_eq!(
        composition.backend_selection.lock().await.clone(),
        Some(BackendSelectionInput {
            mode: BackendSelectionInputMode::Explicit,
            backend_id: Some("backend-local".to_string()),
        })
    );
}
