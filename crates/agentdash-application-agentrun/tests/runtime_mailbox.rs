use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
    sync::Arc,
};

use agentdash_agent_runtime_contract::*;
use agentdash_application_agentrun::agent_run::*;
use agentdash_application_ports::agent_run_runtime::*;
use agentdash_application_ports::workflow_agent_run_delivery::{
    WorkflowAgentRunDeliveryCommand, WorkflowAgentRunDeliveryPort,
};
use agentdash_domain::agent_run_mailbox::{
    AgentRunMailboxRepository, MailboxMessageOrigin, MailboxMessageStatus, MailboxSourceIdentity,
};
use agentdash_test_support::workflow::MemoryAgentRunMailboxRepository;
use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

fn id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid runtime id")
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

fn view(target: AgentRunRuntimeTarget, active_turn: Option<&str>) -> AgentRunRuntimeView {
    let bound_profile = profile();
    let binding = AgentRunRuntimeBinding {
        target: target.clone(),
        presentation_thread_id: id("presentation-mailbox"),
        thread_id: id("thread-mailbox"),
        binding_id: id("binding-mailbox"),
        binding_epoch: agentdash_agent_runtime_contract::BindingEpoch(1),
        driver_generation: RuntimeDriverGeneration(1),
        source_thread_id: id("source-mailbox"),
        profile_digest: id("profile-mailbox"),
        profile_provenance: ProfileProvenance {
            service_digest: id("service-profile"),
            transport_digest: id("transport-profile"),
            host_policy_digest: id("host-profile"),
        },
        bound_profile: bound_profile.clone(),
        surface: RuntimeSurfaceDescriptor {
            source_frame_id: "frame-mailbox".to_string(),
            surface_revision: SurfaceRevision(1),
            surface_digest: id("surface-mailbox"),
            vfs_digest: "vfs-mailbox".to_string(),
            context_recipe_revision: ContextRecipeRevision(1),
            context_digest: id("context-mailbox"),
            settings_revision: ThreadSettingsRevision(0),
            tool_set_revision: ToolSetRevision(0),
            tool_set_digest: "tools-mailbox".to_string(),
            hook_plan: BoundRuntimeHookPlan {
                revision: HookPlanRevision(1),
                digest: id("hook-plan-mailbox"),
                entries: Vec::new(),
            },
            terminal_hook_effect_binding: None,
        },
        settings_revision: ThreadSettingsRevision(0),
    };
    let mut availability = BTreeMap::new();
    availability.insert(
        RuntimeCommandKind::TurnStart,
        if active_turn.is_none() {
            CommandAvailability::Available
        } else {
            CommandAvailability::Unavailable {
                unmet: Vec::new(),
                reason: "active turn".into(),
            }
        },
    );
    availability.insert(
        RuntimeCommandKind::TurnSteer,
        if active_turn.is_some() {
            CommandAvailability::Available
        } else {
            CommandAvailability::Unavailable {
                unmet: Vec::new(),
                reason: "no active turn".into(),
            }
        },
    );
    AgentRunRuntimeView {
        target,
        binding: Some(binding.clone()),
        snapshot: Some(RuntimeSnapshot {
            thread_id: binding.thread_id,
            revision: RuntimeRevision(3),
            latest_event_sequence: EventSequence(3),
            captured_at_ms: 1_783_684_800_000,
            status: RuntimeThreadStatus::Active,
            active_turn_id: active_turn.map(id),
            active_presentation_turn_id: active_turn.map(|_| id("presentation-turn-active")),
            binding_id: binding.binding_id,
            binding_epoch: agentdash_agent_runtime_contract::BindingEpoch(1),
            profile_digest: binding.profile_digest,
            bound_profile,
            active_checkpoint_id: None,
            context_revision: ContextRevision(0),
            settings_revision: ThreadSettingsRevision(0),
            tool_set_revision: ToolSetRevision(0),
            surface: RuntimeSurfaceDescriptor {
                source_frame_id: "frame-mailbox".to_string(),
                surface_revision: SurfaceRevision(1),
                surface_digest: id("surface-mailbox"),
                vfs_digest: "vfs-mailbox".to_string(),
                context_recipe_revision: ContextRecipeRevision(1),
                context_digest: id("context-mailbox"),
                settings_revision: ThreadSettingsRevision(0),
                tool_set_revision: ToolSetRevision(0),
                tool_set_digest: "tools-mailbox".to_string(),
                hook_plan: BoundRuntimeHookPlan {
                    revision: HookPlanRevision(1),
                    digest: id("hook-plan-mailbox"),
                    entries: Vec::new(),
                },
                terminal_hook_effect_binding: None,
            },
            pending_interactions: Vec::new(),
            pending_interaction_details: Vec::new(),
            command_availability: availability,
            transcript: Vec::new(),
            transcript_fidelity: ContextFidelity::Opaque,
        }),
        binding_epoch: Some(agentdash_agent_runtime_contract::BindingEpoch(1)),
        recovery: agentdash_application_agentrun::agent_run::runtime_facade::AgentRunRuntimeRecoverySummary::Active,
    }
}

struct FakeRuntime {
    view: Mutex<AgentRunRuntimeView>,
    sends: Mutex<usize>,
    sent_commands: Mutex<Vec<SendAgentRunMessage>>,
    steers: Mutex<usize>,
    steered_commands: Mutex<Vec<SteerAgentRunTurn>>,
}

#[async_trait]
impl AgentRunRuntime for FakeRuntime {
    async fn append_presentation(
        &self,
        _: AppendAgentRunPresentation,
    ) -> Result<RuntimePresentationAppendReceipt, AgentRunRuntimeError> {
        Err(AgentRunRuntimeError::BindingNotFound)
    }

    async fn inspect(
        &self,
        _target: AgentRunRuntimeTarget,
    ) -> Result<AgentRunRuntimeView, AgentRunRuntimeError> {
        Ok(self.view.lock().await.clone())
    }
    async fn send_message(
        &self,
        command: SendAgentRunMessage,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        *self.sends.lock().await += 1;
        self.sent_commands.lock().await.push(command.clone());
        Ok(OperationReceipt {
            operation_id: id(&format!("operation-{}", command.client_command_id)),
            operation_sequence: OperationSequence(1),
            thread_id: Some(id("thread-mailbox")),
            accepted_revision: RuntimeRevision(4),
            duplicate: false,
        })
    }
    async fn fork_runtime(
        &self,
        _: ForkAgentRunRuntime,
    ) -> Result<AgentRunRuntimeBinding, AgentRunRuntimeError> {
        Err(AgentRunRuntimeError::BindingNotFound)
    }
    async fn compact_context(
        &self,
        _: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        Err(AgentRunRuntimeError::BindingNotFound)
    }
    async fn steer_active_turn(
        &self,
        command: SteerAgentRunTurn,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        *self.steers.lock().await += 1;
        self.steered_commands.lock().await.push(command.clone());
        Ok(OperationReceipt {
            operation_id: id(&format!("operation-{}", command.command.client_command_id)),
            operation_sequence: OperationSequence(1),
            thread_id: Some(id("thread-mailbox")),
            accepted_revision: RuntimeRevision(4),
            duplicate: false,
        })
    }
    async fn interrupt_active_turn(
        &self,
        _: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        Err(AgentRunRuntimeError::BindingNotFound)
    }
    async fn resolve_interaction(
        &self,
        _: ResolveAgentRunInteraction,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        Err(AgentRunRuntimeError::BindingNotFound)
    }
    async fn read_context(
        &self,
        _: AgentRunRuntimeTarget,
    ) -> Result<RuntimeContextView, AgentRunRuntimeError> {
        Err(AgentRunRuntimeError::BindingNotFound)
    }
    async fn read_events(
        &self,
        _: ReadAgentRunEvents,
    ) -> Result<Box<dyn RuntimeEventStream>, AgentRunRuntimeError> {
        Err(AgentRunRuntimeError::BindingNotFound)
    }
}

#[tokio::test]
async fn queued_message_delivers_once_after_canonical_turn_terminal_and_survives_scheduler_restart()
{
    let target = AgentRunRuntimeTarget {
        run_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
    };
    let repository = Arc::new(MemoryAgentRunMailboxRepository::default());
    let runtime = Arc::new(FakeRuntime {
        view: Mutex::new(view(target.clone(), Some("turn-active"))),
        sends: Mutex::new(0),
        sent_commands: Mutex::new(Vec::new()),
        steers: Mutex::new(0),
        steered_commands: Mutex::new(Vec::new()),
    });
    let scheduler = RuntimeAgentRunMailbox::new(repository.clone(), runtime.clone());

    let enqueue = EnqueueRuntimeMailboxMessage {
        target: target.clone(),
        presentation_thread_id: id("presentation-mailbox"),
        presentation: AgentRunPresentationDraft {
            content: agentdash_agent_protocol::text_user_input_blocks("queued"),
            source: agentdash_agent_protocol::UserInputSource::core_composer(),
            launch_source: LaunchPresentationSource::LifecycleAgentUserMessage,
            submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            started_at_seconds: 1_783_684_800,
        },
        client_command_id: "composer-1".into(),
        input: vec![RuntimeInput::Text {
            text: "queued".into(),
        }],
        actor: RuntimeActor::User {
            subject: "user-1".into(),
        },
        identity: None,
        origin: MailboxMessageOrigin::User,
        source: MailboxSourceIdentity::composer(),
        delivery_intent: None,
        executor_config: None,
        backend_selection: None,
    };
    let outcome = scheduler
        .submit(enqueue.clone())
        .await
        .expect("enqueue succeeds");
    assert!(matches!(
        outcome,
        RuntimeMailboxSubmitOutcome::Queued { .. }
    ));
    let RuntimeMailboxSubmitOutcome::Queued { message } = &outcome else {
        unreachable!()
    };
    let persisted_presentation_input: AgentRunPresentationInput = serde_json::from_value(
        message
            .launch_planning_input
            .as_ref()
            .expect("stored runtime mailbox command")["presentation_input"]
            .clone(),
    )
    .expect("stored presentation input");
    let duplicate = scheduler
        .submit(enqueue)
        .await
        .expect("duplicate enqueue succeeds");
    let RuntimeMailboxSubmitOutcome::Queued { message: duplicate } = duplicate else {
        panic!("duplicate remains queued while the turn is active")
    };
    assert_eq!(
        duplicate.launch_planning_input, message.launch_planning_input,
        "duplicate acceptance must retain the first persisted presentation identity"
    );
    assert_eq!(*runtime.sends.lock().await, 0);

    *runtime.view.lock().await = view(target.clone(), None);
    drop(scheduler);
    let restarted = RuntimeAgentRunMailbox::new(repository.clone(), runtime.clone());
    assert_eq!(
        restarted
            .recover_pending_once()
            .await
            .expect("restart recovery discovers and drains the queued target"),
        1
    );
    assert_eq!(*runtime.sends.lock().await, 1);
    let sent_commands = runtime.sent_commands.lock().await;
    assert_eq!(sent_commands.len(), 1);
    assert_eq!(
        sent_commands[0].presentation_thread_id.as_str(),
        "presentation-mailbox"
    );
    let AgentRunPresentationInput::UserSubmission {
        turn_id,
        item_id,
        content,
        ..
    } = &sent_commands[0].presentation_input
    else {
        panic!("user mailbox delivery must stay a user submission")
    };
    assert_eq!(
        content,
        &agentdash_agent_protocol::text_user_input_blocks("queued")
    );
    assert!(
        turn_id.as_str().strip_prefix('t').is_some_and(
            |millis| !millis.is_empty() && millis.chars().all(|ch| ch.is_ascii_digit())
        )
    );
    assert_eq!(item_id.as_str(), format!("{turn_id}:user-input:0"));
    assert_eq!(
        sent_commands[0].presentation_input, persisted_presentation_input,
        "restart delivery must reuse the presentation identity persisted at first acceptance"
    );
    drop(sent_commands);
    let dispatched = repository
        .messages_for(target.run_id, target.agent_id)
        .await;
    assert_eq!(dispatched[0].status, MailboxMessageStatus::Dispatched);
    assert!(
        dispatched[0]
            .accepted_runtime_operation_id
            .as_deref()
            .is_some_and(|operation_id| operation_id.starts_with("operation-mailbox-"))
    );
    assert!(
        restarted
            .recover_and_drain_once(&target)
            .await
            .expect("restart drain succeeds")
            .is_none()
    );
    assert_eq!(*runtime.sends.lock().await, 1);
}

#[tokio::test]
async fn active_turn_steer_is_persisted_before_delivery_and_reuses_presentation_identity() {
    let target = AgentRunRuntimeTarget {
        run_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
    };
    let repository = Arc::new(MemoryAgentRunMailboxRepository::default());
    let runtime = Arc::new(FakeRuntime {
        view: Mutex::new(view(target.clone(), Some("turn-active"))),
        sends: Mutex::new(0),
        sent_commands: Mutex::new(Vec::new()),
        steers: Mutex::new(0),
        steered_commands: Mutex::new(Vec::new()),
    });
    let scheduler = RuntimeAgentRunMailbox::new(repository.clone(), runtime.clone());

    let outcome = scheduler
        .submit(EnqueueRuntimeMailboxMessage {
            target: target.clone(),
            presentation_thread_id: id("presentation-mailbox"),
            presentation: AgentRunPresentationDraft {
                content: agentdash_agent_protocol::text_user_input_blocks("steer now"),
                source: agentdash_agent_protocol::UserInputSource::core_composer(),
                launch_source: LaunchPresentationSource::LifecycleAgentUserMessage,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                started_at_seconds: 1_783_684_800,
            },
            client_command_id: "composer-steer-1".into(),
            input: vec![RuntimeInput::Text {
                text: "steer now".into(),
            }],
            actor: RuntimeActor::User {
                subject: "user-1".into(),
            },
            identity: None,
            origin: MailboxMessageOrigin::User,
            source: MailboxSourceIdentity::composer(),
            delivery_intent: Some("steer".into()),
            executor_config: None,
            backend_selection: None,
        })
        .await
        .expect("steer submit succeeds");

    let (message_id, steered) = match outcome {
        RuntimeMailboxSubmitOutcome::Dispatched {
            message, steered, ..
        } => (message.id, steered),
        RuntimeMailboxSubmitOutcome::Queued { .. } => panic!("steer should dispatch"),
    };
    assert!(steered);
    assert_eq!(*runtime.sends.lock().await, 0);
    assert_eq!(*runtime.steers.lock().await, 1);

    let persisted = repository
        .messages_for(target.run_id, target.agent_id)
        .await;
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].id, message_id);
    let stored_item_id = persisted[0]
        .launch_planning_input
        .as_ref()
        .and_then(|value| value.pointer("/presentation_input/item_id"))
        .and_then(serde_json::Value::as_str)
        .expect("presentation item id is persisted before delivery");
    assert!(stored_item_id.starts_with(&format!(
        "presentation-turn-active:mailbox_steering:scheduler:{message_id}:"
    )));

    let commands = runtime.steered_commands.lock().await;
    assert_eq!(commands.len(), 1);
    let AgentRunPresentationInput::UserSubmission {
        turn_id,
        item_id,
        submission_kind,
        ..
    } = &commands[0].presentation_input
    else {
        panic!("steer must stay a user submission")
    };
    assert_eq!(turn_id.as_str(), "presentation-turn-active");
    assert_eq!(item_id.as_str(), stored_item_id);
    assert_eq!(
        *submission_kind,
        agentdash_agent_protocol::UserInputSubmissionKind::Steer
    );
}

#[tokio::test]
async fn launch_presentation_shape_depends_only_on_typed_source() {
    let marker = "<subagent_notification>{\"status\":\"completed\"}</subagent_notification>";

    for (launch_source, expected_user_submission) in [
        (LaunchPresentationSource::HttpPrompt, true),
        (LaunchPresentationSource::CompanionParentResume, false),
    ] {
        let source_kind = if expected_user_submission {
            "http_prompt"
        } else {
            "companion_parent_resume"
        };
        let target = AgentRunRuntimeTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let repository = Arc::new(MemoryAgentRunMailboxRepository::default());
        let runtime = Arc::new(FakeRuntime {
            view: Mutex::new(view(target.clone(), None)),
            sends: Mutex::new(0),
            sent_commands: Mutex::new(Vec::new()),
            steers: Mutex::new(0),
            steered_commands: Mutex::new(Vec::new()),
        });
        let scheduler = RuntimeAgentRunMailbox::new(repository, runtime.clone());

        scheduler
            .submit(EnqueueRuntimeMailboxMessage {
                target,
                presentation_thread_id: id("presentation-typed-source"),
                presentation: AgentRunPresentationDraft {
                    content: agentdash_agent_protocol::text_user_input_blocks(marker),
                    source: agentdash_agent_protocol::UserInputSource::new(
                        "test",
                        source_kind,
                        if expected_user_submission {
                            "user"
                        } else {
                            "agent"
                        },
                    ),
                    launch_source,
                    submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                    started_at_seconds: 1_783_684_800,
                },
                client_command_id: format!("typed-source-{source_kind}"),
                input: vec![RuntimeInput::Text {
                    text: marker.into(),
                }],
                actor: if expected_user_submission {
                    RuntimeActor::User {
                        subject: "user-1".into(),
                    }
                } else {
                    RuntimeActor::Agent {
                        name: "companion-1".into(),
                    }
                },
                identity: None,
                origin: if expected_user_submission {
                    MailboxMessageOrigin::User
                } else {
                    MailboxMessageOrigin::Companion
                },
                source: MailboxSourceIdentity::composer(),
                delivery_intent: None,
                executor_config: None,
                backend_selection: None,
            })
            .await
            .expect("typed source delivery succeeds");

        let commands = runtime.sent_commands.lock().await;
        assert_eq!(commands.len(), 1);
        assert_eq!(
            matches!(
                commands[0].presentation_input,
                AgentRunPresentationInput::UserSubmission { .. }
            ),
            expected_user_submission,
            "message text must not override the typed launch source"
        );
    }
}

#[tokio::test]
async fn product_delivery_port_uses_product_coordinates_and_canonical_operation_receipt() {
    let target = AgentRunRuntimeTarget {
        run_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
    };
    let repository = Arc::new(MemoryAgentRunMailboxRepository::default());
    let runtime = Arc::new(FakeRuntime {
        view: Mutex::new(view(target.clone(), None)),
        sends: Mutex::new(0),
        sent_commands: Mutex::new(Vec::new()),
        steers: Mutex::new(0),
        steered_commands: Mutex::new(Vec::new()),
    });
    let delivery = RuntimeAgentRunMailbox::new(repository, runtime.clone());

    let result = AgentRunProductDeliveryPort::deliver(
        &delivery,
        DeliverAgentRunProductInput {
            run_id: target.run_id,
            agent_id: target.agent_id,
            presentation_thread_id: id("presentation-product-delivery"),
            presentation: AgentRunPresentationDraft {
                content: agentdash_agent_protocol::text_user_input_blocks("routine wake"),
                source: agentdash_agent_protocol::UserInputSource::new(
                    "routine", "trigger", "system",
                ),
                launch_source: LaunchPresentationSource::RoutineExecutor,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
                started_at_seconds: 1_783_684_800,
            },
            input: vec![RuntimeInput::Text {
                text: "routine wake".into(),
            }],
            actor: RuntimeActor::System {
                component: "routine:test".into(),
            },
            client_command_id: "routine-execution-1".into(),
            backend_selection: None,
            identity: None,
            origin: MailboxMessageOrigin::System,
        },
    )
    .await
    .expect("product delivery succeeds");

    assert!(!result.queued);
    assert!(result.operation_receipt.is_some());
    assert_eq!(*runtime.sends.lock().await, 1);
    let sent_commands = runtime.sent_commands.lock().await;
    let AgentRunPresentationInput::SystemDelivery {
        turn_id,
        launch_source,
        message,
        ..
    } = &sent_commands[0].presentation_input
    else {
        panic!("routine mailbox delivery must be a system projection")
    };
    assert!(
        turn_id.as_str().strip_prefix('t').is_some_and(
            |millis| !millis.is_empty() && millis.chars().all(|ch| ch.is_ascii_digit())
        )
    );
    assert_eq!(*launch_source, LaunchPresentationSource::RoutineExecutor);
    assert_eq!(message, "routine wake");
}

#[tokio::test]
async fn workflow_delivery_port_enters_the_real_runtime_mailbox_as_system_delivery() {
    let target = AgentRunRuntimeTarget {
        run_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
    };
    let repository = Arc::new(MemoryAgentRunMailboxRepository::default());
    let runtime = Arc::new(FakeRuntime {
        view: Mutex::new(view(target.clone(), None)),
        sends: Mutex::new(0),
        sent_commands: Mutex::new(Vec::new()),
        steers: Mutex::new(0),
        steered_commands: Mutex::new(Vec::new()),
    });
    let delivery = RuntimeAgentRunMailbox::new(repository.clone(), runtime.clone());
    let orchestration_id = Uuid::new_v4();

    let receipt = WorkflowAgentRunDeliveryPort::deliver(
        &delivery,
        WorkflowAgentRunDeliveryCommand {
            target: target.clone(),
            presentation_thread_id: id("presentation-workflow-delivery"),
            client_command_id: "workflow-node-1-attempt-1".into(),
            input: vec![RuntimeInput::Text {
                text: "workflow continue".into(),
            }],
            presentation_content: agentdash_agent_protocol::text_user_input_blocks(
                "workflow continue",
            ),
            actor: RuntimeActor::System {
                component: "workflow_orchestrator".into(),
            },
            orchestration_id,
            node_path: "research".into(),
            attempt: 1,
        },
    )
    .await
    .expect("workflow delivery succeeds");

    assert_eq!(*runtime.sends.lock().await, 1);
    let persisted = repository
        .list_messages(target.run_id, target.agent_id)
        .await
        .expect("persisted workflow mailbox message");
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].id, receipt.mailbox_message_id);
    assert_eq!(persisted[0].origin, MailboxMessageOrigin::Workflow);
    assert_eq!(persisted[0].source.namespace, "workflow");
    assert_eq!(persisted[0].source.kind, "orchestrator");
    let expected_source_ref = format!("{orchestration_id}:research#1");
    assert_eq!(
        persisted[0].source.source_ref.as_deref(),
        Some(expected_source_ref.as_str())
    );

    let sent_commands = runtime.sent_commands.lock().await;
    let AgentRunPresentationInput::SystemDelivery {
        launch_source,
        message,
        ..
    } = &sent_commands[0].presentation_input
    else {
        panic!("workflow mailbox delivery must be a system projection")
    };
    assert_eq!(
        *launch_source,
        LaunchPresentationSource::WorkflowOrchestrator
    );
    assert_eq!(message, "workflow continue");
    assert_eq!(
        sent_commands[0].input,
        vec![RuntimeInput::Text {
            text: "workflow continue".into(),
        }]
    );
}
