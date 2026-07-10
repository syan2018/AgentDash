use std::{collections::BTreeSet, str::FromStr, sync::Arc};

use agentdash_agent_runtime::{
    CommitFailurePoint, DriverEventAdmission, InMemoryRuntimeStore, ManagedAgentRuntime,
    RuntimeKernelDefaults, RuntimeRepository,
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
            configuration_boundary: ConfigurationBoundary::HotReplace,
        },
        tools: ToolProfile {
            channels: BTreeSet::new(),
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
        lifecycle: [
            LifecycleCapability::ThreadStart,
            LifecycleCapability::ThreadResume,
            LifecycleCapability::TurnStart,
            LifecycleCapability::TurnSteer,
            LifecycleCapability::TurnInterrupt,
            LifecycleCapability::ToolSetReplace,
        ]
        .into_iter()
        .collect(),
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

fn fixture() -> (
    Arc<InMemoryRuntimeStore>,
    ManagedAgentRuntime<InMemoryRuntimeStore>,
) {
    let store = Arc::new(InMemoryRuntimeStore::default());
    let runtime = ManagedAgentRuntime::new(
        store.clone(),
        RuntimeKernelDefaults {
            binding_id: id("binding-1"),
            driver_generation: RuntimeDriverGeneration(7),
            source_thread_id: id("source-1"),
            profile_digest: id("profile-1"),
            bound_profile: profile(),
        },
    );
    (store, runtime)
}

fn command(
    operation: &str,
    key: &str,
    expected: Option<u64>,
    command: RuntimeCommand,
) -> RuntimeCommandEnvelope {
    RuntimeCommandEnvelope {
        meta: OperationMeta {
            operation_id: id(operation),
            idempotency_key: id(key),
            expected_thread_revision: expected.map(RuntimeRevision),
            actor: RuntimeActor::User {
                subject: "tester".to_string(),
            },
        },
        command,
    }
}

fn start() -> RuntimeCommandEnvelope {
    command(
        "op-1",
        "key-1",
        None,
        RuntimeCommand::ThreadStart {
            input: vec![RuntimeInput::Text {
                text: "hello".to_string(),
            }],
            surface_digest: id("surface-1"),
        },
    )
}

fn driver(event: RuntimeEvent) -> DriverEventEnvelope {
    DriverEventEnvelope {
        binding_id: id("binding-1"),
        generation: RuntimeDriverGeneration(7),
        source_thread_id: id("source-1"),
        source_turn_id: None,
        source_item_id: None,
        event,
    }
}

async fn thread_snapshot(
    runtime: &ManagedAgentRuntime<InMemoryRuntimeStore>,
    thread_id: RuntimeThreadId,
) -> RuntimeSnapshot {
    match runtime
        .snapshot(RuntimeSnapshotQuery::Thread {
            thread_id,
            at_revision: None,
        })
        .await
        .expect("snapshot")
    {
        RuntimeSnapshotResult::Thread { snapshot } => *snapshot,
        RuntimeSnapshotResult::Context { .. } => panic!("expected thread snapshot"),
    }
}

#[tokio::test]
async fn acceptance_projection_journal_and_outbox_commit_atomically() {
    let (store, runtime) = fixture();
    store.fail_next_commit();
    assert!(matches!(
        runtime.execute(start()).await,
        Err(RuntimeExecuteError::Persistence { .. })
    ));
    assert!(store.outbox().await.is_empty());
    assert!(
        store
            .find_operation(&id("op-1"))
            .await
            .expect("read")
            .is_none()
    );

    let receipt = runtime.execute(start()).await.expect("accepted");
    assert_eq!(receipt.operation_sequence.0, 1);
    assert_eq!(store.outbox().await.len(), 1);
    let events = store
        .events_after(&id("thread-source-1"), None)
        .await
        .expect("events")
        .events;
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[0].event,
        RuntimeEvent::OperationAccepted { .. }
    ));
}

#[tokio::test]
async fn every_injected_write_stage_rolls_back_the_complete_acceptance_write_set() {
    for point in [
        CommitFailurePoint::BeforeWrite,
        CommitFailurePoint::AfterProjection,
        CommitFailurePoint::AfterOperation,
        CommitFailurePoint::AfterEvents,
        CommitFailurePoint::AfterOutbox,
    ] {
        let (store, runtime) = fixture();
        store.fail_next_commit_at(point);
        assert!(matches!(
            runtime.execute(start()).await,
            Err(RuntimeExecuteError::Persistence { .. })
        ));
        assert!(
            store.outbox().await.is_empty(),
            "outbox leaked at {point:?}"
        );
        assert!(
            store
                .find_operation(&id("op-1"))
                .await
                .expect("read")
                .is_none(),
            "operation leaked at {point:?}"
        );
        assert!(
            store
                .load_thread(&id("thread-source-1"))
                .await
                .expect("read")
                .is_none(),
            "projection leaked at {point:?}"
        );
        assert!(
            store
                .events_after(&id("thread-source-1"), None)
                .await
                .expect("events")
                .events
                .is_empty(),
            "journal leaked at {point:?}"
        );
    }
}

#[tokio::test]
async fn idempotency_expected_revision_and_operation_sequence_are_enforced() {
    let (store, runtime) = fixture();
    let first = runtime.execute(start()).await.expect("start");
    assert!(runtime.execute(start()).await.expect("duplicate").duplicate);
    assert_eq!(store.outbox().await.len(), 1);
    let thread_id = first.thread_id.expect("thread");

    let turn = |expected| {
        command(
            "op-2",
            "key-2",
            Some(expected),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                input: Vec::new(),
            },
        )
    };
    assert!(matches!(
        runtime.execute(turn(1)).await,
        Err(RuntimeExecuteError::RevisionConflict { .. })
    ));
    assert_eq!(
        runtime
            .execute(turn(2))
            .await
            .expect("turn accepted")
            .operation_sequence
            .0,
        2
    );
}

#[tokio::test]
async fn operation_identity_binds_actor_and_thread_scoped_key_to_the_typed_command() {
    let (_store, runtime) = fixture();
    runtime.execute(start()).await.expect("start");

    let mut changed_actor = start();
    changed_actor.meta.actor = RuntimeActor::System {
        component: "scheduler".to_string(),
    };
    assert!(matches!(
        runtime.execute(changed_actor).await,
        Err(RuntimeExecuteError::OperationConflict {
            conflict: OperationConflictKind::OperationIdReused,
            ..
        })
    ));

    let thread_id: RuntimeThreadId = id("thread-source-1");
    runtime
        .execute(command(
            "op-2",
            "shared-key",
            Some(2),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    let mut changed_actor_and_payload = command(
        "op-3",
        "shared-key",
        Some(4),
        RuntimeCommand::TurnInterrupt {
            thread_id,
            expected_turn_id: id("turn-op-2"),
        },
    );
    changed_actor_and_payload.meta.actor = RuntimeActor::System {
        component: "scheduler".to_string(),
    };
    assert!(matches!(
        runtime.execute(changed_actor_and_payload).await,
        Err(RuntimeExecuteError::OperationConflict {
            conflict: OperationConflictKind::IdempotencyKeyReused,
            ..
        })
    ));
}

#[tokio::test]
async fn concurrent_mutations_allocate_sequences_only_for_the_cas_winner() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("start")
        .thread_id
        .expect("thread");
    let turn = command(
        "op-2",
        "key-2",
        Some(2),
        RuntimeCommand::TurnStart {
            thread_id: thread_id.clone(),
            input: Vec::new(),
        },
    );
    let settings = command(
        "op-3",
        "key-3",
        Some(2),
        RuntimeCommand::ThreadSettingsUpdate {
            thread_id: thread_id.clone(),
            instructions: vec!["be precise".to_string()],
        },
    );

    let (left, right) = tokio::join!(runtime.execute(turn), runtime.execute(settings));
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(left.is_ok() || matches!(left, Err(RuntimeExecuteError::RevisionConflict { .. })));
    assert!(right.is_ok() || matches!(right, Err(RuntimeExecuteError::RevisionConflict { .. })));

    let projection = store
        .load_thread(&thread_id)
        .await
        .expect("read")
        .expect("projection");
    assert_eq!(projection.next_operation_sequence, OperationSequence(2));
    let events = store
        .events_after(&thread_id, None)
        .await
        .expect("events")
        .events;
    assert!(
        events
            .windows(2)
            .all(|pair| pair[1].sequence.expect("cursor").0
                == pair[0].sequence.expect("cursor").0 + 1)
    );
    assert_eq!(
        events.last().and_then(|event| event.sequence),
        Some(projection.next_event_sequence)
    );
    assert_eq!(projection.revision.0, projection.next_event_sequence.0);
}

#[tokio::test]
async fn event_cursor_distinguishes_future_cursor_from_retention_gap() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    runtime
        .execute(command(
            "op-2",
            "key-2",
            Some(2),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    store
        .discard_events_through(&thread_id, EventSequence(2))
        .await;

    assert!(matches!(
        runtime
            .events(RuntimeEventSubscription {
                thread_id: thread_id.clone(),
                after: Some(EventSequence(1)),
                include_transient: false,
            })
            .await,
        Err(RuntimeSubscribeError::CursorGap {
            requested: EventSequence(1),
            earliest_available: EventSequence(3),
            latest_available: EventSequence(4),
        })
    ));
    assert!(matches!(
        runtime
            .events(RuntimeEventSubscription {
                thread_id,
                after: Some(EventSequence(5)),
                include_transient: false,
            })
            .await,
        Err(RuntimeSubscribeError::InvalidCursor)
    ));
}

#[tokio::test]
async fn exactly_one_terminal_and_lost_are_authoritative() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    runtime
        .execute(command(
            "op-2",
            "key-2",
            Some(2),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    let turn_id: RuntimeTurnId = id("turn-op-2");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::TurnTerminal {
            turn_id: turn_id.clone(),
            terminal: RuntimeTurnTerminal::Lost,
            message: Some("driver disappeared".to_string()),
        }))
        .await
        .expect("lost");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::OperationTerminal {
            operation_id: id("op-2"),
            terminal: RuntimeOperationTerminal::Lost {
                retryable: true,
                message: None,
            },
        }))
        .await
        .expect("operation lost");

    assert!(matches!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::TurnTerminal {
                turn_id,
                terminal: RuntimeTurnTerminal::Completed,
                message: None,
            }))
            .await
            .expect("critical protocol fact"),
        DriverEventAdmission::Durable { .. }
    ));
    assert_eq!(store.quarantined().await.len(), 1);
    assert!(
        thread_snapshot(&runtime, thread_id)
            .await
            .active_turn_id
            .is_none()
    );
    assert!(
        store
            .find_operation(&id("op-2"))
            .await
            .expect("read")
            .expect("operation")
            .terminal
            .is_some()
    );
}

#[tokio::test]
async fn stale_generation_is_quarantined_without_advancing_cursor() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    let before = store
        .events_after(&thread_id, None)
        .await
        .expect("events")
        .events
        .len();
    let mut stale = driver(RuntimeEvent::ThreadStatusChanged {
        status: RuntimeThreadStatus::Lost,
    });
    stale.generation = RuntimeDriverGeneration(6);
    assert_eq!(
        runtime.ingest_driver_event(stale).await.expect("admission"),
        DriverEventAdmission::Quarantined
    );
    assert_eq!(
        store
            .events_after(&thread_id, None)
            .await
            .expect("events")
            .events
            .len(),
        before
    );
}

#[tokio::test]
async fn driver_cannot_emit_runtime_owned_context_transitions() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::ContextCheckpointActivated {
            checkpoint_id: id("forged-checkpoint"),
            candidate_id: id("forged-candidate"),
            activation_id: id("forged-activation"),
            compaction_id: id("forged-compaction"),
            context_revision: ContextRevision(1),
            digest: id("forged-digest"),
        }))
        .await
        .expect("protocol violation persisted");
    let projection = store
        .load_thread(&thread_id)
        .await
        .expect("thread")
        .expect("state");
    assert_eq!(projection.status, RuntimeThreadStatus::Lost);
    assert!(projection.active_checkpoint_id.is_none());
    assert!(matches!(
        store.quarantined().await.as_slice(),
        [agentdash_agent_runtime::QuarantinedDriverEvent {
            reason:
                agentdash_agent_runtime::DriverEventQuarantineReason::DriverRuntimeOwnedContextEvent,
            ..
        }]
    ));
}

#[tokio::test]
async fn transient_delta_has_no_durable_cursor() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    runtime
        .execute(command(
            "op-2",
            "key-2",
            Some(2),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    let turn_id: RuntimeTurnId = id("turn-op-2");
    let item_id: RuntimeItemId = id("item-1");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::ItemStarted {
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
        }))
        .await
        .expect("item");
    assert_eq!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::ItemDelta {
                turn_id,
                item_id,
                delta: "token".to_string(),
            }))
            .await
            .expect("delta"),
        DriverEventAdmission::Transient
    );
    let durable = store
        .events_after(&thread_id, Some(EventSequence(4)))
        .await
        .expect("tail")
        .events;
    assert_eq!(durable.len(), 1);
    let mut stream = runtime
        .events(RuntimeEventSubscription {
            thread_id,
            after: Some(EventSequence(4)),
            include_transient: true,
        })
        .await
        .expect("stream");
    assert!(
        stream
            .next()
            .await
            .expect("durable")
            .expect("ok")
            .sequence
            .is_some()
    );
    assert!(
        stream
            .next()
            .await
            .expect("transient")
            .expect("ok")
            .sequence
            .is_none()
    );
}

#[tokio::test]
async fn item_and_interaction_transitions_share_the_thread_projection() {
    let (_store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    runtime
        .execute(command(
            "op-2",
            "key-2",
            Some(2),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    let turn_id: RuntimeTurnId = id("turn-op-2");
    let item_id: RuntimeItemId = id("item-1");
    let interaction_id: RuntimeInteractionId = id("interaction-1");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::ItemStarted {
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
        }))
        .await
        .expect("item");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::InteractionRequested {
            turn_id: turn_id.clone(),
            item_id: Some(item_id.clone()),
            interaction_id: interaction_id.clone(),
            interaction_kind: RuntimeInteractionKind::CommandApproval,
            prompt: "approve?".to_string(),
        }))
        .await
        .expect("interaction");
    runtime
        .execute(command(
            "op-3",
            "key-3",
            Some(6),
            RuntimeCommand::InteractionRespond {
                thread_id: thread_id.clone(),
                interaction_id,
                response: InteractionResponse::Approved,
            },
        ))
        .await
        .expect("response");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::ItemTerminal {
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
            terminal: RuntimeItemTerminal::Completed {
                final_content: RuntimeItemContent::AgentMessage {
                    text: "done".to_string(),
                },
            },
        }))
        .await
        .expect("item terminal");
    assert!(matches!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::ItemDelta {
                turn_id,
                item_id,
                delta: "late".to_string(),
            }))
            .await
            .expect("late delta protocol fact"),
        DriverEventAdmission::Durable { .. }
    ));
    assert!(
        thread_snapshot(&runtime, thread_id)
            .await
            .pending_interactions
            .is_empty()
    );
}

#[tokio::test]
async fn critical_protocol_violation_moves_thread_to_lost() {
    let (_store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::ProtocolViolation {
            code: RuntimeProtocolViolationCode::InvalidLifecycleTransition,
            message: "terminal preceded start".to_string(),
            critical: true,
        }))
        .await
        .expect("violation persisted");
    assert_eq!(
        thread_snapshot(&runtime, thread_id).await.status,
        RuntimeThreadStatus::Lost
    );
}

#[tokio::test]
async fn binding_loss_atomically_converges_every_active_entity_to_lost() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    runtime
        .execute(command(
            "op-2",
            "key-2",
            Some(2),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    let turn_id: RuntimeTurnId = id("turn-op-2");
    let item_id: RuntimeItemId = id("item-1");
    let interaction_id: RuntimeInteractionId = id("interaction-1");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::ItemStarted {
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
        }))
        .await
        .expect("item");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::InteractionRequested {
            turn_id,
            item_id: Some(item_id),
            interaction_id,
            interaction_kind: RuntimeInteractionKind::CommandApproval,
            prompt: "approve?".to_string(),
        }))
        .await
        .expect("interaction");

    runtime
        .ingest_driver_event(driver(RuntimeEvent::BindingLost {
            binding_id: id("binding-1"),
            reason: "connection lost".to_string(),
        }))
        .await
        .expect("binding loss");

    let projection = store
        .load_thread(&thread_id)
        .await
        .expect("read")
        .expect("projection");
    assert_eq!(projection.status, RuntimeThreadStatus::Lost);
    assert!(projection.active_turn_id.is_none());
    assert!(projection.items.values().all(|item| matches!(
        &item.phase,
        agentdash_agent_runtime::EntityPhase::Terminal(RuntimeItemTerminal::Lost { .. })
    )));
    assert!(projection.interactions.values().all(|interaction| matches!(
        &interaction.phase,
        agentdash_agent_runtime::EntityPhase::Terminal(RuntimeInteractionTerminal::Lost)
    )));
    for operation_id in [id("op-1"), id("op-2")] {
        assert!(matches!(
            store
                .find_operation(&operation_id)
                .await
                .expect("read")
                .expect("operation")
                .terminal,
            Some(RuntimeOperationTerminal::Lost { .. })
        ));
    }
    assert!(store.quarantined().await.is_empty());
}

#[tokio::test]
async fn malformed_lifecycle_is_typed_quarantined_and_persists_critical_loss() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    runtime
        .execute(command(
            "op-2",
            "key-2",
            Some(2),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    let turn_id: RuntimeTurnId = id("turn-op-2");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::ItemStarted {
            turn_id: turn_id.clone(),
            item_id: id("item-1"),
        }))
        .await
        .expect("item");

    assert!(matches!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::TurnTerminal {
                turn_id,
                terminal: RuntimeTurnTerminal::Completed,
                message: None,
            }))
            .await
            .expect("critical fact"),
        DriverEventAdmission::Durable { .. }
    ));
    assert!(matches!(
        store.quarantined().await.as_slice(),
        [agentdash_agent_runtime::QuarantinedDriverEvent {
            reason: agentdash_agent_runtime::DriverEventQuarantineReason::InvalidTransition {
                error: agentdash_agent_runtime::TransitionError::TurnHasActiveChildren { .. }
            },
            ..
        }]
    ));
    let events = store
        .events_after(&thread_id, None)
        .await
        .expect("events")
        .events;
    assert!(events.iter().any(|event| matches!(
        &event.event,
        RuntimeEvent::ProtocolViolation {
            code: RuntimeProtocolViolationCode::InvalidLifecycleTransition,
            critical: true,
            ..
        }
    )));
    let snapshot = thread_snapshot(&runtime, thread_id).await;
    assert_eq!(snapshot.status, RuntimeThreadStatus::Lost);
    assert!(snapshot.active_turn_id.is_none());
}
