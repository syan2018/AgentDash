use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
    sync::Arc,
};

use agentdash_agent_runtime_contract::*;
use agentdash_application_agentrun::agent_run::*;
use agentdash_application_ports::agent_run_runtime::*;
use agentdash_domain::agent_run_mailbox::{MailboxMessageStatus, MailboxSourceIdentity};
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
        thread_id: id("thread-mailbox"),
        binding_id: id("binding-mailbox"),
        driver_generation: RuntimeDriverGeneration(1),
        source_thread_id: id("source-mailbox"),
        profile_digest: id("profile-mailbox"),
        profile_provenance: ProfileProvenance {
            service_digest: id("service-profile"),
            transport_digest: id("transport-profile"),
            host_policy_digest: id("host-profile"),
        },
        bound_profile: bound_profile.clone(),
        surface_digest: id("surface-mailbox"),
        settings_revision: ThreadSettingsRevision(0),
        tool_set_revision: ToolSetRevision(0),
        hook_plan: BoundRuntimeHookPlan {
            revision: HookPlanRevision(1),
            digest: id("hook-plan-mailbox"),
            entries: Vec::new(),
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
    AgentRunRuntimeView {
        target,
        binding: Some(binding.clone()),
        snapshot: Some(RuntimeSnapshot {
            thread_id: binding.thread_id,
            revision: RuntimeRevision(3),
            status: RuntimeThreadStatus::Active,
            active_turn_id: active_turn.map(id),
            binding_id: binding.binding_id,
            profile_digest: binding.profile_digest,
            bound_profile,
            active_checkpoint_id: None,
            context_revision: ContextRevision(0),
            settings_revision: ThreadSettingsRevision(0),
            tool_set_revision: ToolSetRevision(0),
            pending_interactions: Vec::new(),
            command_availability: availability,
            transcript: Vec::new(),
            transcript_fidelity: ContextFidelity::Opaque,
        }),
    }
}

struct FakeRuntime {
    view: Mutex<AgentRunRuntimeView>,
    sends: Mutex<usize>,
}

#[async_trait]
impl AgentRunRuntime for FakeRuntime {
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
        Ok(OperationReceipt {
            operation_id: id(&format!("operation-{}", command.client_command_id)),
            operation_sequence: OperationSequence(1),
            thread_id: Some(id("thread-mailbox")),
            accepted_revision: RuntimeRevision(4),
            duplicate: false,
        })
    }
    async fn compact_context(
        &self,
        _: GuardedAgentRunCommand,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        Err(AgentRunRuntimeError::BindingNotFound)
    }
    async fn steer_active_turn(
        &self,
        _: SteerAgentRunTurn,
    ) -> Result<OperationReceipt, AgentRunRuntimeError> {
        Err(AgentRunRuntimeError::BindingNotFound)
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
    });
    let scheduler = RuntimeAgentRunMailbox::new(repository.clone(), runtime.clone());

    let outcome = scheduler
        .submit(EnqueueRuntimeMailboxMessage {
            target: target.clone(),
            client_command_id: "composer-1".into(),
            input: vec![RuntimeInput::Text {
                text: "queued".into(),
            }],
            actor: RuntimeActor::User {
                subject: "user-1".into(),
            },
            identity: None,
            source: MailboxSourceIdentity::composer(),
            backend_selection: None,
        })
        .await
        .expect("enqueue succeeds");
    assert!(matches!(
        outcome,
        RuntimeMailboxSubmitOutcome::Queued { .. }
    ));
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
async fn product_delivery_port_uses_product_coordinates_and_canonical_operation_receipt() {
    let target = AgentRunRuntimeTarget {
        run_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
    };
    let repository = Arc::new(MemoryAgentRunMailboxRepository::default());
    let runtime = Arc::new(FakeRuntime {
        view: Mutex::new(view(target.clone(), None)),
        sends: Mutex::new(0),
    });
    let delivery = RuntimeAgentRunMailbox::new(repository, runtime.clone());

    let result = AgentRunProductDeliveryPort::deliver(
        &delivery,
        DeliverAgentRunProductInput {
            run_id: target.run_id,
            agent_id: target.agent_id,
            input: vec![RuntimeInput::Text {
                text: "routine wake".into(),
            }],
            actor: RuntimeActor::System {
                component: "routine:test".into(),
            },
            client_command_id: "routine-execution-1".into(),
            backend_selection: None,
            identity: None,
        },
    )
    .await
    .expect("product delivery succeeds");

    assert!(!result.queued);
    assert!(result.operation_receipt.is_some());
    assert_eq!(*runtime.sends.lock().await, 1);
}
