use std::{collections::BTreeSet, str::FromStr, sync::Arc};

use agentdash_agent_runtime::*;
use agentdash_agent_runtime_contract::*;

mod support;
use support::TestTerminalPresentationProjector;

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
    Arc<RuntimeStoreFixture>,
    ManagedAgentRuntime<RuntimeStoreFixture>,
) {
    let store = Arc::new(RuntimeStoreFixture::default());
    let runtime =
        ManagedAgentRuntime::new(store.clone(), Arc::new(TestTerminalPresentationProjector));
    (store, runtime)
}

async fn start(runtime: &ManagedAgentRuntime<RuntimeStoreFixture>) -> RuntimeThreadId {
    runtime
        .execute(RuntimeCommandEnvelope {
            presentation: Vec::new(),
            meta: OperationMeta {
                operation_id: id("operation-start"),
                idempotency_key: id("key-start"),
                expected_thread_revision: None,
                actor: RuntimeActor::System {
                    component: "hook-test".to_string(),
                },
            },
            command: RuntimeCommand::ThreadStart {
                thread_id: id("thread-source-hook"),
                presentation_thread_id: id("presentation-thread-hook"),
                presentation_turn_id: None,
                binding_id: id("binding-hook"),
                driver_generation: RuntimeDriverGeneration(1),
                source_thread_id: id("source-hook"),
                profile_digest: id("profile-hook"),
                bound_profile: Box::new(profile()),
                input: Vec::new(),
                surface: Box::new(RuntimeSurfaceDescriptor {
                    source_frame_id: "frame-hook".to_string(),
                    surface_revision: SurfaceRevision(1),
                    surface_digest: id("surface-hook"),
                    vfs_digest: "vfs-hook".to_string(),
                    context_recipe_revision: ContextRecipeRevision(1),
                    context_digest: id("context-hook"),
                    settings_revision: ThreadSettingsRevision(0),
                    tool_set_revision: ToolSetRevision(0),
                    tool_set_digest: "tools-hook".to_string(),
                    hook_plan: BoundRuntimeHookPlan {
                        revision: HookPlanRevision(1),
                        digest: id("hook-plan-empty-1"),
                        entries: Vec::new(),
                    },
                    terminal_hook_effect_binding: None,
                }),
                settings_revision: ThreadSettingsRevision(0),
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
            revision: HookPlanRevision(2),
            digest: id("hook-plan-2"),
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
        presentation: None,
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
async fn context_frame_hook_effect_commits_with_terminal_and_replay_does_not_duplicate() {
    let (store, runtime) = fixture();
    let thread_id = start(&runtime).await;
    runtime
        .bind_hook_plan(plan(thread_id.clone(), [HookAction::EmitEffect].into()))
        .await
        .unwrap();
    let HookAdmission::Durable(run) = runtime
        .accept_hook(invocation(thread_id.clone(), "hook-run-context"))
        .await
        .unwrap()
    else {
        panic!("effect hook must be durable")
    };
    let run = runtime.start_hook(&run.hook_run_id).await.unwrap();
    let payload = serde_json::Value::Null;
    let mut effect = HookEffect {
        effect_id: id("hook-effect-context"),
        hook_run_id: run.hook_run_id.clone(),
        thread_id: thread_id.clone(),
        idempotency_key: "context-frame:hook-run-context".to_string(),
        descriptor: HookEffectDescriptor {
            effect_type: "runtime_context_presentation".to_string(),
            schema_version: 1,
            target_authority: "agent_runtime_context_projection".to_string(),
            retry_limit: 0,
            payload_digest: hook_effect_payload_digest(&payload),
        },
        payload,
        presentation: Some(ContextFrameFacts {
            kind: agentdash_agent_protocol::ContextFrameKind::SystemNotice,
            source: agentdash_agent_protocol::ContextFrameSource::RuntimeContextUpdate,
            phase_node: None,
            apply_mode: None,
            delivery_status:
                agentdash_agent_protocol::ContextDeliveryStatus::QueuedForTransformContext,
            delivery_channel: agentdash_agent_protocol::ContextDeliveryChannel::TurnStart,
            message_role: agentdash_agent_protocol::ContextMessageRole::User,
            rendered_text: "continue".to_string(),
            sections: vec![
                agentdash_agent_protocol::ContextFrameSection::SystemNotice {
                    title: "TurnStart Notice".to_string(),
                    summary: "TurnStart notice 已桥接为 ContextFrame。".to_string(),
                    body: Some("continue".to_string()),
                },
            ],
        }),
    };
    effect.descriptor.payload_digest = hook_effect_content_digest(&effect);
    let completion = HookCompletion {
        status: HookRunStatus::Completed,
        decision: HookGateDecision::Continue,
        message: None,
    };
    runtime
        .complete_hook(&run.hook_run_id, completion.clone(), vec![effect.clone()])
        .await
        .unwrap();
    runtime
        .complete_hook(&run.hook_run_id, completion, vec![effect])
        .await
        .unwrap();
    let durable_effects = store.hook_effects(&run.hook_run_id).await.unwrap();
    assert_eq!(durable_effects.len(), 1);
    assert!(!durable_effects[0].requires_external_dispatch());
    let frames = store
        .journal_records_after(&thread_id, None)
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
    assert_eq!(frames.len(), 1);
    assert_eq!(
        frames[0].kind,
        agentdash_agent_protocol::ContextFrameKind::SystemNotice
    );
    assert_eq!(frames[0].rendered_text, "continue");
    assert!(matches!(
        frames[0].sections.as_slice(),
        [agentdash_agent_protocol::ContextFrameSection::SystemNotice { body: Some(body), .. }]
            if body == "continue"
    ));
}

#[tokio::test]
async fn arbitrary_context_frame_json_is_rejected_before_hook_terminal_commit() {
    let (store, runtime) = fixture();
    let thread_id = start(&runtime).await;
    runtime
        .bind_hook_plan(plan(thread_id.clone(), [HookAction::EmitEffect].into()))
        .await
        .unwrap();
    let HookAdmission::Durable(run) = runtime
        .accept_hook(invocation(thread_id, "hook-run-legacy-context"))
        .await
        .unwrap()
    else {
        panic!("effect hook must be durable")
    };
    let run = runtime.start_hook(&run.hook_run_id).await.unwrap();
    let payload = serde_json::json!({"kind":"system_notice","rendered_text":"forged"});
    let error = runtime
        .complete_hook(
            &run.hook_run_id,
            HookCompletion {
                status: HookRunStatus::Completed,
                decision: HookGateDecision::Continue,
                message: None,
            },
            vec![HookEffect {
                effect_id: id("legacy-context-effect"),
                hook_run_id: run.hook_run_id.clone(),
                thread_id: run.thread_id.clone(),
                idempotency_key: "legacy-context-effect".to_string(),
                descriptor: HookEffectDescriptor {
                    effect_type: "context_frame".to_string(),
                    schema_version: 1,
                    target_authority: "agent_runtime_context_projection".to_string(),
                    retry_limit: 0,
                    payload_digest: hook_effect_payload_digest(&payload),
                },
                payload,
                presentation: None,
            }],
        )
        .await
        .expect_err("arbitrary ContextFrame JSON must not cross the Hook boundary");
    assert!(matches!(
        error,
        HookOrchestrationError::Domain(HookRuntimeError::InvalidPresentationEffect(_))
    ));
    assert_eq!(
        store
            .load_hook_run(&run.hook_run_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        HookRunStatus::Running
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
                presentation: None,
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
