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
    AgentRunMailboxClaimRequest, AgentRunMailboxRepository, ConsumptionBarrier, MailboxDelivery,
    MailboxDrainMode, MailboxMessageOrigin, MailboxMessageStatus, MailboxSourceIdentity,
};
use agentdash_test_support::workflow::{
    MemoryAgentRunMailboxRepository, MemoryAgentRunMessageSubmissionStore,
};
use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

fn id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid runtime id")
}

fn runtime_mailbox(
    repository: Arc<MemoryAgentRunMailboxRepository>,
    runtime: Arc<dyn AgentRunRuntime>,
) -> RuntimeAgentRunMailbox {
    RuntimeAgentRunMailbox::new(
        repository.clone(),
        runtime,
        Arc::new(MemoryAgentRunMessageSubmissionStore::new(repository)),
    )
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
        context_delivery_target:
            agentdash_application_ports::agent_run_runtime::AgentRunContextDeliveryTarget {
                connector_id: "pi-agent".to_string(),
                executor: "PI_AGENT".to_string(),
            },
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
            thread_name: None,
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

fn enqueue_user_message(
    target: AgentRunRuntimeTarget,
    client_command_id: &str,
    text: &str,
) -> EnqueueRuntimeMailboxMessage {
    EnqueueRuntimeMailboxMessage {
        target,
        presentation_thread_id: id("presentation-mailbox"),
        presentation: AgentRunPresentationDraft {
            content: agentdash_agent_protocol::text_user_input_blocks(text),
            source: agentdash_agent_protocol::UserInputSource::core_composer(),
            launch_source: LaunchPresentationSource::LifecycleAgentUserMessage,
            submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        },
        client_command_id: client_command_id.to_string(),
        input: vec![RuntimeInput::text(text.to_string())],
        actor: RuntimeActor::User {
            subject: "user-1".into(),
        },
        identity: None,
        origin: MailboxMessageOrigin::User,
        source: MailboxSourceIdentity::composer(),
        delivery_intent: None,
        executor_config: None,
        backend_selection: None,
    }
}

fn identity(display_name: &str) -> agentdash_spi::AuthIdentity {
    agentdash_spi::AuthIdentity {
        auth_mode: agentdash_spi::AuthMode::Personal,
        user_id: "user-1".to_string(),
        subject: "user-1".to_string(),
        display_name: Some(display_name.to_string()),
        email: None,
        avatar_url: None,
        groups: Vec::new(),
        is_admin: false,
        provider: Some("test".to_string()),
        extra: serde_json::Value::Null,
    }
}

struct FakeRuntime {
    view: Mutex<AgentRunRuntimeView>,
    sends: Mutex<usize>,
    sent_commands: Mutex<Vec<SendAgentRunMessage>>,
    steers: Mutex<usize>,
    steered_commands: Mutex<Vec<SteerAgentRunTurn>>,
    fail_next_steer_stale: Mutex<bool>,
    accept_failure: Mutex<Option<FakeAcceptFailure>>,
}

enum FakeAcceptFailure {
    SnapshotUnavailable,
    PersistenceRetryable,
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
        let mut sent_commands = self.sent_commands.lock().await;
        if let Some(existing) = sent_commands
            .iter()
            .find(|existing| existing.client_command_id == command.client_command_id)
        {
            if existing != &command {
                return Err(AgentRunRuntimeError::ClientCommandConflict);
            }
            return Ok(OperationReceipt {
                operation_id: id(&format!("operation-{}", command.client_command_id)),
                operation_sequence: OperationSequence(1),
                thread_id: Some(id("thread-mailbox")),
                accepted_revision: RuntimeRevision(4),
                duplicate: true,
            });
        }
        *self.sends.lock().await += 1;
        sent_commands.push(command.clone());
        Ok(OperationReceipt {
            operation_id: id(&format!("operation-{}", command.client_command_id)),
            operation_sequence: OperationSequence(1),
            thread_id: Some(id("thread-mailbox")),
            accepted_revision: RuntimeRevision(4),
            duplicate: false,
        })
    }
    async fn accept_message(
        &self,
        command: AcceptAgentRunMessage,
    ) -> Result<AgentRunMessageAdmission, AgentRunRuntimeError> {
        if let Some(failure) = self.accept_failure.lock().await.take() {
            return Err(match failure {
                FakeAcceptFailure::SnapshotUnavailable => {
                    AgentRunRuntimeError::Snapshot(RuntimeSnapshotError::Unavailable {
                        reason: "transient snapshot outage".to_string(),
                    })
                }
                FakeAcceptFailure::PersistenceRetryable => {
                    AgentRunRuntimeError::Execute(RuntimeExecuteError::Persistence {
                        reason: "transient operation persistence outage".to_string(),
                        retryable: true,
                    })
                }
            });
        }
        let view = self.view.lock().await.clone();
        let Some(snapshot) = view.snapshot else {
            let receipt = self
                .send_message(SendAgentRunMessage {
                    target: command.target,
                    presentation_thread_id: command.presentation_thread_id,
                    presentation: command.presentation,
                    client_command_id: command.client_command_id,
                    input: command.input,
                    actor: command.actor,
                    identity: command.identity,
                    backend_selection: command.backend_selection,
                })
                .await?;
            return Ok(AgentRunMessageAdmission::Accepted {
                receipt,
                delivery: AgentRunMessageAcceptedDelivery::Started,
            });
        };
        if snapshot.active_turn_id.is_some() {
            if command.delivery_preference != AgentRunMessageDeliveryPreference::PreferSteer {
                return Ok(AgentRunMessageAdmission::Deferred);
            }
            let receipt = self
                .steer_active_turn(SteerAgentRunTurn {
                    command: GuardedAgentRunCommand {
                        target: command.target,
                        client_command_id: command.client_command_id,
                        guard: AgentRunCommandGuard {
                            thread_id: snapshot.thread_id,
                            expected_revision: snapshot.revision,
                            expected_active_turn_id: snapshot.active_turn_id,
                        },
                        actor: command.actor,
                    },
                    presentation: command.presentation,
                    input: command.input,
                })
                .await?;
            return Ok(AgentRunMessageAdmission::Accepted {
                receipt,
                delivery: AgentRunMessageAcceptedDelivery::Steered,
            });
        }
        let receipt = self
            .send_message(SendAgentRunMessage {
                target: command.target,
                presentation_thread_id: command.presentation_thread_id,
                presentation: command.presentation,
                client_command_id: command.client_command_id,
                input: command.input,
                actor: command.actor,
                identity: command.identity,
                backend_selection: command.backend_selection,
            })
            .await?;
        Ok(AgentRunMessageAdmission::Accepted {
            receipt,
            delivery: AgentRunMessageAcceptedDelivery::Started,
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
        let mut steered_commands = self.steered_commands.lock().await;
        if let Some(existing) = steered_commands.iter().find(|existing| {
            existing.command.client_command_id == command.command.client_command_id
        }) {
            if existing != &command {
                return Err(AgentRunRuntimeError::ClientCommandConflict);
            }
            return Ok(OperationReceipt {
                operation_id: id(&format!("operation-{}", command.command.client_command_id)),
                operation_sequence: OperationSequence(2),
                thread_id: Some(id("thread-mailbox")),
                accepted_revision: RuntimeRevision(4),
                duplicate: true,
            });
        }
        *self.steers.lock().await += 1;
        steered_commands.push(command.clone());
        let mut fail_next = self.fail_next_steer_stale.lock().await;
        if *fail_next {
            *fail_next = false;
            let target = command.command.target.clone();
            *self.view.lock().await = view(target, None);
            return Err(AgentRunRuntimeError::StaleActiveTurn);
        }
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let scheduler = runtime_mailbox(repository.clone(), runtime.clone());

    let enqueue = EnqueueRuntimeMailboxMessage {
        target: target.clone(),
        presentation_thread_id: id("presentation-mailbox"),
        presentation: AgentRunPresentationDraft {
            content: agentdash_agent_protocol::text_user_input_blocks("queued"),
            source: agentdash_agent_protocol::UserInputSource::core_composer(),
            launch_source: LaunchPresentationSource::LifecycleAgentUserMessage,
            submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
        },
        client_command_id: "composer-1".into(),
        input: vec![RuntimeInput::text("queued")],
        actor: RuntimeActor::User {
            subject: "user-1".into(),
        },
        identity: Some(identity("Before profile refresh")),
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
    let persisted_presentation: AgentRunPresentationDraft = serde_json::from_value(
        message
            .launch_planning_input
            .as_ref()
            .expect("stored runtime mailbox command")["presentation"]
            .clone(),
    )
    .expect("stored presentation draft");
    let stored = message
        .launch_planning_input
        .as_ref()
        .expect("stored runtime mailbox command");
    assert!(stored.pointer("/presentation/turn_id").is_none());
    assert!(stored.pointer("/presentation/item_id").is_none());
    let mut retry_after_profile_refresh = enqueue;
    retry_after_profile_refresh
        .identity
        .as_mut()
        .expect("test identity")
        .display_name = Some("After profile refresh".to_string());
    let duplicate = scheduler
        .submit(retry_after_profile_refresh)
        .await
        .expect("duplicate enqueue succeeds");
    let RuntimeMailboxSubmitOutcome::Queued { message: duplicate } = duplicate else {
        panic!("duplicate remains queued while the turn is active")
    };
    assert_eq!(
        duplicate.id, message.id,
        "independent prepare/submit retries must reuse the first mailbox row"
    );
    assert_eq!(
        duplicate.launch_planning_input, message.launch_planning_input,
        "duplicate acceptance must retain the first persisted presentation draft"
    );
    let different_draft = scheduler
        .submit(enqueue_user_message(
            target.clone(),
            "composer-1",
            "different queued draft",
        ))
        .await;
    assert!(matches!(
        different_draft,
        Err(RuntimeMailboxError::Persistence(
            agentdash_domain::DomainError::Conflict { .. }
        ))
    ));
    assert_eq!(*runtime.sends.lock().await, 0);

    *runtime.view.lock().await = view(target.clone(), None);
    drop(scheduler);
    let restarted = runtime_mailbox(repository.clone(), runtime.clone());
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
    assert_eq!(
        sent_commands[0].presentation.content,
        agentdash_agent_protocol::text_user_input_blocks("queued")
    );
    assert_eq!(
        sent_commands[0].presentation, persisted_presentation,
        "restart delivery must reuse the typed draft persisted at first acceptance"
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
async fn active_turn_steer_lets_runtime_own_identity_and_clears_user_draft_after_acceptance() {
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let scheduler = runtime_mailbox(repository.clone(), runtime.clone());

    let outcome = scheduler
        .submit(EnqueueRuntimeMailboxMessage {
            target: target.clone(),
            presentation_thread_id: id("presentation-mailbox"),
            presentation: AgentRunPresentationDraft {
                content: agentdash_agent_protocol::text_user_input_blocks("steer now"),
                source: agentdash_agent_protocol::UserInputSource::core_composer(),
                launch_source: LaunchPresentationSource::LifecycleAgentUserMessage,
                submission_kind: agentdash_agent_protocol::UserInputSubmissionKind::Prompt,
            },
            client_command_id: "composer-steer-1".into(),
            input: vec![RuntimeInput::text("steer now")],
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
    assert!(persisted[0].launch_planning_input.is_none());
    assert!(persisted[0].payload_json.is_none());

    let commands = runtime.steered_commands.lock().await;
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0].presentation.submission_kind,
        agentdash_agent_protocol::UserInputSubmissionKind::Prompt
    );
    assert_eq!(
        commands[0].presentation.content,
        agentdash_agent_protocol::text_user_input_blocks("steer now")
    );
}

#[tokio::test]
async fn promote_claims_active_turn_as_steer_without_rewriting_the_draft() {
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let mailbox = runtime_mailbox(repository.clone(), runtime.clone());
    let RuntimeMailboxSubmitOutcome::Queued { message } = mailbox
        .submit(enqueue_user_message(
            target.clone(),
            "promote-active",
            "now",
        ))
        .await
        .expect("active turn queues ordinary delivery")
    else {
        panic!("ordinary active-turn delivery must queue")
    };
    let original_draft = message.launch_planning_input.clone();
    let promoted = mailbox
        .promote(&target, message.id)
        .await
        .expect("queued user message promotes atomically");
    assert_eq!(promoted.launch_planning_input, original_draft);

    let (_, _, steered) = mailbox
        .drain_once(&target)
        .await
        .expect("active promote drains")
        .expect("promoted message is claimable");
    assert!(steered);
    assert_eq!(*runtime.steers.lock().await, 1);
    assert_eq!(*runtime.sends.lock().await, 0);
}

#[tokio::test]
async fn promote_rejects_non_user_delivery_before_it_can_enter_active_steer_claiming() {
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let mailbox = runtime_mailbox(repository, runtime);
    let mut command = enqueue_user_message(target.clone(), "system-promote", "system");
    command.origin = MailboxMessageOrigin::System;
    command.presentation.launch_source = LaunchPresentationSource::SystemDelivery;
    let RuntimeMailboxSubmitOutcome::Queued { message } = mailbox
        .submit(command)
        .await
        .expect("system delivery queues behind active turn")
    else {
        panic!("system delivery must queue")
    };
    assert!(matches!(
        mailbox.promote(&target, message.id).await,
        Err(RuntimeMailboxError::Persistence(
            agentdash_domain::DomainError::Conflict { .. }
        ))
    ));
}

#[tokio::test]
async fn promote_changes_paused_message_policy_without_resuming_delivery() {
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let mailbox = runtime_mailbox(repository.clone(), runtime.clone());
    let RuntimeMailboxSubmitOutcome::Queued { message } = mailbox
        .submit(enqueue_user_message(
            target.clone(),
            "paused-promote",
            "later",
        ))
        .await
        .expect("message queues")
    else {
        panic!("message must queue")
    };
    repository
        .mark_message_status(
            message.id,
            None,
            MailboxMessageStatus::Paused,
            Some("manual pause".to_string()),
        )
        .await
        .expect("pause message");
    let promoted = mailbox
        .promote(&target, message.id)
        .await
        .expect("paused policy may be promoted");
    assert_eq!(promoted.status, MailboxMessageStatus::Paused);
    assert_eq!(promoted.last_error.as_deref(), Some("manual pause"));
    assert!(matches!(
        promoted.delivery,
        agentdash_domain::agent_run_mailbox::MailboxDelivery::SteerActiveTurn { .. }
    ));
    assert!(
        mailbox
            .drain_once(&target)
            .await
            .expect("paused drain inspection succeeds")
            .is_none()
    );
    assert_eq!(*runtime.sends.lock().await, 0);
    assert_eq!(*runtime.steers.lock().await, 0);
}

#[tokio::test]
async fn promoted_message_starts_the_next_turn_when_terminal_precedes_claim() {
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let mailbox = runtime_mailbox(repository, runtime.clone());
    let RuntimeMailboxSubmitOutcome::Queued { message } = mailbox
        .submit(enqueue_user_message(
            target.clone(),
            "promote-terminal",
            "next",
        ))
        .await
        .expect("message queues")
    else {
        panic!("message must queue")
    };
    mailbox
        .promote(&target, message.id)
        .await
        .expect("message promotes");
    *runtime.view.lock().await = view(target.clone(), None);

    let (_, _, steered) = mailbox
        .drain_once(&target)
        .await
        .expect("idle drain succeeds")
        .expect("promoted AgentLoop message remains claimable when idle");
    assert!(!steered);
    assert_eq!(*runtime.sends.lock().await, 1);
    assert_eq!(*runtime.steers.lock().await, 0);
}

#[tokio::test]
async fn stale_steer_attempt_requeues_and_replans_as_next_turn_start() {
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
        fail_next_steer_stale: Mutex::new(true),
        accept_failure: Mutex::new(None),
    });
    let mailbox = runtime_mailbox(repository.clone(), runtime.clone());
    let RuntimeMailboxSubmitOutcome::Queued { message } = mailbox
        .submit(enqueue_user_message(target.clone(), "promote-race", "race"))
        .await
        .expect("message queues")
    else {
        panic!("message must queue")
    };
    mailbox
        .promote(&target, message.id)
        .await
        .expect("message promotes");

    assert!(
        mailbox
            .drain_once(&target)
            .await
            .expect("stale attempt is a replan signal")
            .is_none()
    );
    let queued = repository
        .get_message(message.id)
        .await
        .expect("repository read")
        .expect("message remains");
    assert_eq!(queued.status, MailboxMessageStatus::Queued);
    assert!(queued.claim_token.is_none());

    let (_, _, steered) = mailbox
        .drain_once(&target)
        .await
        .expect("replanned attempt succeeds")
        .expect("same draft remains deliverable");
    assert!(!steered);
    assert_eq!(*runtime.steers.lock().await, 1);
    assert_eq!(*runtime.sends.lock().await, 1);
    let delivered = repository
        .get_message(message.id)
        .await
        .expect("repository read")
        .expect("message remains");
    assert_eq!(delivered.status, MailboxMessageStatus::Dispatched);
    assert_eq!(delivered.attempt_count, 2);
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
            fail_next_steer_stale: Mutex::new(false),
            accept_failure: Mutex::new(None),
        });
        let scheduler = runtime_mailbox(repository, runtime.clone());

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
                },
                client_command_id: format!("typed-source-{source_kind}"),
                input: vec![RuntimeInput::text(marker)],
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
                commands[0].presentation.launch_source,
                LaunchPresentationSource::HttpPrompt
                    | LaunchPresentationSource::LifecycleAgentUserMessage
                    | LaunchPresentationSource::CompanionDispatch
                    | LaunchPresentationSource::LocalRelayPrompt
            ),
            expected_user_submission,
            "message text must not override the typed launch source"
        );
    }
}

#[tokio::test]
async fn explicit_steer_policy_survives_idle_to_active_admission_race() {
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let mailbox = runtime_mailbox(repository.clone(), runtime.clone());
    let mut command = enqueue_user_message(target.clone(), "idle-active-steer", "keep policy");
    command.delivery_intent = Some("steer".to_string());
    let execution_profile_override = agentdash_spi::AgentConfig::new("PI_AGENT");
    command.executor_config = Some(execution_profile_override.clone());
    let visible_content = command.presentation.content.clone();
    let draft = mailbox
        .prepare_message(command)
        .expect("prepare mailbox draft");
    assert!(matches!(
        draft.delivery,
        MailboxDelivery::SteerActiveTurn { .. }
    ));
    assert_eq!(draft.barrier, ConsumptionBarrier::AgentLoopTurnBoundary);
    assert_eq!(draft.drain_mode, MailboxDrainMode::All);
    assert_eq!(
        draft.payload_json,
        Some(serde_json::to_value(visible_content).expect("visible presentation payload"))
    );
    assert!(!draft.retain_payload);
    let expected_profile = serde_json::to_value(execution_profile_override).expect("typed profile");
    assert_eq!(
        draft
            .launch_planning_input
            .as_ref()
            .and_then(|value| value.get("execution_profile_override")),
        Some(&expected_profile)
    );
    repository
        .create_message(draft)
        .await
        .expect("persist steer draft");

    *runtime.view.lock().await = view(target.clone(), Some("turn-became-active"));
    let (_, _, steered) = mailbox
        .drain_once(&target)
        .await
        .expect("deliver after active transition")
        .expect("claimed explicit steer");
    assert!(steered);
    assert_eq!(*runtime.steers.lock().await, 1);
    assert_eq!(*runtime.sends.lock().await, 0);
}

#[tokio::test]
async fn retryable_reconciliation_errors_preserve_semantic_reconcile_flag() {
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let mailbox = runtime_mailbox(repository.clone(), runtime.clone());
    let draft = mailbox
        .prepare_message(enqueue_user_message(
            target.clone(),
            "reconcile-retry",
            "reconcile me",
        ))
        .expect("prepare mailbox draft");
    let message = repository
        .create_message(draft)
        .await
        .expect("persist mailbox draft");
    repository
        .claim_next(AgentRunMailboxClaimRequest {
            run_id: target.run_id,
            agent_id: target.agent_id,
            barriers: vec![ConsumptionBarrier::ImmediateIfIdle],
            drain_mode: Some(MailboxDrainMode::One),
            limit: 1,
            claim_token: Uuid::new_v4(),
            claim_expires_at: chrono::Utc::now() - chrono::Duration::seconds(1),
        })
        .await
        .expect("claim before simulated scheduler loss");
    repository
        .recover_expired_consuming(chrono::Utc::now())
        .await
        .expect("recover expired claim");

    *runtime.accept_failure.lock().await = Some(FakeAcceptFailure::SnapshotUnavailable);
    assert!(
        mailbox
            .drain_once(&target)
            .await
            .expect("snapshot outage is retryable")
            .is_none()
    );
    let after_snapshot = repository
        .get_message(message.id)
        .await
        .expect("load mailbox message")
        .expect("message exists");
    assert_eq!(after_snapshot.status, MailboxMessageStatus::Queued);
    assert!(after_snapshot.reconcile_required);

    *runtime.accept_failure.lock().await = Some(FakeAcceptFailure::PersistenceRetryable);
    assert!(
        mailbox
            .drain_once(&target)
            .await
            .expect("persistence outage is retryable")
            .is_none()
    );
    let after_persistence = repository
        .get_message(message.id)
        .await
        .expect("load mailbox message")
        .expect("message exists");
    assert_eq!(after_persistence.status, MailboxMessageStatus::Queued);
    assert!(after_persistence.reconcile_required);
}

#[tokio::test]
async fn direct_producer_duplicate_replays_terminal_delivery_and_rejects_changed_draft() {
    for (case, active_turn, prefer_steer, expected_status) in [
        ("started", None, false, MailboxMessageStatus::Dispatched),
        (
            "steered",
            Some("turn-active"),
            true,
            MailboxMessageStatus::Steered,
        ),
    ] {
        let target = AgentRunRuntimeTarget {
            run_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
        };
        let repository = Arc::new(MemoryAgentRunMailboxRepository::default());
        let runtime = Arc::new(FakeRuntime {
            view: Mutex::new(view(target.clone(), active_turn)),
            sends: Mutex::new(0),
            sent_commands: Mutex::new(Vec::new()),
            steers: Mutex::new(0),
            steered_commands: Mutex::new(Vec::new()),
            fail_next_steer_stale: Mutex::new(false),
            accept_failure: Mutex::new(None),
        });
        let mailbox = runtime_mailbox(repository.clone(), runtime.clone());
        let mut command = enqueue_user_message(
            target.clone(),
            &format!("terminal-duplicate-{case}"),
            "same request",
        );
        if prefer_steer {
            command.delivery_intent = Some("steer".to_string());
        }

        let RuntimeMailboxSubmitOutcome::Dispatched {
            message: first_message,
            receipt: first_receipt,
            ..
        } = mailbox
            .submit(command.clone())
            .await
            .expect("first delivery")
        else {
            panic!("first delivery must settle");
        };
        assert_eq!(first_message.status, expected_status);
        assert!(!first_receipt.duplicate);
        assert!(first_message.launch_planning_input.is_none());

        let RuntimeMailboxSubmitOutcome::Dispatched {
            message: replayed_message,
            receipt: replayed_receipt,
            ..
        } = mailbox
            .submit(command.clone())
            .await
            .expect("same semantic request replays")
        else {
            panic!("terminal duplicate must replay accepted delivery");
        };
        assert_eq!(replayed_message.id, first_message.id);
        assert_eq!(replayed_message.status, expected_status);
        assert_eq!(replayed_receipt.operation_id, first_receipt.operation_id);
        assert_eq!(
            replayed_message.accepted_runtime_operation_id.as_deref(),
            Some(replayed_receipt.operation_id.as_str())
        );
        assert!(replayed_receipt.duplicate);
        assert_eq!(
            *runtime.sends.lock().await + *runtime.steers.lock().await,
            1,
            "duplicate must not create a second Runtime operation"
        );

        let mut conflicting = command;
        conflicting.presentation.content =
            agentdash_agent_protocol::text_user_input_blocks("changed request");
        conflicting.input = vec![RuntimeInput::text("changed request".to_string())];
        assert!(matches!(
            mailbox.submit(conflicting).await,
            Err(RuntimeMailboxError::Persistence(
                agentdash_domain::DomainError::Conflict { .. }
            ))
        ));
    }
}

#[tokio::test]
async fn paused_target_keeps_new_message_queued_until_resume() {
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let mailbox = runtime_mailbox(repository.clone(), runtime.clone());
    repository
        .pause_state(
            target.run_id,
            target.agent_id,
            "manual pause".to_string(),
            None,
        )
        .await
        .expect("pause target");
    let RuntimeMailboxSubmitOutcome::Queued { message } = mailbox
        .submit(enqueue_user_message(
            target.clone(),
            "pause-new-message",
            "wait for resume",
        ))
        .await
        .expect("paused submission remains durable")
    else {
        panic!("paused target must not dispatch a newly admitted message");
    };
    assert_eq!(*runtime.sends.lock().await, 0);
    assert_eq!(message.status, MailboxMessageStatus::Queued);

    repository
        .resume_state(target.run_id, target.agent_id)
        .await
        .expect("resume target");
    assert!(
        mailbox
            .drain_once(&target)
            .await
            .expect("drain after resume")
            .is_some()
    );
    assert_eq!(*runtime.sends.lock().await, 1);
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let delivery = runtime_mailbox(repository, runtime.clone());

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
            },
            input: vec![RuntimeInput::text("routine wake")],
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
    assert_eq!(
        sent_commands[0].presentation.launch_source,
        LaunchPresentationSource::RoutineExecutor
    );
    assert_eq!(
        sent_commands[0].presentation.content,
        agentdash_agent_protocol::text_user_input_blocks("routine wake")
    );
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
        fail_next_steer_stale: Mutex::new(false),
        accept_failure: Mutex::new(None),
    });
    let delivery = runtime_mailbox(repository.clone(), runtime.clone());
    let orchestration_id = Uuid::new_v4();

    let receipt = WorkflowAgentRunDeliveryPort::deliver(
        &delivery,
        WorkflowAgentRunDeliveryCommand {
            target: target.clone(),
            presentation_thread_id: id("presentation-workflow-delivery"),
            client_command_id: "workflow-node-1-attempt-1".into(),
            input: vec![RuntimeInput::text("workflow continue")],
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
    assert_eq!(
        sent_commands[0].presentation.launch_source,
        LaunchPresentationSource::WorkflowOrchestrator
    );
    assert_eq!(
        sent_commands[0].presentation.content,
        agentdash_agent_protocol::text_user_input_blocks("workflow continue")
    );
    assert_eq!(
        sent_commands[0].input,
        vec![RuntimeInput::text("workflow continue")]
    );
}
