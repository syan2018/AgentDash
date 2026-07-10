use std::{collections::BTreeSet, str::FromStr, sync::Arc};

use agentdash_agent_runtime::*;
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
            cancellation: false,
        },
        workspace: WorkspaceProfile {
            capabilities: BTreeSet::new(),
            mechanism: DeliveryMechanism::Native,
        },
        interactions: InteractionProfile {
            kinds: BTreeSet::new(),
            durable_correlation: true,
        },
        lifecycle: [LifecycleCapability::ThreadStart].into(),
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
            binding_id: id("binding-hook"),
            driver_generation: RuntimeDriverGeneration(1),
            source_thread_id: id("source-hook"),
            profile_digest: id("profile-hook"),
            bound_profile: profile(),
        },
    );
    (store, runtime)
}

async fn start(runtime: &ManagedAgentRuntime<InMemoryRuntimeStore>) -> RuntimeThreadId {
    runtime
        .execute(RuntimeCommandEnvelope {
            meta: OperationMeta {
                operation_id: id("operation-start"),
                idempotency_key: id("key-start"),
                expected_thread_revision: None,
                actor: RuntimeActor::System {
                    component: "hook-test".to_string(),
                },
            },
            command: RuntimeCommand::ThreadStart {
                input: Vec::new(),
                surface_digest: id("surface-hook"),
            },
        })
        .await
        .expect("start thread")
        .thread_id
        .expect("thread id")
}

fn plan(thread_id: RuntimeThreadId, actions: BTreeSet<HookAction>) -> RuntimeHookPlanBinding {
    RuntimeHookPlanBinding {
        thread_id,
        plan: BoundRuntimeHookPlan {
            revision: HookPlanRevision(1),
            digest: id("hook-plan-1"),
            entries: vec![BoundRuntimeHookEntry {
                definition_id: id("definition-1"),
                point: HookPoint::BeforeTurn,
                actions: actions.clone(),
                delivered_strength: SemanticStrength::ExactDurableBoundary,
                failure_policy: if actions == [HookAction::Observe].into() {
                    HookFailurePolicy::ObserveOnly
                } else {
                    HookFailurePolicy::FailClosed
                },
                required: true,
                site: HookExecutionSite::ManagedRuntime,
            }],
        },
    }
}

fn invocation(thread_id: RuntimeThreadId, run: &str) -> RuntimeHookInvocation {
    RuntimeHookInvocation {
        hook_run_id: id(run),
        thread_id,
        definition_id: id("definition-1"),
        point: HookPoint::BeforeTurn,
        correlation: HookCorrelation {
            operation_id: None,
            turn_id: None,
            item_id: None,
            interaction_id: None,
        },
        input: serde_json::json!({"command": "turn_start"}),
    }
}

#[tokio::test]
async fn actionful_hook_is_recoverable_and_terminal_effect_is_atomic() {
    let (store, runtime) = fixture();
    let thread_id = start(&runtime).await;
    runtime
        .bind_hook_plan(plan(
            thread_id.clone(),
            [HookAction::Block, HookAction::EmitEffect].into(),
        ))
        .await
        .expect("bind plan");
    let HookAdmission::Durable(run) = runtime
        .accept_hook(invocation(thread_id.clone(), "hook-run-1"))
        .await
        .expect("accept hook")
    else {
        panic!("actionful run must be durable")
    };
    assert_eq!(run.status, HookRunStatus::Accepted);
    let run = runtime
        .start_hook(&run.hook_run_id)
        .await
        .expect("start durable hook");
    assert_eq!(run.status, HookRunStatus::Running);
    assert_eq!(
        runtime
            .recoverable_hook_runs()
            .await
            .expect("recovery")
            .len(),
        1
    );

    let payload = serde_json::json!({"message": "continue"});
    let effect = HookEffect {
        effect_id: id("hook-effect-1"),
        hook_run_id: run.hook_run_id.clone(),
        thread_id: thread_id.clone(),
        idempotency_key: "mailbox:hook-run-1".to_string(),
        descriptor: HookEffectDescriptor {
            effect_type: "mailbox.enqueue".to_string(),
            schema_version: 1,
            target_authority: "agent_run_mailbox".to_string(),
            retry_limit: 3,
            payload_digest: hook_effect_payload_digest(&payload),
        },
        payload,
    };
    let terminal = runtime
        .complete_hook(
            &run.hook_run_id,
            HookCompletion {
                status: HookRunStatus::Blocked,
                decision: HookGateDecision::Block,
                message: Some("policy denied".to_string()),
            },
            vec![effect.clone()],
        )
        .await
        .expect("complete hook");
    assert_eq!(terminal.status, HookRunStatus::Blocked);
    assert_eq!(
        runtime
            .complete_hook(
                &run.hook_run_id,
                HookCompletion {
                    status: HookRunStatus::Blocked,
                    decision: HookGateDecision::Block,
                    message: Some("policy denied".to_string()),
                },
                vec![effect],
            )
            .await
            .expect("terminal replay is idempotent")
            .status,
        HookRunStatus::Blocked
    );
    assert!(
        runtime
            .recoverable_hook_runs()
            .await
            .expect("recovery")
            .is_empty()
    );
    assert_eq!(
        store
            .load_hook_run(&run.hook_run_id)
            .await
            .expect("load")
            .expect("run")
            .decision,
        Some(HookGateDecision::Block)
    );
}

#[tokio::test]
async fn silent_observer_does_not_advance_the_durable_cursor() {
    let (_store, runtime) = fixture();
    let thread_id = start(&runtime).await;
    runtime
        .bind_hook_plan(plan(thread_id.clone(), [HookAction::Observe].into()))
        .await
        .expect("bind plan");
    let before = match runtime
        .snapshot(RuntimeSnapshotQuery::Thread {
            thread_id: thread_id.clone(),
            at_revision: None,
        })
        .await
        .expect("snapshot")
    {
        RuntimeSnapshotResult::Thread { snapshot } => snapshot.revision,
        _ => unreachable!(),
    };
    assert!(matches!(
        runtime
            .accept_hook(invocation(thread_id.clone(), "observer-run"))
            .await
            .expect("observe"),
        HookAdmission::SilentObserver
    ));
    let after = match runtime
        .snapshot(RuntimeSnapshotQuery::Thread {
            thread_id,
            at_revision: None,
        })
        .await
        .expect("snapshot")
    {
        RuntimeSnapshotResult::Thread { snapshot } => snapshot.revision,
        _ => unreachable!(),
    };
    assert_eq!(before, after);
}

#[tokio::test]
async fn accepted_started_and_terminal_are_distinct_idempotent_durable_transitions() {
    let (store, runtime) = fixture();
    let thread_id = start(&runtime).await;
    runtime
        .bind_hook_plan(plan(thread_id.clone(), [HookAction::Block].into()))
        .await
        .expect("bind plan");
    let invocation = invocation(thread_id, "hook-run-lifecycle");
    let HookAdmission::Durable(accepted) = runtime
        .accept_hook(invocation.clone())
        .await
        .expect("accept hook")
    else {
        panic!("actionful hook is durable")
    };
    assert_eq!(accepted.status, HookRunStatus::Accepted);
    let HookAdmission::Durable(replayed) = runtime
        .accept_hook(invocation)
        .await
        .expect("accept replay")
    else {
        panic!("actionful hook replay is durable")
    };
    assert_eq!(replayed.status, HookRunStatus::Accepted);

    let (left, right) = tokio::join!(
        runtime.start_hook(&accepted.hook_run_id),
        runtime.start_hook(&accepted.hook_run_id)
    );
    assert_eq!(left.expect("left start").status, HookRunStatus::Running);
    assert_eq!(right.expect("right start").status, HookRunStatus::Running);
    assert_eq!(
        store
            .load_hook_run(&accepted.hook_run_id)
            .await
            .expect("load run")
            .expect("durable run")
            .status,
        HookRunStatus::Running
    );
}

#[tokio::test]
async fn invalid_effect_digest_cannot_advance_terminal_or_event_cursor() {
    let (store, runtime) = fixture();
    let thread_id = start(&runtime).await;
    runtime
        .bind_hook_plan(plan(
            thread_id.clone(),
            [HookAction::Block, HookAction::EmitEffect].into(),
        ))
        .await
        .expect("bind plan");
    let HookAdmission::Durable(run) = runtime
        .accept_hook(invocation(thread_id.clone(), "hook-run-invalid-effect"))
        .await
        .expect("accept hook")
    else {
        panic!("actionful hook is durable")
    };
    let run = runtime
        .start_hook(&run.hook_run_id)
        .await
        .expect("start hook");
    let before = store
        .load_thread(&thread_id)
        .await
        .expect("load thread")
        .expect("thread")
        .next_event_sequence;
    let error = runtime
        .complete_hook(
            &run.hook_run_id,
            HookCompletion {
                status: HookRunStatus::Blocked,
                decision: HookGateDecision::Block,
                message: Some("denied".to_string()),
            },
            vec![HookEffect {
                effect_id: id("hook-effect-tampered"),
                hook_run_id: run.hook_run_id.clone(),
                thread_id: thread_id.clone(),
                idempotency_key: "tampered".to_string(),
                descriptor: HookEffectDescriptor {
                    effect_type: "mailbox.enqueue".to_string(),
                    schema_version: 1,
                    target_authority: "agent_run_mailbox".to_string(),
                    retry_limit: 1,
                    payload_digest: "sha256:not-the-payload".to_string(),
                },
                payload: serde_json::json!({"message": "continue"}),
            }],
        )
        .await
        .expect_err("tampered payload digest is rejected");
    assert!(matches!(
        error,
        HookOrchestrationError::Domain(HookRuntimeError::InvalidEffectDescriptor)
    ));
    let durable = store
        .load_hook_run(&run.hook_run_id)
        .await
        .expect("load run")
        .expect("run");
    assert_eq!(durable.status, HookRunStatus::Running);
    assert_eq!(
        store
            .load_thread(&thread_id)
            .await
            .expect("load thread")
            .expect("thread")
            .next_event_sequence,
        before
    );
}
