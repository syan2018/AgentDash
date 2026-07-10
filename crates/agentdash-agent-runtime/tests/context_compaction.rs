use std::{collections::BTreeSet, str::FromStr, sync::Arc};

use agentdash_agent_runtime::{
    ActivationObservation, CommitFailurePoint, CompactionPreparation, ContextActivationStatus,
    ContextRuntimeError, InMemoryRuntimeStore, ManagedAgentRuntime, RuntimeCommit,
    RuntimeKernelDefaults, RuntimeRepository, RuntimeUnitOfWork,
};
use agentdash_agent_runtime_contract::*;

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
            modalities: BTreeSet::new(),
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
        .into_iter()
        .collect(),
        hooks: HookProfile {
            points: Vec::new(),
            configuration_boundary: ConfigurationBoundary::Binding,
        },
        context: ContextProfile {
            capabilities: [
                ContextCapability::Read,
                ContextCapability::PrepareCompaction,
                ContextCapability::ActivateCheckpoint,
            ]
            .into_iter()
            .collect(),
            fidelity: ContextFidelity::PlatformExact,
            activation_idempotent: true,
        },
        telemetry_config: BTreeSet::new(),
    }
}

fn fixture() -> (
    Arc<InMemoryRuntimeStore>,
    ManagedAgentRuntime<InMemoryRuntimeStore>,
) {
    let store = Arc::new(InMemoryRuntimeStore::default());
    let runtime = ManagedAgentRuntime::new(
        store.clone(),
        RuntimeKernelDefaults {
            binding_id: id("binding-1"),
            driver_generation: RuntimeDriverGeneration(4),
            source_thread_id: id("source-1"),
            profile_digest: id("profile-1"),
            bound_profile: profile(),
        },
    );
    (store, runtime)
}

fn envelope(operation: &str, key: &str, command: RuntimeCommand) -> RuntimeCommandEnvelope {
    RuntimeCommandEnvelope {
        meta: OperationMeta {
            operation_id: id(operation),
            idempotency_key: id(key),
            expected_thread_revision: None,
            actor: RuntimeActor::System {
                component: "context-runtime-test".to_string(),
            },
        },
        command,
    }
}

async fn start_and_accept_compaction(
    runtime: &ManagedAgentRuntime<InMemoryRuntimeStore>,
    operation: &str,
    trigger: ContextCompactionTrigger,
) -> RuntimeThreadId {
    let thread_id = runtime
        .execute(envelope(
            "thread-start",
            "thread-key",
            RuntimeCommand::ThreadStart {
                input: Vec::new(),
                surface_digest: id("surface-1"),
            },
        ))
        .await
        .expect("start")
        .thread_id
        .expect("thread id");
    runtime
        .execute(envelope(
            operation,
            &format!("{operation}-key"),
            RuntimeCommand::ContextCompact {
                thread_id: thread_id.clone(),
                compaction_id: id(operation),
                trigger,
                base_checkpoint_id: None,
                expected_context_revision: ContextRevision(0),
            },
        ))
        .await
        .expect("compact accepted");
    thread_id
}

fn preparation(
    thread_id: RuntimeThreadId,
    operation: &str,
    suffix: &str,
    trigger: ContextCompactionTrigger,
) -> CompactionPreparation {
    CompactionPreparation {
        candidate_id: id(&format!("candidate-{suffix}")),
        compaction_id: id(operation),
        activation_id: id(&format!("stable-activation-{suffix}")),
        operation_id: id(operation),
        thread_id,
        trigger,
        expected_base_checkpoint_id: None,
        expected_base_revision: ContextRevision(0),
        checkpoint_id: id(&format!("checkpoint-{suffix}")),
        materialized: MaterializedContext {
            recipe: ContextRecipe {
                revision: ContextRecipeRevision(9),
                provenance: ContextProvenance {
                    settings_revision: ThreadSettingsRevision(0),
                    tool_set_revision: ToolSetRevision(0),
                },
                source_item_ids: Vec::new(),
            },
            blocks: vec![ContextBlock::CompactionSummary {
                summary: format!("summary-{suffix}"),
            }],
            digest: id(&format!("digest-{suffix}")),
            fidelity: ContextFidelity::PlatformExact,
        },
    }
}

#[tokio::test]
async fn prepare_is_atomic_and_does_not_change_the_active_head() {
    let (store, runtime) = fixture();
    let thread_id =
        start_and_accept_compaction(&runtime, "compact-1", ContextCompactionTrigger::Manual).await;
    let prepare = preparation(
        thread_id.clone(),
        "compact-1",
        "manual",
        ContextCompactionTrigger::Manual,
    );
    let pending = store
        .pending_context_preparations()
        .await
        .expect("durable preparation discovery");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].operation_id, prepare.operation_id);
    assert_eq!(pending[0].compaction_id, prepare.compaction_id);
    store.fail_next_commit_at(CommitFailurePoint::AfterContext);
    assert!(matches!(
        runtime.prepare_compaction(prepare.clone()).await,
        Err(ContextRuntimeError::Store(_))
    ));
    assert!(
        store
            .load_context_candidate(&prepare.compaction_id)
            .await
            .expect("candidate")
            .is_none()
    );
    assert!(store.context_activation_outbox().await.is_empty());

    runtime
        .prepare_compaction(prepare.clone())
        .await
        .expect("prepare");
    assert!(
        store
            .pending_context_preparations()
            .await
            .expect("pending preparations")
            .is_empty()
    );
    assert!(
        store
            .load_context_head(&thread_id)
            .await
            .expect("head")
            .is_none()
    );
    assert_eq!(store.context_activation_outbox().await.len(), 1);
    assert_eq!(
        store
            .pending_context_activations()
            .await
            .expect("pending activations")
            .len(),
        1
    );
    assert_eq!(
        store.context_activation_outbox().await[0].activation_id,
        prepare.activation_id
    );
    runtime
        .recover_compaction(&prepare.compaction_id, ActivationObservation::NotApplied)
        .await
        .expect("requeue activation");
    let activation_outbox = store.context_activation_outbox().await;
    assert_eq!(activation_outbox.len(), 1);
    assert!(
        activation_outbox
            .iter()
            .all(|entry| entry.activation_id == prepare.activation_id)
    );
    assert!(
        store
            .load_context_head(&thread_id)
            .await
            .expect("head")
            .is_none()
    );
}

#[tokio::test]
async fn compaction_acceptance_and_recovery_work_are_atomic() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(envelope(
            "thread-start",
            "thread-key",
            RuntimeCommand::ThreadStart {
                input: Vec::new(),
                surface_digest: id("surface-1"),
            },
        ))
        .await
        .expect("start")
        .thread_id
        .expect("thread");
    store.fail_next_commit_at(CommitFailurePoint::AfterContext);
    assert!(matches!(
        runtime
            .execute(envelope(
                "compact-atomic",
                "compact-atomic-key",
                RuntimeCommand::ContextCompact {
                    thread_id: thread_id.clone(),
                    compaction_id: id("compaction-atomic"),
                    trigger: ContextCompactionTrigger::Manual,
                    base_checkpoint_id: None,
                    expected_context_revision: ContextRevision(0),
                },
            ))
            .await,
        Err(RuntimeExecuteError::Persistence { .. })
    ));
    assert!(
        store
            .find_operation(&id("compact-atomic"))
            .await
            .expect("operation")
            .is_none()
    );
    assert!(
        store
            .pending_context_preparations()
            .await
            .expect("pending work")
            .is_empty()
    );
    assert!(
        store
            .events_after(&thread_id, None)
            .await
            .expect("events")
            .events
            .iter()
            .all(|event| !matches!(
                &event.event,
                RuntimeEvent::OperationAccepted { operation_id }
                    if operation_id == &id("compact-atomic")
            ))
    );
}

#[tokio::test]
async fn prepare_requires_exact_operation_compaction_base_and_trigger_correlation() {
    let (store, runtime) = fixture();
    let thread_id =
        start_and_accept_compaction(&runtime, "compact-1", ContextCompactionTrigger::Manual).await;
    let mut wrong_operation = preparation(
        thread_id.clone(),
        "compact-1",
        "correlation",
        ContextCompactionTrigger::Manual,
    );
    wrong_operation.operation_id = id("thread-start");
    assert!(matches!(
        runtime.prepare_compaction(wrong_operation).await,
        Err(ContextRuntimeError::OperationNotActive)
    ));

    let mut wrong_trigger = preparation(
        thread_id,
        "compact-1",
        "correlation",
        ContextCompactionTrigger::Manual,
    );
    wrong_trigger.trigger = ContextCompactionTrigger::Automatic;
    assert!(matches!(
        runtime.prepare_compaction(wrong_trigger.clone()).await,
        Err(ContextRuntimeError::BaseChanged)
    ));
    assert!(
        store
            .load_context_candidate(&wrong_trigger.compaction_id)
            .await
            .expect("candidate")
            .is_none()
    );
}

#[tokio::test]
async fn thread_read_and_context_read_have_distinct_fidelity_and_views() {
    let (store, runtime) = fixture();
    let thread_id =
        start_and_accept_compaction(&runtime, "compact-1", ContextCompactionTrigger::Manual).await;
    let mut prepare = preparation(
        thread_id.clone(),
        "compact-1",
        "view",
        ContextCompactionTrigger::Manual,
    );
    prepare.materialized.fidelity = ContextFidelity::DriverExact;
    runtime
        .prepare_compaction(prepare.clone())
        .await
        .expect("prepare");
    runtime
        .recover_compaction(
            &prepare.compaction_id,
            ActivationObservation::Applied {
                digest: prepare.materialized.digest.clone(),
                driver_context_revision: id("driver-revision-11"),
            },
        )
        .await
        .expect("recover");

    let thread = runtime
        .snapshot(RuntimeSnapshotQuery::Thread {
            thread_id: thread_id.clone(),
            at_revision: None,
        })
        .await
        .expect("thread view");
    let context = runtime
        .snapshot(RuntimeSnapshotQuery::Context {
            thread_id,
            at_context_revision: Some(ContextRevision(1)),
        })
        .await
        .expect("context view");
    assert!(matches!(
        thread,
        RuntimeSnapshotResult::Thread {
            snapshot,
        } if snapshot.transcript_fidelity == ContextFidelity::EventProjected
    ));
    let RuntimeSnapshotResult::Context { context } = context else {
        panic!("expected context view");
    };
    assert_eq!(context.fidelity, ContextFidelity::DriverExact);
    assert_eq!(context.blocks, prepare.materialized.blocks);
    assert_eq!(
        context.head.expect("head").provenance,
        prepare.materialized.recipe.provenance
    );
    assert_eq!(store.context_activation_outbox().await.len(), 1);
    assert_eq!(
        store
            .pending_context_activations()
            .await
            .expect("pending activations")
            .len(),
        0
    );
}

#[tokio::test]
async fn manual_and_automatic_compaction_use_the_same_candidate_activation_state_machine() {
    for (suffix, trigger) in [
        ("manual", ContextCompactionTrigger::Manual),
        ("automatic", ContextCompactionTrigger::Automatic),
    ] {
        let (store, runtime) = fixture();
        let operation = format!("compact-{suffix}");
        let thread_id = start_and_accept_compaction(&runtime, &operation, trigger).await;
        let prepare = preparation(thread_id, &operation, suffix, trigger);
        runtime
            .prepare_compaction(prepare.clone())
            .await
            .expect("prepare");
        let activation = store
            .load_context_activation(&prepare.activation_id)
            .await
            .expect("activation")
            .expect("record");
        assert_eq!(activation.status, ContextActivationStatus::Prepared);
        assert_eq!(
            store
                .load_context_candidate(&prepare.compaction_id)
                .await
                .expect("candidate")
                .expect("record")
                .trigger,
            trigger
        );
    }
}

#[tokio::test]
async fn applied_confirmation_survives_a_head_cas_crash_and_recovery_is_reentrant() {
    let (store, runtime) = fixture();
    let thread_id =
        start_and_accept_compaction(&runtime, "compact-1", ContextCompactionTrigger::Manual).await;
    let prepare = preparation(
        thread_id.clone(),
        "compact-1",
        "crash",
        ContextCompactionTrigger::Manual,
    );
    runtime
        .prepare_compaction(prepare.clone())
        .await
        .expect("prepare");
    runtime
        .confirm_compaction_activation(
            &prepare.compaction_id,
            prepare.materialized.digest.clone(),
            id("driver-revision-12"),
        )
        .await
        .expect("applied confirmation");
    assert!(matches!(
        store
            .recoverable_context_activations()
            .await
            .expect("recovery discovery")
            .as_slice(),
        [activation] if matches!(activation.status, ContextActivationStatus::Applied { .. })
    ));
    store.fail_next_commit_at(CommitFailurePoint::AfterContext);
    assert!(matches!(
        runtime.finalize_compaction(&prepare.compaction_id).await,
        Err(ContextRuntimeError::Store(_))
    ));
    assert!(
        store
            .load_context_head(&thread_id)
            .await
            .expect("head")
            .is_none()
    );
    assert!(matches!(
        store
            .load_context_activation(&prepare.activation_id)
            .await
            .expect("activation")
            .expect("record")
            .status,
        ContextActivationStatus::Applied { .. }
    ));

    runtime
        .finalize_compaction(&prepare.compaction_id)
        .await
        .expect("retry");
    let durable_event_count = store
        .events_after(&thread_id, None)
        .await
        .expect("events")
        .events
        .len();
    runtime
        .confirm_compaction_activation(
            &prepare.compaction_id,
            prepare.materialized.digest.clone(),
            id("driver-revision-12"),
        )
        .await
        .expect("duplicate applied ack");
    assert!(matches!(
        runtime
            .confirm_compaction_activation(
                &prepare.compaction_id,
                prepare.materialized.digest.clone(),
                id("conflicting-driver-revision"),
            )
            .await,
        Err(ContextRuntimeError::DigestMismatch)
    ));
    runtime
        .finalize_compaction(&prepare.compaction_id)
        .await
        .expect("idempotent retry");
    assert_eq!(
        store
            .events_after(&thread_id, None)
            .await
            .expect("events")
            .events
            .len(),
        durable_event_count
    );
    assert!(matches!(
        store
            .find_operation(&prepare.operation_id)
            .await
            .expect("operation")
            .expect("record")
            .terminal,
        Some(RuntimeOperationTerminal::Succeeded)
    ));
    assert!(
        store
            .recoverable_context_activations()
            .await
            .expect("recovery discovery")
            .is_empty()
    );
    assert_eq!(
        store
            .load_context_head(&thread_id)
            .await
            .expect("head")
            .expect("active")
            .checkpoint_id,
        prepare.checkpoint_id
    );
}

#[tokio::test]
async fn managed_compaction_is_serialized_before_any_second_driver_side_effect() {
    let (store, runtime) = fixture();
    let thread_id =
        start_and_accept_compaction(&runtime, "compact-left", ContextCompactionTrigger::Manual)
            .await;
    let conflict = runtime
        .execute(envelope(
            "compact-right",
            "compact-right-key",
            RuntimeCommand::ContextCompact {
                thread_id: thread_id.clone(),
                compaction_id: id("compact-right"),
                trigger: ContextCompactionTrigger::Automatic,
                base_checkpoint_id: None,
                expected_context_revision: ContextRevision(0),
            },
        ))
        .await
        .expect_err("second compaction must be rejected before prepare/activation");
    assert!(matches!(
        conflict,
        RuntimeExecuteError::ContextCompactionInProgress { operation_id }
            if operation_id == id("compact-left")
    ));
    assert_eq!(
        store
            .pending_context_preparations()
            .await
            .expect("pending work")
            .len(),
        1
    );
    assert!(store.context_activation_outbox().await.is_empty());

    let left = preparation(
        thread_id.clone(),
        "compact-left",
        "left",
        ContextCompactionTrigger::Manual,
    );
    runtime
        .prepare_compaction(left.clone())
        .await
        .expect("left prepare");
    runtime
        .confirm_compaction_activation(
            &left.compaction_id,
            left.materialized.digest.clone(),
            id("driver-left"),
        )
        .await
        .expect("left applied");
    runtime
        .finalize_compaction(&left.compaction_id)
        .await
        .expect("left finalized");
    assert_eq!(
        store
            .load_context_head(&thread_id)
            .await
            .expect("head")
            .expect("active")
            .checkpoint_id,
        left.checkpoint_id
    );
    assert_eq!(
        store
            .load_thread(&thread_id)
            .await
            .expect("thread")
            .expect("state")
            .status,
        RuntimeThreadStatus::Active
    );
    runtime
        .execute(envelope(
            "compact-after-terminal",
            "compact-after-terminal-key",
            RuntimeCommand::ContextCompact {
                thread_id,
                compaction_id: id("compact-after-terminal"),
                trigger: ContextCompactionTrigger::Automatic,
                base_checkpoint_id: Some(left.checkpoint_id),
                expected_context_revision: ContextRevision(1),
            },
        ))
        .await
        .expect("terminal work releases the per-thread compaction slot");
}

#[tokio::test]
async fn durable_activation_status_cannot_move_back_from_terminal_to_prepared() {
    let (store, runtime) = fixture();
    let thread_id =
        start_and_accept_compaction(&runtime, "compact-1", ContextCompactionTrigger::Manual).await;
    let prepare = preparation(
        thread_id.clone(),
        "compact-1",
        "terminal",
        ContextCompactionTrigger::Manual,
    );
    runtime
        .prepare_compaction(prepare.clone())
        .await
        .expect("prepare");
    runtime
        .recover_compaction(
            &prepare.compaction_id,
            ActivationObservation::Applied {
                digest: prepare.materialized.digest.clone(),
                driver_context_revision: id("driver-terminal"),
            },
        )
        .await
        .expect("complete");
    let projection = store
        .load_thread(&thread_id)
        .await
        .expect("thread")
        .expect("state");
    let mut activation = store
        .load_context_activation(&prepare.activation_id)
        .await
        .expect("activation")
        .expect("record");
    activation.status = ContextActivationStatus::Prepared;
    let error = store
        .commit(RuntimeCommit {
            expected_projection_revision: Some(projection.revision),
            projection,
            operation: None,
            operation_terminals: Vec::new(),
            events: Vec::new(),
            outbox: Vec::new(),
            context_activation_outbox: Vec::new(),
            context_preparation_work_items: Vec::new(),
            context_checkpoints: Vec::new(),
            context_candidates: Vec::new(),
            context_activations: vec![activation],
            context_head: None,
            quarantine: Vec::new(),
        })
        .await
        .expect_err("terminal activation cannot regress");
    assert!(matches!(
        error,
        agentdash_agent_runtime::RuntimeStoreError::ContextInvariant {
            violation: agentdash_agent_runtime::ContextStoreInvariant::ActivationTransition,
        }
    ));
}

#[tokio::test]
async fn conflicting_post_apply_observation_desynchronizes_instead_of_claiming_false_success() {
    let (store, runtime) = fixture();
    let thread_id =
        start_and_accept_compaction(&runtime, "compact-1", ContextCompactionTrigger::Manual).await;
    let prepare = preparation(
        thread_id.clone(),
        "compact-1",
        "applied",
        ContextCompactionTrigger::Manual,
    );
    runtime
        .prepare_compaction(prepare.clone())
        .await
        .expect("prepare");
    runtime
        .confirm_compaction_activation(
            &prepare.compaction_id,
            prepare.materialized.digest.clone(),
            id("driver-applied"),
        )
        .await
        .expect("applied");

    runtime
        .recover_compaction(&prepare.compaction_id, ActivationObservation::NotApplied)
        .await
        .expect("desynchronized terminal");
    let thread = store
        .load_thread(&thread_id)
        .await
        .expect("thread")
        .expect("state");
    assert_eq!(thread.status, RuntimeThreadStatus::Desynchronized);
    assert!(
        store
            .load_context_head(&thread_id)
            .await
            .expect("head")
            .is_none()
    );
    assert!(matches!(
        store
            .load_context_activation(&prepare.activation_id)
            .await
            .expect("activation")
            .expect("record")
            .status,
        ContextActivationStatus::Terminal {
            terminal: agentdash_agent_runtime::CompactionTerminal::Lost { .. },
            applied: Some(_),
        }
    ));
}

#[tokio::test]
async fn opaque_driver_compaction_never_advances_platform_context_head() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(envelope(
            "thread-start",
            "thread-key",
            RuntimeCommand::ThreadStart {
                input: Vec::new(),
                surface_digest: id("surface-1"),
            },
        ))
        .await
        .expect("start")
        .thread_id
        .expect("thread");
    runtime
        .observe_opaque_driver_compaction(&thread_id)
        .await
        .expect("telemetry");
    assert!(
        store
            .load_context_head(&thread_id)
            .await
            .expect("head")
            .is_none()
    );
    let state = store
        .load_thread(&thread_id)
        .await
        .expect("thread")
        .expect("state");
    assert_eq!(state.context_revision, ContextRevision(0));
    assert!(
        store
            .events_after(&thread_id, None)
            .await
            .expect("events")
            .events
            .iter()
            .any(|event| matches!(event.event, RuntimeEvent::DriverContextCompactedOpaque))
    );
}

#[tokio::test]
async fn opaque_materialization_cannot_be_prepared_as_a_platform_checkpoint() {
    let (store, runtime) = fixture();
    let thread_id =
        start_and_accept_compaction(&runtime, "compact-1", ContextCompactionTrigger::Manual).await;
    let mut prepare = preparation(
        thread_id.clone(),
        "compact-1",
        "opaque",
        ContextCompactionTrigger::Manual,
    );
    prepare.materialized.fidelity = ContextFidelity::Opaque;
    assert!(matches!(
        runtime.prepare_compaction(prepare).await,
        Err(ContextRuntimeError::OpaqueContext)
    ));
    assert!(
        store
            .load_context_head(&thread_id)
            .await
            .expect("head")
            .is_none()
    );
    assert!(store.context_activation_outbox().await.is_empty());
}

#[tokio::test]
async fn unverifiable_activation_desynchronizes_the_thread_and_blocks_new_turns() {
    let (store, runtime) = fixture();
    let thread_id =
        start_and_accept_compaction(&runtime, "compact-1", ContextCompactionTrigger::Automatic)
            .await;
    let prepare = preparation(
        thread_id.clone(),
        "compact-1",
        "lost",
        ContextCompactionTrigger::Automatic,
    );
    runtime
        .prepare_compaction(prepare.clone())
        .await
        .expect("prepare");
    runtime
        .recover_compaction(
            &prepare.compaction_id,
            ActivationObservation::Unverifiable {
                reason: "driver cannot prove activation state".to_string(),
            },
        )
        .await
        .expect("lost terminal");
    assert_eq!(
        store
            .load_thread(&thread_id)
            .await
            .expect("thread")
            .expect("state")
            .status,
        RuntimeThreadStatus::Desynchronized
    );
    assert!(matches!(
        runtime
            .execute(envelope(
                "turn-1",
                "turn-key",
                RuntimeCommand::TurnStart {
                    thread_id,
                    input: Vec::new(),
                },
            ))
            .await,
        Err(RuntimeExecuteError::Unsupported { .. })
    ));
}
