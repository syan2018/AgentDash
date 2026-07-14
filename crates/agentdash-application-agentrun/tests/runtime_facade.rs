use std::{collections::BTreeSet, str::FromStr, sync::Arc};

use agentdash_agent_runtime::{ManagedAgentRuntime, RuntimeRepository, RuntimeStoreFixture};
use agentdash_agent_runtime_contract::*;
use agentdash_application_agentrun::agent_run::{
    AgentRunCommandGuard, AgentRunPresentationInput, AgentRunRuntime,
    AgentRunRuntimeApplicationPresentationProjector, AgentRunRuntimeError, GuardedAgentRunCommand,
    LaunchPresentationSource, ManagedAgentRunRuntime, SendAgentRunMessage, SteerAgentRunTurn,
};
use agentdash_application_ports::agent_run_runtime::*;
use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

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
        self.turn_start_facts
            .lock()
            .await
            .notices
            .retain(|notice| !notice_ids.contains(&notice.id));
        Ok(())
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
        presentation_input: AgentRunPresentationInput::UserSubmission {
            turn_id: id("turn-facade-0001"),
            item_id: id("turn-facade-0001:user-input:0"),
            content: agentdash_agent_protocol::text_user_input_blocks(text),
            source: agentdash_agent_protocol::UserInputSource::core_composer(),
            submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            started_at_seconds: 1_783_684_800,
        },
        client_command_id: "client-command-1".to_string(),
        input: vec![RuntimeInput::Text {
            text: text.to_string(),
        }],
        actor: RuntimeActor::User {
            subject: "subject-1".to_string(),
        },
        identity: None,
        backend_selection: None,
    }
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
    let main_oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../agentdash-agent-protocol/tests/fixtures/context_frames_main_957fa9d.json"
    ))
    .unwrap();
    let bootstrap_frame: agentdash_agent_protocol::ContextFrame = serde_json::from_value(
        main_oracle["frames"]
            .as_array()
            .unwrap()
            .iter()
            .find(|frame| frame["kind"] == "capability_state_delta")
            .unwrap()
            .clone(),
    )
    .unwrap();
    composition
        .set_bootstrap_frames(vec![bootstrap_frame])
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
    let source_turn_id = "turn-facade-0001".to_string();
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
    assert_eq!(
        serde_json::to_value(
            &presentation[2]
                .as_presentation()
                .expect("context frame presentation")
                .event,
        )
        .unwrap()
        .pointer("/payload/data/frame")
        .cloned()
        .unwrap(),
        main_oracle["frames"]
            .as_array()
            .unwrap()
            .iter()
            .find(|frame| frame["kind"] == "capability_state_delta")
            .unwrap()
            .clone(),
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
                        "startedAt": 1_783_684_800,
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
    let AgentRunPresentationInput::UserSubmission {
        started_at_seconds, ..
    } = &mut presentation_conflict.presentation_input
    else {
        unreachable!()
    };
    *started_at_seconds += 1;
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
async fn turn_start_pending_and_auto_resume_match_main_stream_payload_and_order() {
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
    facade.send_message(send("first")).await.unwrap();
    let snapshot = facade.inspect(target()).await.unwrap().snapshot.unwrap();
    runtime
        .ingest_driver_event(DriverEventEnvelope {
            binding_id: id("binding-facade"),
            generation: RuntimeDriverGeneration(3),
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
                notices: Vec::new(),
            },
        )
        .await;
    let mut second = send("continue");
    second.client_command_id = "client-command-2".to_string();
    second.presentation_input = AgentRunPresentationInput::SystemDelivery {
        turn_id: id("turn-facade-0002"),
        launch_source: LaunchPresentationSource::HookAutoResume,
        message: "continue".to_string(),
        started_at_seconds: 1,
    };
    facade.send_message(second).await.unwrap();
    let mut frames = store
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
    assert_eq!(frames.len(), 2);
    let family_oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../agentdash-agent-protocol/tests/fixtures/context_frames_main_957fa9d.json"
    ))
    .unwrap();
    let pending_oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../agentdash-agent-runtime/tests/fixtures/wi03_pending_action_stream_main_957fa9d.json"
    ))
    .unwrap();
    let expected_frames = [
        family_oracle["frames"]
            .as_array()
            .unwrap()
            .iter()
            .find(|frame| frame["kind"] == "auto_resume")
            .unwrap(),
        &pending_oracle["frame"],
    ];
    for (actual, expected) in frames.iter_mut().zip(expected_frames) {
        actual.id = expected["id"].as_str().unwrap().to_string();
        actual.created_at_ms = expected["created_at_ms"].as_i64().unwrap();
        assert_eq!(serde_json::to_value(actual).unwrap(), *expected);
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
    command.presentation_input = AgentRunPresentationInput::UserSubmission {
        turn_id: id("turn-main-0001"),
        item_id: id("turn-main-0001:user-input:0"),
        content: agentdash_agent_protocol::text_user_input_blocks("hello"),
        source: agentdash_agent_protocol::UserInputSource::core_composer(),
        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        started_at_seconds: 1_783_684_800,
    };

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
    let main = golden["frames"]
        .as_array()
        .expect("golden frames")
        .iter()
        .map(|frame| frame["notification"]["event"].clone())
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
    initial.presentation_input = AgentRunPresentationInput::UserSubmission {
        turn_id: id("turn-main-0001"),
        item_id: id("turn-main-0001:user-input:0"),
        content: agentdash_agent_protocol::text_user_input_blocks("hello"),
        source: agentdash_agent_protocol::UserInputSource::core_composer(),
        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        started_at_seconds: 1_783_684_800,
    };
    facade
        .send_message(initial)
        .await
        .expect("establish active turn");
    let snapshot = facade
        .inspect(target())
        .await
        .expect("inspect active turn")
        .snapshot
        .expect("runtime snapshot");
    assert_eq!(
        snapshot.active_presentation_turn_id,
        Some(id("turn-main-0001"))
    );

    let steer = SteerAgentRunTurn {
        command: GuardedAgentRunCommand {
            target: target(),
            client_command_id: "client-steer-1".to_string(),
            guard: AgentRunCommandGuard {
                thread_id: snapshot.thread_id.clone(),
                expected_revision: snapshot.revision,
                expected_active_turn_id: snapshot.active_turn_id.clone(),
            },
            actor: RuntimeActor::User {
                subject: "subject-1".to_string(),
            },
        },
        presentation_input: AgentRunPresentationInput::UserSubmission {
            turn_id: id("turn-main-0001"),
            item_id: id(
                "turn-main-0001:mailbox_steering:scheduler:33333333-3333-3333-3333-333333333333:44444444-4444-4444-4444-444444444444",
            ),
            content: agentdash_agent_protocol::text_user_input_blocks("steer now"),
            source: agentdash_agent_protocol::UserInputSource::core_composer(),
            submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Steer,
            started_at_seconds: 1_783_684_800,
        },
        input: vec![RuntimeInput::Text {
            text: "steer now".to_string(),
        }],
    };
    let accepted = facade
        .steer_active_turn(steer.clone())
        .await
        .expect("steer active turn");
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
        .steer_active_turn(steer.clone())
        .await
        .expect("replay after terminal");
    assert!(replayed.duplicate);
    assert_eq!(replayed.operation_id, accepted.operation_id);

    let mut conflicting = steer;
    let AgentRunPresentationInput::UserSubmission { item_id, .. } =
        &mut conflicting.presentation_input
    else {
        unreachable!()
    };
    *item_id = id("turn-main-0001:conflicting-steer-item");
    assert!(matches!(
        facade.steer_active_turn(conflicting).await,
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
        .map(|frame| frame["notification"]["event"].clone())
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
    command.presentation_input = AgentRunPresentationInput::UserSubmission {
        turn_id: id("turn-modalities-0001"),
        item_id: id("turn-modalities-0001:user-input:0"),
        content: vec![
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
        ],
        source: agentdash_agent_protocol::UserInputSource::core_composer(),
        submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        started_at_seconds: 1_783_684_800,
    };
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
    assert_eq!(
        current,
        golden["protected_events"]
            .as_array()
            .expect("protected events")
            .clone(),
        "image/localImage nullable detail and skill/mention payloads must remain byte-semantic"
    );
}

#[tokio::test]
async fn runtime_facade_delivery_sources_match_main_delivery_golden_exactly() {
    async fn capture(
        presentation_thread_id: &str,
        presentation_input: AgentRunPresentationInput,
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
        command.presentation_input = presentation_input;
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
            AgentRunPresentationInput::SystemDelivery {
                turn_id: id(turn_id),
                launch_source,
                message: message.into(),
                started_at_seconds: 1_783_684_800,
            },
        )
        .await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], golden["cases"][case]["first_event"]);
        assert_eq!(events[1]["type"], "turn_started");
    }

    let companion = capture(
        "session-companion-0001",
        AgentRunPresentationInput::UserSubmission {
            turn_id: id("turn-companion-0001"),
            item_id: id("turn-companion-0001:user-input:0"),
            content: agentdash_agent_protocol::text_user_input_blocks("companion dispatch"),
            source: agentdash_agent_protocol::UserInputSource::new(
                "companion",
                "dispatch",
                "agent",
            )
            .with_route("sub"),
            submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            started_at_seconds: 1_783_684_800,
        },
    )
    .await;
    assert_eq!(companion.len(), 2);
    assert_eq!(companion[0], golden["cases"]["companion"]["first_event"]);
    assert_eq!(companion[1]["type"], "turn_started");

    let companion_parent_resume = capture(
        "session-companion-marker-0001",
        AgentRunPresentationInput::SystemDelivery {
            turn_id: id("turn-companion-marker-0001"),
            launch_source: LaunchPresentationSource::CompanionParentResume,
            message: "<subagent_notification>{\"status\":\"completed\"}</subagent_notification>"
                .into(),
            started_at_seconds: 1_783_684_800,
        },
    )
    .await;
    assert_eq!(companion_parent_resume.len(), 2);
    assert_eq!(
        companion_parent_resume[0],
        golden["cases"]["companion_marker"]["first_event"]
    );
    assert_eq!(
        companion_parent_resume[0]["payload"]["data"]["value"]["source"]["actor"],
        "agent"
    );
    assert_eq!(companion_parent_resume[1]["type"], "turn_started");
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
