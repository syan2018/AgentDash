use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use agentdash_agent_protocol::{
    BackboneEvent, ContextFrameKind, ContextFrameSection, PlatformEvent,
    RuntimeCompanionAgentEntry, RuntimeContextFragmentEntry, RuntimeMemorySourceEntry,
    RuntimeSkillEntry, RuntimeToolSchemaEntry, SkillContextExposure,
};
use agentdash_agent_runtime::{
    AgentSurfaceCompiler, BusinessAgentSurfaceFacts, CommitFailurePoint, ContextProjectionIdentity,
    ContributionMeta, ContributionRequirement, DriverEventAdmission, ManagedAgentRuntime,
    NormalizedAssignmentContext, NormalizedContextSurfaceState, NormalizedMcpServerReadiness,
    NormalizedSkillCluster, NormalizedSurfaceEntity, RuntimeCommit, RuntimePresentationAppendError,
    RuntimePresentationAppendRequest, RuntimeRepository, RuntimeStoreError, RuntimeStoreFixture,
    RuntimeSurfacePresentationPlan, RuntimeTerminalApplicationEffectClaimRequest,
    RuntimeTerminalApplicationEffectOutbox, RuntimeTransientEvents, RuntimeUnitOfWork,
    RuntimeWorkerId, SurfaceSourceRef, ToolContribution, WorkspaceRequirement,
};
use agentdash_agent_runtime_contract::*;

mod support;
use support::TestTerminalPresentationProjector;

fn id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid id")
}

fn abstract_presentation_record(
    thread_id: &RuntimeThreadId,
    sequence: u64,
    revision: RuntimeRevision,
    label: &str,
) -> RuntimeJournalRecord {
    let event = serde_json::from_value(serde_json::json!({
        "type": "item_completed",
        "payload": {
            "item": {
                "type": "dynamicToolCall",
                "id": format!("abstract-{label}"),
                "namespace": null,
                "tool": "persistence_fixture",
                "arguments": { "label": label, "explicit_null": null, "ordered": [1, 2] },
                "status": "completed",
                "contentItems": null,
                "success": true,
                "durationMs": null
            },
            "threadId": "source-thread",
            "turnId": "source-turn",
            "completedAtMs": 1_712_345_678_901_i64 + sequence as i64
        }
    }))
    .expect("valid abstract presentation fixture");
    RuntimeJournalRecord::new(
        RuntimeCarrierMetadata {
            thread_id: thread_id.clone(),
            recorded_at_ms: 9_000 + sequence,
            sequence: Some(EventSequence(sequence)),
            transient: None,
            revision,
            operation_id: None,
            append_idempotency_key: None,
            binding_id: Some(id("binding-1")),
            coordinate: RuntimePresentationCoordinate {
                runtime_turn_id: None,
                presentation_turn_id: Some(id("source-turn")),
                runtime_item_id: None,
                interaction_id: None,
                source_thread_id: Some("source-thread".to_string()),
                source_turn_id: Some("source-turn".to_string()),
                source_item_id: Some(format!("abstract-{label}")),
                source_request_id: None,
                source_entry_index: Some(sequence as u32),
            },
        },
        RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
            PresentationDurability::Durable,
            event,
        )),
    )
    .expect("valid durable presentation record")
}

#[tokio::test]
async fn memory_repository_preserves_ordered_presentation_records_without_rewriting_payload() {
    let (store, runtime) = fixture();
    runtime
        .execute(start())
        .await
        .expect("start runtime thread");
    let thread_id: RuntimeThreadId = id("thread-source-1");
    let base = store
        .load_thread(&thread_id)
        .await
        .expect("load thread")
        .expect("thread");
    let first_sequence = base.next_event_sequence.0 + 1;
    let records = vec![
        abstract_presentation_record(&thread_id, first_sequence, base.revision, "A"),
        abstract_presentation_record(&thread_id, first_sequence + 1, base.revision, "B"),
    ];
    let protected_before = records
        .iter()
        .map(|record| serde_json::to_value(&record.as_presentation().expect("presentation").event))
        .collect::<Result<Vec<_>, _>>()
        .expect("serialize protected bodies");
    let mut projection = base.clone();
    projection.next_event_sequence = EventSequence(first_sequence + 1);
    let mut live = store.subscribe_presentation(&thread_id).await;
    store
        .commit(RuntimeCommit {
            expected_projection_revision: Some(base.revision),
            projection,
            operation: None,
            operation_terminals: Vec::new(),
            records: records.clone(),
            outbox: Vec::new(),
            terminal_application_effects: Vec::new(),
            context_activation_outbox: Vec::new(),
            context_preparation_work_items: Vec::new(),
            context_checkpoints: Vec::new(),
            context_candidates: Vec::new(),
            context_activations: Vec::new(),
            context_head: None,
            hook_plan_binding: None,
            hook_runs: Vec::new(),
            hook_effects: Vec::new(),
            quarantine: Vec::new(),
        })
        .await
        .expect("commit ordered presentation records");

    for expected in &records {
        let received = tokio::time::timeout(Duration::from_secs(1), live.recv())
            .await
            .expect("durable presentation live delivery timed out")
            .expect("durable presentation live sender closed");
        assert_eq!(&received, expected);
    }

    let replay = store
        .journal_records_after(&thread_id, Some(base.next_event_sequence))
        .await
        .expect("replay journal records");
    assert_eq!(replay.records, records);
    let protected_after = replay
        .records
        .iter()
        .map(|record| serde_json::to_value(&record.as_presentation().expect("presentation").event))
        .collect::<Result<Vec<_>, _>>()
        .expect("serialize replayed bodies");
    assert_eq!(protected_after, protected_before);
    assert_eq!(
        protected_after[0].pointer("/payload/item/arguments/explicit_null"),
        Some(&serde_json::Value::Null)
    );

    let duplicate = store
        .commit(RuntimeCommit {
            expected_projection_revision: Some(base.revision),
            projection: base,
            operation: None,
            operation_terminals: Vec::new(),
            records: records.clone(),
            outbox: Vec::new(),
            terminal_application_effects: Vec::new(),
            context_activation_outbox: Vec::new(),
            context_preparation_work_items: Vec::new(),
            context_checkpoints: Vec::new(),
            context_candidates: Vec::new(),
            context_activations: Vec::new(),
            context_head: None,
            hook_plan_binding: None,
            hook_runs: Vec::new(),
            hook_effects: Vec::new(),
            quarantine: Vec::new(),
        })
        .await;
    assert!(matches!(duplicate, Err(RuntimeStoreError::Unavailable(_))));

    let current = store
        .load_thread(&thread_id)
        .await
        .expect("load thread after durable commit")
        .expect("thread");
    let event = abstract_presentation_record(
        &thread_id,
        current.next_event_sequence.0 + 1,
        current.revision,
        "transient",
    )
    .as_presentation()
    .expect("presentation")
    .event
    .clone();
    let transient = RuntimeJournalRecord::new(
        RuntimeCarrierMetadata {
            thread_id: thread_id.clone(),
            recorded_at_ms: 10_000,
            sequence: None,
            transient: Some(RuntimeTransientCoordinate {
                binding_id: id("binding-1"),
                stream_generation: RuntimeDriverGeneration(1),
                sequence: RuntimeTransientSequence(1),
                event_id: id("transient-event-1"),
                turn_id: None,
            }),
            revision: current.revision,
            operation_id: None,
            append_idempotency_key: None,
            binding_id: Some(id("binding-1")),
            coordinate: RuntimePresentationCoordinate {
                runtime_turn_id: None,
                presentation_turn_id: Some(id("source-turn")),
                runtime_item_id: None,
                interaction_id: None,
                source_thread_id: Some("source-thread".to_string()),
                source_turn_id: Some("source-turn".to_string()),
                source_item_id: Some("abstract-transient".to_string()),
                source_request_id: None,
                source_entry_index: Some(1),
            },
        },
        RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
            PresentationDurability::Ephemeral,
            event,
        )),
    )
    .expect("valid ephemeral presentation record");
    let mut invalid_projection = current.clone();
    invalid_projection.next_event_sequence =
        EventSequence(current.next_event_sequence.0.saturating_add(1));
    let transient_commit = store
        .commit(RuntimeCommit {
            expected_projection_revision: Some(current.revision),
            projection: invalid_projection,
            operation: None,
            operation_terminals: Vec::new(),
            records: vec![transient],
            outbox: Vec::new(),
            terminal_application_effects: Vec::new(),
            context_activation_outbox: Vec::new(),
            context_preparation_work_items: Vec::new(),
            context_checkpoints: Vec::new(),
            context_candidates: Vec::new(),
            context_activations: Vec::new(),
            context_head: None,
            hook_plan_binding: None,
            hook_runs: Vec::new(),
            hook_effects: Vec::new(),
            quarantine: Vec::new(),
        })
        .await;
    assert!(matches!(
        transient_commit,
        Err(RuntimeStoreError::Unavailable(_))
    ));
    assert_eq!(
        store
            .journal_records_after(
                &thread_id,
                Some(EventSequence(first_sequence.saturating_sub(1))),
            )
            .await
            .expect("read durable journal after transient rejection")
            .records,
        records
    );
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
            LifecycleCapability::SurfaceAdopt,
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
    Arc<RuntimeStoreFixture>,
    ManagedAgentRuntime<RuntimeStoreFixture>,
) {
    let store = Arc::new(RuntimeStoreFixture::default());
    let runtime =
        ManagedAgentRuntime::new(store.clone(), Arc::new(TestTerminalPresentationProjector))
            .with_surface_validator(Arc::new(AllowSurface));
    (store, runtime)
}

struct AllowSurface;

#[async_trait::async_trait]
impl agentdash_agent_runtime::RuntimeSurfaceReferenceValidator for AllowSurface {
    async fn validate_surface_reference(
        &self,
        _binding_id: &RuntimeBindingId,
        _runtime_thread_id: &RuntimeThreadId,
        _target: &RuntimeSurfaceDescriptor,
    ) -> Result<(), String> {
        Ok(())
    }
}

fn command(
    operation: &str,
    key: &str,
    expected: Option<u64>,
    command: RuntimeCommand,
) -> RuntimeCommandEnvelope {
    RuntimeCommandEnvelope {
        presentation: Vec::new(),
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
            thread_id: id("thread-source-1"),
            presentation_thread_id: id("presentation-thread-1"),
            presentation_turn_id: None,
            binding_id: id("binding-1"),
            driver_generation: RuntimeDriverGeneration(7),
            source_thread_id: id("source-1"),
            profile_digest: id("profile-1"),
            bound_profile: Box::new(profile()),
            input: Vec::new(),
            surface: Box::new(RuntimeSurfaceDescriptor {
                source_frame_id: "frame-1".to_string(),
                surface_revision: SurfaceRevision(1),
                surface_digest: id("surface-1"),
                vfs_digest: "vfs-1".to_string(),
                context_recipe_revision: ContextRecipeRevision(1),
                context_digest: id("context-1"),
                settings_revision: ThreadSettingsRevision(0),
                tool_set_revision: ToolSetRevision(0),
                tool_set_digest: "tools-1".to_string(),
                hook_plan: BoundRuntimeHookPlan {
                    revision: HookPlanRevision(1),
                    digest: id("hook-plan-empty-1"),
                    entries: Vec::new(),
                },
                terminal_hook_effect_binding: None,
            }),
            settings_revision: ThreadSettingsRevision(0),
        },
    )
}

#[tokio::test]
async fn driver_acceptance_closes_delivery_only_operation_idempotently() {
    let (_store, runtime) = fixture();
    let receipt = runtime
        .execute(start())
        .await
        .expect("accept delivery-only empty thread start");
    assert!(
        !runtime
            .complete_driver_dispatch_operation(
                &receipt.thread_id.expect("thread id"),
                &receipt.operation_id
            )
            .await
            .expect("first driver acceptance terminal")
    );
    assert!(
        runtime
            .complete_driver_dispatch_operation(&id("thread-source-1"), &receipt.operation_id)
            .await
            .expect("duplicate driver acceptance terminal")
    );
    let RuntimeSnapshotResult::Operation { operation } = runtime
        .snapshot(RuntimeSnapshotQuery::Operation {
            operation_id: receipt.operation_id,
        })
        .await
        .expect("operation snapshot")
    else {
        panic!("expected operation snapshot")
    };
    assert_eq!(
        operation.terminal,
        Some(RuntimeOperationTerminal::Succeeded)
    );
}

fn driver(event: RuntimeEvent) -> DriverEventEnvelope {
    driver_facts(vec![RuntimeJournalFact::Internal(event)])
}

fn driver_facts(facts: Vec<RuntimeJournalFact>) -> DriverEventEnvelope {
    let operation_id = facts.iter().find_map(|fact| match fact {
        RuntimeJournalFact::Internal(
            RuntimeEvent::TurnStarted { turn_id, .. } | RuntimeEvent::TurnTerminal { turn_id, .. },
        ) => turn_id.as_str().strip_prefix("turn-").and_then(|value| {
            agentdash_agent_runtime_contract::RuntimeOperationId::new(value).ok()
        }),
        _ => None,
    });
    DriverEventEnvelope {
        binding_id: id("binding-1"),
        generation: RuntimeDriverGeneration(7),
        operation_id,
        source_thread_id: id("source-1"),
        source_turn_id: None,
        source_item_id: None,
        source_request_id: None,
        source_entry_index: None,
        facts,
    }
}

fn session_meta_presentation(
    label: &str,
    durability: PresentationDurability,
) -> ImmutablePresentationEvent {
    ImmutablePresentationEvent::new(
        durability,
        agentdash_agent_protocol::BackboneEvent::Platform(
            agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                key: "ordering_fixture".into(),
                value: serde_json::json!({"label": label}),
            },
        ),
    )
}

fn thread_name_presentation(
    thread_id: &str,
    thread_name: Option<&str>,
    durability: PresentationDurability,
) -> ImmutablePresentationEvent {
    ImmutablePresentationEvent::new(
        durability,
        BackboneEvent::ThreadNameUpdated(
            agentdash_agent_protocol::codex_app_server_protocol::ThreadNameUpdatedNotification {
                thread_id: thread_id.to_string(),
                thread_name: thread_name.map(str::to_string),
            },
        ),
    )
}

#[derive(Default)]
struct RecordingCommittedPresentationObserver {
    projection_changes: Mutex<Vec<bool>>,
}

#[async_trait::async_trait]
impl agentdash_agent_runtime::RuntimeCommittedPresentationObserver
    for RecordingCommittedPresentationObserver
{
    async fn observe(
        &self,
        presentation: agentdash_agent_runtime::CommittedDurablePresentation,
    ) -> Result<(), String> {
        self.projection_changes
            .lock()
            .expect("recording observer lock")
            .push(presentation.projection_changed);
        Ok(())
    }
}

#[tokio::test]
async fn standard_thread_name_projects_set_duplicate_and_clear_after_durable_commit() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let observer = Arc::new(RecordingCommittedPresentationObserver::default());
    let runtime =
        ManagedAgentRuntime::new(store.clone(), Arc::new(TestTerminalPresentationProjector))
            .with_surface_validator(Arc::new(AllowSurface))
            .with_committed_presentation_observer(observer.clone());
    let thread_id = runtime
        .execute(start())
        .await
        .expect("start Runtime thread")
        .thread_id
        .expect("thread id");
    let replay_base = store
        .load_thread(&thread_id)
        .await
        .expect("load replay base")
        .expect("runtime thread");

    for (name, expected) in [
        (Some("修复登录态"), Some("修复登录态")),
        (Some("修复登录态"), Some("修复登录态")),
        (None, None),
    ] {
        assert!(matches!(
            runtime
                .ingest_driver_event(driver_facts(vec![RuntimeJournalFact::Presentation(
                    thread_name_presentation("source-1", name, PresentationDurability::Durable,)
                ),]))
                .await
                .expect("commit standard thread name"),
            DriverEventAdmission::Durable { .. }
        ));
        assert_eq!(
            thread_snapshot(&runtime, thread_id.clone())
                .await
                .thread_name
                .as_deref(),
            expected
        );
    }
    let mut replayed = replay_base;
    for record in store
        .journal_records_after(&thread_id, None)
        .await
        .expect("load durable name journal")
        .records
        .iter()
        .filter(|record| {
            matches!(
                record.fact(),
                RuntimeJournalFact::Presentation(ImmutablePresentationEvent {
                    event: BackboneEvent::ThreadNameUpdated(_),
                    ..
                })
            )
        })
    {
        replayed
            .apply_journal_record(record)
            .expect("replay accepted thread name fact");
    }
    assert_eq!(
        replayed.thread_name,
        store
            .load_thread(&thread_id)
            .await
            .expect("load committed state")
            .expect("runtime thread")
            .thread_name,
        "durable journal replay must converge to the committed projection"
    );

    assert_eq!(
        observer
            .projection_changes
            .lock()
            .expect("recording observer lock")
            .as_slice(),
        &[true, false, true]
    );
}

#[tokio::test]
async fn invalid_thread_name_source_or_blank_rolls_back_staged_prefix_and_quarantines() {
    for invalid in [
        thread_name_presentation(
            "another-source",
            Some("valid-looking-title"),
            PresentationDurability::Durable,
        ),
        thread_name_presentation("source-1", Some(" \n "), PresentationDurability::Durable),
    ] {
        let (store, runtime) = fixture();
        let thread_id = runtime
            .execute(start())
            .await
            .expect("start Runtime thread")
            .thread_id
            .expect("thread id");
        let admission = runtime
            .ingest_driver_event(driver_facts(vec![
                RuntimeJournalFact::Internal(RuntimeEvent::ConversationError {
                    turn_id: None,
                    error: RuntimeConversationError {
                        code: Some("staged-name-prefix".into()),
                        message: "must roll back".into(),
                        retryable: true,
                        details: None,
                    },
                }),
                RuntimeJournalFact::Presentation(invalid),
            ]))
            .await
            .expect("persist critical invalid name violation");

        assert!(matches!(
            admission,
            DriverEventAdmission::Terminalized { .. }
        ));
        let snapshot = thread_snapshot(&runtime, thread_id.clone()).await;
        assert_eq!(snapshot.status, RuntimeThreadStatus::Lost);
        assert_eq!(snapshot.thread_name, None);
        assert_eq!(store.quarantined().await.len(), 1);
        assert!(
            store
                .journal_records_after(&thread_id, None)
                .await
                .expect("journal after invalid name")
                .records
                .iter()
                .all(|record| !matches!(
                    record.fact(),
                    RuntimeJournalFact::Internal(RuntimeEvent::ConversationError { error, .. })
                        if error.code.as_deref() == Some("staged-name-prefix")
                ))
        );
    }
}

#[tokio::test]
async fn ephemeral_thread_name_is_a_critical_protocol_violation() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("start Runtime thread")
        .thread_id
        .expect("thread id");

    let admission = runtime
        .ingest_driver_event(driver_facts(vec![RuntimeJournalFact::Presentation(
            thread_name_presentation(
                "source-1",
                Some("瞬时标题"),
                PresentationDurability::Ephemeral,
            ),
        )]))
        .await
        .expect("persist critical ephemeral name violation");

    assert!(matches!(
        admission,
        DriverEventAdmission::Terminalized { .. }
    ));
    let snapshot = thread_snapshot(&runtime, thread_id).await;
    assert_eq!(snapshot.status, RuntimeThreadStatus::Lost);
    assert_eq!(snapshot.thread_name, None);
    assert_eq!(store.quarantined().await.len(), 1);
}

async fn thread_snapshot(
    runtime: &ManagedAgentRuntime<RuntimeStoreFixture>,
    thread_id: RuntimeThreadId,
) -> RuntimeSnapshot {
    match runtime
        .snapshot(RuntimeSnapshotQuery::Thread {
            thread_id: thread_id.clone(),
            at_revision: None,
        })
        .await
        .expect("snapshot")
    {
        RuntimeSnapshotResult::Thread { snapshot } => *snapshot,
        RuntimeSnapshotResult::Operation { .. } | RuntimeSnapshotResult::Context { .. } => {
            panic!("expected thread snapshot")
        }
    }
}

fn adopted_surface() -> RuntimeSurfaceDescriptor {
    RuntimeSurfaceDescriptor {
        source_frame_id: "frame-2".to_string(),
        surface_revision: SurfaceRevision(2),
        surface_digest: id("surface-2"),
        vfs_digest: "vfs-2".to_string(),
        context_recipe_revision: ContextRecipeRevision(2),
        context_digest: id("context-2"),
        settings_revision: ThreadSettingsRevision(1),
        tool_set_revision: ToolSetRevision(1),
        tool_set_digest: "tools-2".to_string(),
        hook_plan: BoundRuntimeHookPlan {
            revision: HookPlanRevision(2),
            digest: id("hook-plan-empty-2"),
            entries: Vec::new(),
        },
        terminal_hook_effect_binding: Some(terminal_effect_binding()),
    }
}

fn surface_adoption_plan() -> RuntimeSurfacePresentationPlan {
    let entity = |fingerprint: &str| NormalizedSurfaceEntity {
        fingerprint: fingerprint.to_string(),
    };
    let companion = |key: &str, display_name: &str| RuntimeCompanionAgentEntry {
        agent_key: key.to_string(),
        executor: "native".to_string(),
        display_name: display_name.to_string(),
        context_usage_kind: Some("agents".to_string()),
    };
    let memory = |key: &str, revision: &str| RuntimeMemorySourceEntry {
        provider_key: "project".to_string(),
        source_key: key.to_string(),
        display_name: key.to_string(),
        source_uri: format!("agentdash://memory/{key}"),
        index_uri: format!("agentdash://memory/{key}/index"),
        mount_id: "workspace".to_string(),
        scope: "project".to_string(),
        index_status: "ready".to_string(),
        trust_level: "trusted".to_string(),
        revision: revision.to_string(),
        summary: None,
        context_usage_kind: Some("memory".to_string()),
    };
    let skill = |name: &str, description: &str| RuntimeSkillEntry {
        name: name.to_string(),
        capability_key: format!("skill:{name}"),
        provider_key: "builtin".to_string(),
        local_name: name.to_string(),
        display_name: None,
        description: description.to_string(),
        file_path: format!("skills/{name}/SKILL.md"),
        base_dir: None,
        exposure: SkillContextExposure::DefaultExposed,
        disable_model_invocation: false,
        context_usage_kind: Some("skills".to_string()),
    };
    let previous_state = NormalizedContextSurfaceState {
        capability_keys: BTreeSet::from(["file_read".to_string()]),
        excluded_tool_paths: BTreeSet::from(["file_read::grep".to_string()]),
        mcp_servers: BTreeMap::from([("server-a".to_string(), entity("old"))]),
        companion_agents: BTreeMap::from([(
            "reviewer".to_string(),
            companion("reviewer", "Reviewer"),
        )]),
        companion_agent_order: vec!["reviewer".to_string()],
        vfs_mounts: BTreeMap::from([("workspace".to_string(), entity("old"))]),
        default_vfs_mount: Some("workspace".to_string()),
        memory_sources: BTreeMap::from([("project:old".to_string(), memory("old", "1"))]),
        memory_source_order: vec!["project:old".to_string()],
        skills: BTreeMap::from([("skill:review".to_string(), skill("review", "old"))]),
        skill_clusters: vec![NormalizedSkillCluster {
            provider_key: "builtin".to_string(),
            display_name: "Builtin".to_string(),
            model_summary: Some("Project skills".to_string()),
        }],
        assignment: Some(NormalizedAssignmentContext {
            fragments: vec![RuntimeContextFragmentEntry {
                slot: "task".to_string(),
                label: "Task".to_string(),
                source: "workflow".to_string(),
                content: "Review".to_string(),
                context_usage_kind: Some("system_developer".to_string()),
            }],
        }),
        ..Default::default()
    };
    let mut target_state = previous_state.clone();
    target_state
        .capability_keys
        .extend(["collaboration".to_string(), "file_write".to_string()]);
    target_state.excluded_tool_paths.clear();
    target_state
        .included_tool_paths
        .insert("file_write::apply_patch".to_string());
    target_state
        .mcp_servers
        .insert("server-a".to_string(), entity("changed"));
    target_state
        .mcp_servers
        .insert("server-b".to_string(), entity("added"));
    target_state.unavailable_mcp_servers = vec![NormalizedMcpServerReadiness {
        name: "server-b".to_string(),
        reason_code: "connection_failed".to_string(),
        message: "connection refused".to_string(),
    }];
    target_state.companion_agents.insert(
        "reviewer".to_string(),
        companion("reviewer", "Senior Reviewer"),
    );
    target_state
        .companion_agents
        .insert("builder".to_string(), companion("builder", "Builder"));
    target_state.companion_agent_order = vec!["builder".to_string(), "reviewer".to_string()];
    target_state.vfs_mounts.remove("workspace");
    target_state
        .vfs_mounts
        .insert("project".to_string(), entity("added"));
    target_state.default_vfs_mount = Some("project".to_string());
    target_state.memory_sources.remove("project:old");
    target_state
        .memory_sources
        .insert("project:new".to_string(), memory("new", "2"));
    target_state.memory_source_order = vec!["project:new".to_string()];
    target_state
        .skills
        .insert("skill:review".to_string(), skill("review", "changed"));
    target_state
        .skills
        .insert("skill:test".to_string(), skill("test", "added"));
    target_state.tool_schemas.insert(
        "apply_patch".to_string(),
        RuntimeToolSchemaEntry {
            name: "apply_patch".to_string(),
            description: "Apply a patch".to_string(),
            parameters_schema: serde_json::json!({"type": "object"}),
            capability_key: Some("file_write".to_string()),
            source: Some("workspace".to_string()),
            tool_path: Some("file_write::apply_patch".to_string()),
            context_usage_kind: Some("system_tools".to_string()),
        },
    );
    target_state.assignment = Some(NormalizedAssignmentContext {
        fragments: vec![RuntimeContextFragmentEntry {
            slot: "task".to_string(),
            label: "Task".to_string(),
            source: "workflow".to_string(),
            content: "# Implement\nShip it".to_string(),
            context_usage_kind: Some("system_developer".to_string()),
        }],
    });
    let compile = |revision: u64, normalized_context_surface| {
        AgentSurfaceCompiler
            .compile_business_facts(BusinessAgentSurfaceFacts {
                revision: SurfaceRevision(revision),
                context_recipe: ContextRecipe {
                    revision: ContextRecipeRevision(revision),
                    provenance: ContextProvenance {
                        settings_revision: ThreadSettingsRevision(1),
                        tool_set_revision: ToolSetRevision(revision),
                    },
                    source_item_ids: Vec::new(),
                },
                tool_set_revision: ToolSetRevision(revision),
                hook_plan_revision: HookPlanRevision(1),
                workspace: WorkspaceRequirement {
                    capabilities: BTreeSet::new(),
                    minimum_mechanism: DeliveryMechanism::HostAdaptedExact,
                    requirement: ContributionRequirement::Required,
                },
                source: SurfaceSourceRef {
                    layer: "workflow".into(),
                    key: "apply".into(),
                },
                transition_phase_node: Some("apply".into()),
                instructions: Vec::new(),
                tools: vec![ToolContribution {
                    meta: ContributionMeta {
                        key: "tool:read".into(),
                        source: SurfaceSourceRef {
                            layer: "platform".into(),
                            key: "workspace".into(),
                        },
                        priority: 1,
                        requirement: ContributionRequirement::Required,
                    },
                    runtime_name: "read".into(),
                    description: "Read a workspace file".into(),
                    parameters_schema: serde_json::json!({
                        "type": "object",
                        "properties": { "path": { "type": "string", "description": "file path" } },
                        "required": ["path"]
                    }),
                    capability_key: "file_read".into(),
                    tool_path: "file_read::read".into(),
                    allowed_channels: BTreeSet::from([ToolChannel::DirectCallback]),
                    configuration_boundary: ConfigurationBoundary::Binding,
                    protocol_projection: ToolProtocolProjection::FsRead,
                    presentation_emitter: ToolPresentationEmitter::ToolBroker,
                    parity_fixture_id: "main_tool_read".into(),
                }],
                hooks: Vec::new(),
                bootstrap_context: Default::default(),
                normalized_context_surface,
                projection_identity: ContextProjectionIdentity {
                    operation_id: format!("surface-{revision}"),
                    source_frame_id: "apply".into(),
                    source_frame_revision: revision,
                    recorded_at_ms: 11,
                },
            })
            .unwrap()
    };
    let previous = compile(1, previous_state);
    let target = compile(2, target_state);
    RuntimeSurfacePresentationPlan::for_adoption(&previous.snapshot, &target)
}

fn terminal_effect_binding() -> RuntimeTerminalHookEffectBinding {
    RuntimeTerminalHookEffectBinding {
        handler: RuntimeTerminalHookEffectHandlerRef {
            handler_type: id("agent_run_terminal_control"),
            handler_id: id("agent-run-terminal-control-v1"),
            revision: RuntimeTerminalHookEffectHandlerRevision(7),
        },
        supported_effect_kinds: BTreeSet::from([id("delivery_convergence"), id("hook_post_turn")]),
    }
}

#[tokio::test]
async fn surface_adopt_is_cas_guarded_and_keeps_active_turn_while_connector_sync_is_queued() {
    let (store, runtime) = fixture();
    runtime
        .execute(start())
        .await
        .expect("start Runtime thread");
    let initial = thread_snapshot(&runtime, id("thread-source-1")).await;

    let stale = runtime
        .execute(command(
            "surface-stale-operation",
            "surface-stale-key",
            Some(initial.revision.0),
            RuntimeCommand::SurfaceAdopt {
                thread_id: initial.thread_id.clone(),
                expected_surface_revision: SurfaceRevision(0),
                expected_surface_digest: initial.surface.surface_digest.clone(),
                target: Box::new(adopted_surface()),
            },
        ))
        .await;
    assert!(matches!(
        stale,
        Err(RuntimeExecuteError::InvalidCommand { .. })
    ));

    let mut adoption = command(
        "surface-success-operation",
        "surface-success-key",
        Some(initial.revision.0),
        RuntimeCommand::SurfaceAdopt {
            thread_id: initial.thread_id.clone(),
            expected_surface_revision: initial.surface.surface_revision,
            expected_surface_digest: initial.surface.surface_digest.clone(),
            target: Box::new(adopted_surface()),
        },
    );
    let adoption_plan = surface_adoption_plan();
    adoption.presentation = adoption_plan.adoption_presentation(
        &id("presentation-thread-1"),
        None,
        "surface-success-operation",
    );
    let receipt = runtime
        .execute(adoption.clone())
        .await
        .expect("adopt idle Runtime surface");
    let replay = runtime.execute(adoption).await.expect("replay adoption");
    assert!(!receipt.duplicate);
    assert!(replay.duplicate);
    let adopted = thread_snapshot(&runtime, initial.thread_id.clone()).await;
    assert_eq!(adopted.surface, adopted_surface());
    assert!(matches!(
        &store.outbox().await.last().expect("surface outbox").command,
        RuntimeCommand::SurfaceAdopt { target, .. } if target.as_ref() == &adopted_surface()
    ));
    let adoption_records = store
        .journal_records_after(&initial.thread_id, None)
        .await
        .unwrap()
        .records
        .into_iter()
        .collect::<Vec<_>>();
    let hook_plan_position = adoption_records
        .iter()
        .position(|record| {
            matches!(
                record.fact(),
                RuntimeJournalFact::Internal(RuntimeEvent::HookPlanBound { .. })
            )
        })
        .expect("surface adoption must bind its hook plan");
    let presentation_position = adoption_records
        .iter()
        .position(|record| record.as_presentation().is_some())
        .expect("surface adoption must append its ContextFrame");
    assert!(hook_plan_position < presentation_position);
    let frames = adoption_records
        .into_iter()
        .filter_map(
            |record| match record.as_presentation().map(|event| &event.event) {
                Some(BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(changed))) => {
                    Some(changed.frame.clone())
                }
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(
        frames, adoption_plan.adoption_frames,
        "Runtime journal must preserve the exact compiled adoption payload and order"
    );
    assert_eq!(frames.len(), 2, "adoption replay must not duplicate frames");
    assert_eq!(frames[0].kind, ContextFrameKind::CapabilityStateDelta);
    assert_eq!(frames[1].kind, ContextFrameKind::AssignmentContext);
    assert!(matches!(
        frames[0].sections.as_slice(),
        [
            ContextFrameSection::CapabilityKeyDelta { .. },
            ContextFrameSection::ToolPathDelta { .. },
            ContextFrameSection::McpServerDelta { .. },
            ContextFrameSection::CompanionAgentRosterDelta { .. },
            ContextFrameSection::VfsDelta { .. },
            ContextFrameSection::MemoryInventory { .. },
            ContextFrameSection::SkillDelta { .. },
            ContextFrameSection::ToolSchemaDelta { .. }
        ]
    ));
    assert!(matches!(
        frames[1].sections.as_slice(),
        [ContextFrameSection::AssignmentContext { .. }]
    ));
    assert_eq!(
        frames[0]
            .rendered_text
            .matches("Step Transition: apply")
            .count(),
        8
    );
    assert!(frames[0].rendered_text.contains("connection refused"));
    assert!(frames[1].rendered_text.contains("## Implement"));

    runtime
        .execute(command(
            "surface-turn-operation",
            "surface-turn-key",
            Some(adopted.revision.0),
            RuntimeCommand::TurnStart {
                thread_id: adopted.thread_id.clone(),
                presentation_turn_id: id("presentation-turn-486"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("start active turn");
    let active = thread_snapshot(&runtime, adopted.thread_id.clone()).await;
    assert_eq!(
        active.active_presentation_turn_id,
        Some(id("presentation-turn-486"))
    );
    let active_receipt = runtime
        .execute(command(
            "surface-active-operation",
            "surface-active-key",
            Some(active.revision.0),
            RuntimeCommand::SurfaceAdopt {
                thread_id: active.thread_id,
                expected_surface_revision: active.surface.surface_revision,
                expected_surface_digest: active.surface.surface_digest,
                target: Box::new(RuntimeSurfaceDescriptor {
                    surface_revision: SurfaceRevision(3),
                    surface_digest: id("surface-3"),
                    source_frame_id: "frame-3".to_string(),
                    ..adopted_surface()
                }),
            },
        ))
        .await
        .expect("canonical surface adoption is independent from connector turn boundary");
    assert!(!active_receipt.duplicate);
    let active_adopted = thread_snapshot(&runtime, id("thread-source-1")).await;
    assert_eq!(
        active_adopted.active_turn_id, active.active_turn_id,
        "platform surface facts must not interrupt the active turn"
    );
    assert_eq!(active_adopted.surface.surface_revision, SurfaceRevision(3));
    assert!(matches!(
        &store.outbox().await.last().expect("active surface outbox").command,
        RuntimeCommand::SurfaceAdopt { target, .. }
            if target.surface_revision == SurfaceRevision(3)
    ));
}

#[tokio::test]
async fn tool_set_replace_keeps_current_and_target_revisions_distinct() {
    let (store, runtime) = fixture();
    runtime
        .execute(start())
        .await
        .expect("start runtime thread");
    let initial = thread_snapshot(&runtime, id("thread-source-1")).await;

    runtime
        .execute(command(
            "tool-set-replace-operation",
            "tool-set-replace-key",
            Some(initial.revision.0),
            RuntimeCommand::ToolSetReplace {
                thread_id: initial.thread_id.clone(),
                expected_current_tool_set_revision: ToolSetRevision(0),
                target_tool_set_revision: ToolSetRevision(4),
                tool_set_digest: "tools-4".to_string(),
            },
        ))
        .await
        .expect("replace tool set");

    let replaced = thread_snapshot(&runtime, initial.thread_id).await;
    assert_eq!(replaced.tool_set_revision, ToolSetRevision(4));
    assert_eq!(replaced.surface.tool_set_revision, ToolSetRevision(4));
    assert_eq!(replaced.surface.tool_set_digest, "tools-4");
    assert!(matches!(
        &store.outbox().await.last().expect("tool replace outbox").command,
        RuntimeCommand::ToolSetReplace {
            expected_current_tool_set_revision: ToolSetRevision(0),
            target_tool_set_revision: ToolSetRevision(4),
            tool_set_digest,
            ..
        } if tool_set_digest == "tools-4"
    ));
}

#[tokio::test]
async fn native_surface_adopt_commits_context_frames_and_lowers_driver_sync_to_tool_replace() {
    let (store, runtime) = fixture();
    let mut native_start = start();
    let RuntimeCommand::ThreadStart { bound_profile, .. } = &mut native_start.command else {
        unreachable!()
    };
    bound_profile
        .lifecycle
        .remove(&LifecycleCapability::SurfaceAdopt);
    runtime
        .execute(native_start)
        .await
        .expect("start native runtime thread");
    let initial = thread_snapshot(&runtime, id("thread-source-1")).await;

    runtime
        .execute(command(
            "native-active-turn-operation",
            "native-active-turn-key",
            Some(initial.revision.0),
            RuntimeCommand::TurnStart {
                thread_id: initial.thread_id.clone(),
                presentation_turn_id: id("presentation-turn-native-surface"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("start native active turn");
    let active = thread_snapshot(&runtime, initial.thread_id.clone()).await;
    let mut adoption = command(
        "native-surface-operation",
        "native-surface-key",
        Some(active.revision.0),
        RuntimeCommand::SurfaceAdopt {
            thread_id: active.thread_id.clone(),
            expected_surface_revision: active.surface.surface_revision,
            expected_surface_digest: active.surface.surface_digest.clone(),
            target: Box::new(adopted_surface()),
        },
    );
    let adoption_plan = surface_adoption_plan();
    adoption.presentation = adoption_plan.adoption_presentation(
        &id("presentation-thread-1"),
        active.active_presentation_turn_id.as_ref(),
        "native-surface-operation",
    );

    runtime
        .execute(adoption)
        .await
        .expect("platform-owned surface adoption during native turn");

    let adopted = thread_snapshot(&runtime, initial.thread_id.clone()).await;
    assert_eq!(adopted.surface, adopted_surface());
    assert!(matches!(
        &store.outbox().await.last().expect("native surface outbox").command,
        RuntimeCommand::ToolSetReplace {
            expected_current_tool_set_revision: ToolSetRevision(0),
            target_tool_set_revision: ToolSetRevision(1),
            tool_set_digest,
            ..
        } if tool_set_digest == "tools-2"
    ));
    let frames = store
        .journal_records_after(&initial.thread_id, None)
        .await
        .expect("native surface journal")
        .records
        .into_iter()
        .filter_map(
            |record| match record.as_presentation().map(|event| &event.event) {
                Some(BackboneEvent::Platform(PlatformEvent::ContextFrameChanged(changed))) => {
                    Some(changed.frame.clone())
                }
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(frames, adoption_plan.adoption_frames);
}

#[tokio::test]
async fn thread_start_presentation_identity_matches_input_presence() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let runtime = ManagedAgentRuntime::new(store, Arc::new(TestTerminalPresentationProjector));

    let mut phantom = start();
    let RuntimeCommand::ThreadStart {
        presentation_turn_id,
        ..
    } = &mut phantom.command
    else {
        unreachable!()
    };
    *presentation_turn_id = Some(id("presentation-turn-phantom"));
    assert!(matches!(
        runtime.execute(phantom).await,
        Err(RuntimeExecuteError::InvalidCommand { .. })
    ));

    let mut missing = start();
    let RuntimeCommand::ThreadStart { input, .. } = &mut missing.command else {
        unreachable!()
    };
    *input = vec![RuntimeInput::text("hello".to_string())];
    assert!(matches!(
        runtime.execute(missing).await,
        Err(RuntimeExecuteError::InvalidCommand { .. })
    ));
}

#[tokio::test]
async fn application_transient_append_publishes_complete_ephemeral_body_without_journaling() {
    let (store, runtime) = fixture();
    runtime
        .execute(start())
        .await
        .expect("start Runtime thread");
    runtime
        .execute(command(
            "op-application-transient",
            "key-application-transient",
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: id("thread-source-1"),
                presentation_turn_id: id("presentation-turn-application-transient"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("start Runtime turn");
    let event = ImmutablePresentationEvent::new(
        PresentationDurability::Ephemeral,
        agentdash_agent_protocol::BackboneEvent::Platform(
            agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                key: "hook_trace_ephemeral_fixture".into(),
                value: serde_json::json!({"explicit_null": null, "order": [2, 1]}),
            },
        ),
    );
    let mut live = store.subscribe_presentation(&id("thread-source-1")).await;

    runtime
        .append_transient_presentation(RuntimeTransientPresentationAppendRequest {
            runtime_thread_id: id("thread-source-1"),
            producer: "application.hook_trace".into(),
            events: vec![RuntimePresentationInput {
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: Some(id("turn-op-application-transient")),
                    presentation_turn_id: Some(id("presentation-turn-application-transient")),
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("presentation-thread-1".into()),
                    source_turn_id: None,
                    source_item_id: None,
                    source_request_id: Some("hook-run-1".into()),
                    source_entry_index: None,
                },
                event: event.clone(),
            }],
        })
        .await
        .expect("append transient presentation");

    let live_record = tokio::time::timeout(std::time::Duration::from_secs(1), live.recv())
        .await
        .expect("transient live presentation timeout")
        .expect("transient live presentation");
    assert_eq!(live_record.as_presentation(), Some(&event));

    let transient = store
        .read_presentation(
            &id("thread-source-1"),
            Some(RuntimeDriverGeneration(7)),
            None,
        )
        .await;
    assert_eq!(transient.len(), 1);
    assert_eq!(transient[0].as_presentation(), Some(&event));
    assert!(transient[0].carrier().sequence.is_none());
    assert!(transient[0].carrier().transient.is_some());
    assert!(
        store
            .journal_records_after(&id("thread-source-1"), None)
            .await
            .expect("durable journal")
            .records
            .iter()
            .all(|record| record.as_presentation() != Some(&event))
    );
}

#[tokio::test]
async fn driver_mixed_presentation_batch_preserves_source_order_live_and_durable_get() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("start Runtime thread")
        .thread_id
        .expect("thread id");
    runtime
        .execute(command(
            "op-mixed-order",
            "key-mixed-order",
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-mixed-order"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("start Runtime turn");
    let runtime_turn_id: RuntimeTurnId = id("turn-op-mixed-order");
    let mut live = store.subscribe_presentation(&thread_id).await;

    runtime
        .ingest_driver_event(driver_facts(vec![
            RuntimeJournalFact::Internal(RuntimeEvent::TurnStarted {
                turn_id: runtime_turn_id,
                presentation_turn_id: id("presentation-turn-mixed-order"),
            }),
            RuntimeJournalFact::Presentation(session_meta_presentation(
                "durable-a",
                PresentationDurability::Durable,
            )),
            RuntimeJournalFact::Presentation(session_meta_presentation(
                "ephemeral-b",
                PresentationDurability::Ephemeral,
            )),
            RuntimeJournalFact::Presentation(session_meta_presentation(
                "durable-c",
                PresentationDurability::Durable,
            )),
        ]))
        .await
        .expect("mixed driver batch");

    let mut live_labels = Vec::new();
    for _ in 0..3 {
        let record = tokio::time::timeout(Duration::from_secs(1), live.recv())
            .await
            .expect("mixed live delivery timeout")
            .expect("mixed live record");
        let RuntimeJournalFact::Presentation(event) = record.fact() else {
            panic!("presentation subscription must contain presentation facts");
        };
        let agentdash_agent_protocol::BackboneEvent::Platform(
            agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { value, .. },
        ) = &event.event
        else {
            panic!("ordering fixture event");
        };
        live_labels.push((
            value["label"].as_str().expect("label").to_string(),
            event.durability,
        ));
    }
    assert_eq!(
        live_labels,
        vec![
            ("durable-a".into(), PresentationDurability::Durable),
            ("ephemeral-b".into(), PresentationDurability::Ephemeral),
            ("durable-c".into(), PresentationDurability::Durable),
        ]
    );

    let durable_labels = store
        .journal_records_after(&thread_id, None)
        .await
        .expect("durable journal")
        .records
        .iter()
        .filter_map(|record| {
            let RuntimeJournalFact::Presentation(event) = record.fact() else {
                return None;
            };
            let agentdash_agent_protocol::BackboneEvent::Platform(
                agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value },
            ) = &event.event
            else {
                return None;
            };
            (key == "ordering_fixture").then(|| value["label"].as_str().unwrap().to_string())
        })
        .collect::<Vec<_>>();
    assert_eq!(durable_labels, vec!["durable-a", "durable-c"]);
}

#[tokio::test]
async fn driver_mixed_batch_commit_failure_leaks_no_transient_or_live_presentation() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("start Runtime thread")
        .thread_id
        .expect("thread id");
    runtime
        .execute(command(
            "op-mixed-failure",
            "key-mixed-failure",
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-mixed-failure"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("start Runtime turn");
    let mut live = store.subscribe_presentation(&thread_id).await;
    store.fail_next_commit_at(CommitFailurePoint::AfterEvents);

    assert!(matches!(
        runtime
            .ingest_driver_event(driver_facts(vec![
                RuntimeJournalFact::Presentation(session_meta_presentation(
                    "durable-before-failure",
                    PresentationDurability::Durable,
                )),
                RuntimeJournalFact::Presentation(session_meta_presentation(
                    "ephemeral-after-failure",
                    PresentationDurability::Ephemeral,
                )),
            ]))
            .await,
        Err(RuntimeExecuteError::Persistence { .. })
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), live.recv())
            .await
            .is_err()
    );
    assert!(
        store
            .read_presentation(&thread_id, Some(RuntimeDriverGeneration(7)), None)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn application_transient_is_rejected_after_terminal_without_reviving_replay() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("start Runtime thread")
        .thread_id
        .expect("thread id");
    runtime
        .execute(command(
            "op-late-transient",
            "key-late-transient",
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-late-transient"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("start Runtime turn");
    let runtime_turn_id: RuntimeTurnId = id("turn-op-late-transient");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::TurnTerminal {
            turn_id: runtime_turn_id.clone(),
            terminal: RuntimeTurnTerminal::Completed,
            message: None,
            diagnostic: None,
        }))
        .await
        .expect("terminal");

    let late = runtime
        .append_transient_presentation(RuntimeTransientPresentationAppendRequest {
            runtime_thread_id: thread_id.clone(),
            producer: "application.hook_trace".into(),
            events: vec![RuntimePresentationInput {
                coordinate: RuntimePresentationCoordinate {
                    runtime_turn_id: Some(runtime_turn_id),
                    presentation_turn_id: Some(id("presentation-turn-late-transient")),
                    runtime_item_id: None,
                    interaction_id: None,
                    source_thread_id: Some("presentation-thread-1".into()),
                    source_turn_id: Some("presentation-turn-late-transient".into()),
                    source_item_id: None,
                    source_request_id: Some("late-hook".into()),
                    source_entry_index: None,
                },
                event: session_meta_presentation("late-hook", PresentationDurability::Ephemeral),
            }],
        })
        .await;
    assert!(matches!(
        late,
        Err(RuntimePresentationAppendError::Invalid(_))
    ));
    assert!(
        store
            .read_presentation(&thread_id, Some(RuntimeDriverGeneration(7)), None)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn concurrent_terminal_and_application_transient_never_publish_transient_after_terminal() {
    let (store, runtime) = fixture();
    let runtime = Arc::new(runtime);
    let thread_id = runtime
        .execute(start())
        .await
        .expect("start Runtime thread")
        .thread_id
        .expect("thread id");
    runtime
        .execute(command(
            "op-transient-terminal-race",
            "key-transient-terminal-race",
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-transient-terminal-race"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("start Runtime turn");
    let runtime_turn_id: RuntimeTurnId = id("turn-op-transient-terminal-race");
    let mut live = store.subscribe_presentation(&thread_id).await;

    let terminal_runtime = runtime.clone();
    let terminal_turn_id = runtime_turn_id.clone();
    let terminal = tokio::spawn(async move {
        terminal_runtime
            .ingest_driver_event(driver_facts(vec![
                RuntimeJournalFact::Presentation(session_meta_presentation(
                    "terminal-marker",
                    PresentationDurability::Durable,
                )),
                RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                    turn_id: terminal_turn_id,
                    terminal: RuntimeTurnTerminal::Completed,
                    message: None,
                    diagnostic: None,
                }),
            ]))
            .await
    });
    let transient_runtime = runtime.clone();
    let transient_thread_id = thread_id.clone();
    let transient = tokio::spawn(async move {
        transient_runtime
            .append_transient_presentation(RuntimeTransientPresentationAppendRequest {
                runtime_thread_id: transient_thread_id,
                producer: "application.hook_trace".into(),
                events: vec![RuntimePresentationInput {
                    coordinate: RuntimePresentationCoordinate {
                        runtime_turn_id: Some(runtime_turn_id),
                        presentation_turn_id: Some(id("presentation-turn-transient-terminal-race")),
                        runtime_item_id: None,
                        interaction_id: None,
                        source_thread_id: Some("presentation-thread-1".into()),
                        source_turn_id: Some("presentation-turn-transient-terminal-race".into()),
                        source_item_id: None,
                        source_request_id: Some("race-hook".into()),
                        source_entry_index: None,
                    },
                    event: session_meta_presentation(
                        "race-transient",
                        PresentationDurability::Ephemeral,
                    ),
                }],
            })
            .await
    });
    terminal
        .await
        .expect("terminal task")
        .expect("terminal ingestion");
    let transient_result = transient.await.expect("transient task");

    let mut labels = Vec::new();
    while let Ok(Ok(record)) = tokio::time::timeout(Duration::from_millis(50), live.recv()).await {
        let RuntimeJournalFact::Presentation(event) = record.fact() else {
            continue;
        };
        let agentdash_agent_protocol::BackboneEvent::Platform(
            agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value },
        ) = &event.event
        else {
            continue;
        };
        if key == "ordering_fixture" {
            labels.push(value["label"].as_str().expect("label").to_string());
        }
    }
    let terminal_index = labels
        .iter()
        .position(|label| label == "terminal-marker")
        .expect("terminal marker live publication");
    if transient_result.is_ok() {
        let transient_index = labels
            .iter()
            .position(|label| label == "race-transient")
            .expect("accepted transient must publish");
        assert!(transient_index < terminal_index, "live order: {labels:?}");
    } else {
        assert!(!labels.iter().any(|label| label == "race-transient"));
    }
    assert!(
        store
            .read_presentation(&thread_id, Some(RuntimeDriverGeneration(7)), None)
            .await
            .is_empty(),
        "terminal clear must remove any transient accepted before the terminal"
    );
}

#[tokio::test]
async fn terminal_application_effect_is_atomic_and_retries_through_typed_lease() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("start Runtime thread")
        .thread_id
        .expect("thread id");
    runtime
        .execute(command(
            "op-terminal-effect",
            "key-terminal-effect",
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-terminal-effect"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("start Runtime turn");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::TurnTerminal {
            turn_id: id("turn-op-terminal-effect"),
            terminal: RuntimeTurnTerminal::Completed,
            message: Some("done".into()),
            diagnostic: None,
        }))
        .await
        .expect("terminal");

    let effects = store.terminal_application_effects().await;
    assert_eq!(effects.len(), 1);
    let effect = &effects[0];
    assert_eq!(effect.runtime_thread_id, thread_id);
    assert_eq!(effect.runtime_turn_id, id("turn-op-terminal-effect"));
    assert_eq!(
        effect.presentation_turn_id,
        id("presentation-turn-terminal-effect")
    );
    assert_eq!(effect.binding_id, id("binding-1"));
    assert_eq!(effect.driver_generation, RuntimeDriverGeneration(7));
    assert_eq!(effect.surface_revision, SurfaceRevision(1));
    assert_eq!(effect.surface_digest, id("surface-1"));
    let terminal_record = store
        .journal_records_after(&thread_id, None)
        .await
        .expect("journal")
        .records
        .into_iter()
        .find(|record| record.carrier().sequence == Some(effect.terminal_event_sequence))
        .expect("terminal presentation record");
    assert!(matches!(
        terminal_record.fact(),
        RuntimeJournalFact::Presentation(event)
            if matches!(
                &event.event,
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, .. }
                ) if key == "turn_terminal"
            )
    ));

    let request = RuntimeTerminalApplicationEffectClaimRequest {
        owner: RuntimeWorkerId("terminal-worker".into()),
        lease_duration_ms: 30_000,
        limit: 1,
    };
    let first = store
        .claim_terminal_application_effects(request.clone())
        .await
        .expect("first claim")
        .pop()
        .expect("terminal work");
    assert_eq!(first.attempt, 1);
    store
        .release_terminal_application_effect(&first, "retry".into())
        .await
        .expect("release");
    let second = store
        .claim_terminal_application_effects(request)
        .await
        .expect("second claim")
        .pop()
        .expect("retried terminal work");
    assert_eq!(second.attempt, 2);
    assert!(matches!(
        store.ack_terminal_application_effect(&first).await,
        Err(RuntimeStoreError::WorkClaimConflict)
    ));
    store
        .ack_terminal_application_effect(&second)
        .await
        .expect("ack terminal work");
    assert!(
        store
            .claim_terminal_application_effects(RuntimeTerminalApplicationEffectClaimRequest {
                owner: RuntimeWorkerId("terminal-worker".into()),
                lease_duration_ms: 30_000,
                limit: 1,
            })
            .await
            .expect("claim after ack")
            .is_empty()
    );
}

#[tokio::test]
async fn terminal_application_effect_freezes_the_exact_adopted_surface_binding() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("start Runtime thread")
        .thread_id
        .expect("thread id");
    let initial = thread_snapshot(&runtime, thread_id.clone()).await;
    runtime
        .execute(command(
            "op-adopt-terminal-binding",
            "key-adopt-terminal-binding",
            Some(initial.revision.0),
            RuntimeCommand::SurfaceAdopt {
                thread_id: thread_id.clone(),
                expected_surface_revision: initial.surface.surface_revision,
                expected_surface_digest: initial.surface.surface_digest,
                target: Box::new(adopted_surface()),
            },
        ))
        .await
        .expect("adopt surface with terminal binding");
    let adopted = thread_snapshot(&runtime, thread_id.clone()).await;
    runtime
        .execute(command(
            "op-terminal-adopted-binding",
            "key-terminal-adopted-binding",
            Some(adopted.revision.0),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-adopted-binding"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("start Runtime turn");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::TurnTerminal {
            turn_id: id("turn-op-terminal-adopted-binding"),
            terminal: RuntimeTurnTerminal::Completed,
            message: None,
            diagnostic: None,
        }))
        .await
        .expect("terminal");

    let effects = store.terminal_application_effects().await;
    let effect = effects.last().expect("terminal application effect");
    assert_eq!(effect.surface_revision, adopted_surface().surface_revision);
    assert_eq!(effect.surface_digest, adopted_surface().surface_digest);
    assert_eq!(
        effect.terminal_hook_effect_binding,
        Some(terminal_effect_binding())
    );
}

struct RejectTerminalPresentationProjector;

impl RuntimeApplicationPresentationProjector for RejectTerminalPresentationProjector {
    fn project_terminal(
        &self,
        _context: RuntimeTerminalPresentationContext,
    ) -> Result<Vec<RuntimePresentationInput>, RuntimeApplicationPresentationProjectionError> {
        Err(RuntimeApplicationPresentationProjectionError::Invalid(
            "terminal projection rejected by test projector".into(),
        ))
    }
}

#[tokio::test]
async fn terminal_commit_and_projection_failures_create_no_effect_work() {
    async fn active_turn(
        runtime: &ManagedAgentRuntime<RuntimeStoreFixture>,
        operation: &str,
    ) -> RuntimeThreadId {
        let thread_id = runtime
            .execute(start())
            .await
            .expect("start Runtime thread")
            .thread_id
            .expect("thread id");
        runtime
            .execute(command(
                operation,
                &format!("key-{operation}"),
                Some(3),
                RuntimeCommand::TurnStart {
                    thread_id: thread_id.clone(),
                    presentation_turn_id: id(&format!("presentation-{operation}")),
                    input: Vec::new(),
                },
            ))
            .await
            .expect("start Runtime turn");
        thread_id
    }

    let (store, runtime) = fixture();
    let thread_id = active_turn(&runtime, "op-terminal-effect-failure").await;
    store.fail_next_commit_at(CommitFailurePoint::AfterOutbox);
    assert!(matches!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::TurnTerminal {
                turn_id: id("turn-op-terminal-effect-failure"),
                terminal: RuntimeTurnTerminal::Completed,
                message: None,
                diagnostic: None,
            }))
            .await,
        Err(RuntimeExecuteError::Persistence { .. })
    ));
    assert!(store.terminal_application_effects().await.is_empty());
    assert!(
        store
            .journal_records_after(&thread_id, None)
            .await
            .expect("journal")
            .records
            .iter()
            .all(|record| !is_turn_terminal_record(record))
    );

    let missing_store = Arc::new(RuntimeStoreFixture::default());
    let missing_runtime = ManagedAgentRuntime::new(
        missing_store.clone(),
        Arc::new(RejectTerminalPresentationProjector),
    );
    active_turn(&missing_runtime, "op-terminal-effect-missing").await;
    assert!(matches!(
        missing_runtime
            .ingest_driver_event(driver(RuntimeEvent::TurnTerminal {
                turn_id: id("turn-op-terminal-effect-missing"),
                terminal: RuntimeTurnTerminal::Completed,
                message: None,
                diagnostic: None,
            }))
            .await,
        Err(RuntimeExecuteError::InvalidCommand { .. })
    ));
    assert!(
        missing_store
            .terminal_application_effects()
            .await
            .is_empty()
    );
}

fn is_turn_terminal_record(record: &RuntimeJournalRecord) -> bool {
    matches!(
        record.fact(),
        RuntimeJournalFact::Presentation(event)
            if matches!(
                &event.event,
                agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, .. }
                ) if key == "turn_terminal"
            )
    )
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
    assert!(
        store
            .load_hook_plan(&id("thread-source-1"))
            .await
            .expect("read hook plan")
            .is_none()
    );

    let receipt = runtime.execute(start()).await.expect("accepted");
    assert_eq!(receipt.operation_sequence.0, 1);
    let outbox = store.outbox().await;
    assert_eq!(outbox.len(), 1);
    assert_eq!(
        outbox[0].presentation_thread_id,
        id("presentation-thread-1")
    );
    let projection = store
        .load_thread(&id("thread-source-1"))
        .await
        .expect("load projection")
        .expect("projection");
    assert_eq!(
        projection.presentation_thread_id,
        id("presentation-thread-1")
    );
    let events = store
        .internal_events_after(&id("thread-source-1"), None)
        .await
        .expect("events")
        .events;
    assert_eq!(events.len(), 3);
    assert!(matches!(
        events[0].event,
        RuntimeEvent::OperationAccepted { .. }
    ));
    let hook_plan = store
        .load_hook_plan(&id("thread-source-1"))
        .await
        .expect("read hook plan")
        .expect("hook plan committed with ThreadStart");
    assert_eq!(hook_plan.plan.revision, HookPlanRevision(1));
}

#[tokio::test]
async fn thread_start_with_initial_input_owns_the_canonical_turn() {
    let (store, runtime) = fixture();
    let mut start = start();
    let RuntimeCommand::ThreadStart {
        input,
        presentation_turn_id,
        ..
    } = &mut start.command
    else {
        unreachable!("fixture is ThreadStart");
    };
    *presentation_turn_id = Some(id("presentation-turn-thread-start-input"));
    input.push(RuntimeInput::text("hello".to_string()));

    let thread_id = runtime
        .execute(start)
        .await
        .expect("start with input")
        .thread_id
        .expect("thread id");
    let snapshot = thread_snapshot(&runtime, thread_id).await;
    assert_eq!(snapshot.revision, RuntimeRevision(6));
    assert_eq!(snapshot.latest_event_sequence, EventSequence(6));
    assert_eq!(snapshot.active_turn_id, Some(id("turn-op-1")));
    assert_eq!(
        snapshot.active_presentation_turn_id,
        Some(id("presentation-turn-thread-start-input"))
    );
    let state = store
        .load_thread(&snapshot.thread_id)
        .await
        .expect("load canonical thread")
        .expect("canonical thread");
    assert_eq!(state.items.len(), 1);
    let user = state.items.values().next().expect("canonical user item");
    let agentdash_agent_runtime::EntityPhase::Terminal(RuntimeItemTerminal::Completed {
        final_content,
    }) = &user.phase
    else {
        panic!("canonical user item must be completed")
    };
    assert!(matches!(
        final_content.item(),
        agentdash_agent_protocol::AgentDashThreadItem::Codex(
            agentdash_agent_protocol::CodexThreadItem::UserMessage { .. }
        )
    ));
}

#[tokio::test]
async fn snapshot_cursor_is_the_latest_included_durable_event() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("thread id");
    let snapshot = thread_snapshot(&runtime, thread_id.clone()).await;
    assert_eq!(snapshot.latest_event_sequence, EventSequence(3));
    assert!(
        store
            .internal_events_after(&thread_id, Some(snapshot.latest_event_sequence))
            .await
            .expect("events after snapshot")
            .events
            .is_empty()
    );

    assert!(matches!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::ThreadStatusChanged {
                status: RuntimeThreadStatus::Suspended,
            }))
            .await
            .expect("driver event"),
        DriverEventAdmission::Durable { .. }
    ));
    let after = store
        .internal_events_after(&thread_id, Some(snapshot.latest_event_sequence))
        .await
        .expect("new events")
        .events;
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].sequence, Some(EventSequence(4)));
}

#[tokio::test]
async fn accepted_operation_is_readable_by_canonical_operation_identity() {
    let (_store, runtime) = fixture();
    let receipt = runtime.execute(start()).await.expect("start accepted");
    let result = runtime
        .snapshot(RuntimeSnapshotQuery::Operation {
            operation_id: receipt.operation_id.clone(),
        })
        .await
        .expect("operation snapshot");
    let RuntimeSnapshotResult::Operation { operation } = result else {
        panic!("expected operation snapshot");
    };
    assert_eq!(operation.operation_id, receipt.operation_id);
    assert_eq!(operation.receipt, receipt);
    assert!(matches!(
        operation.command,
        RuntimeCommand::ThreadStart { .. }
    ));
    assert!(operation.terminal.is_none());
}

#[tokio::test]
async fn every_injected_write_stage_rolls_back_the_complete_acceptance_write_set() {
    for point in [
        CommitFailurePoint::BeforeWrite,
        CommitFailurePoint::AfterProjection,
        CommitFailurePoint::AfterOperation,
        CommitFailurePoint::AfterEvents,
        CommitFailurePoint::AfterOutbox,
        CommitFailurePoint::AfterContext,
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
                .internal_events_after(&id("thread-source-1"), None)
                .await
                .expect("events")
                .events
                .is_empty(),
            "journal leaked at {point:?}"
        );
        assert!(
            store
                .load_hook_plan(&id("thread-source-1"))
                .await
                .expect("read hook plan")
                .is_none(),
            "hook plan leaked at {point:?}"
        );
    }
}

#[tokio::test]
async fn idempotency_expected_revision_and_operation_sequence_are_enforced() {
    let (store, runtime) = fixture();
    let first = runtime.execute(start()).await.expect("start");
    assert!(runtime.execute(start()).await.expect("duplicate").duplicate);
    let mut altered_presentation = start();
    altered_presentation.presentation = vec![RuntimePresentationInput {
        coordinate: RuntimePresentationCoordinate {
            runtime_turn_id: None,
            presentation_turn_id: None,
            runtime_item_id: None,
            interaction_id: None,
            source_thread_id: Some("source-1".to_string()),
            source_turn_id: None,
            source_item_id: None,
            source_request_id: Some("command-request".to_string()),
            source_entry_index: Some(0),
        },
        event: abstract_presentation_record(
            &id("thread-source-1"),
            1,
            RuntimeRevision(1),
            "idempotency-conflict",
        )
        .as_presentation()
        .expect("presentation")
        .clone(),
    }];
    assert!(matches!(
        runtime.execute(altered_presentation).await,
        Err(RuntimeExecuteError::OperationConflict {
            conflict: OperationConflictKind::OperationIdReused,
            ..
        })
    ));
    assert_eq!(store.outbox().await.len(), 1);
    let thread_id = first.thread_id.expect("thread");

    let turn = |expected| {
        command(
            "op-2",
            "key-2",
            Some(expected),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-757"),
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
            .execute(turn(3))
            .await
            .expect("turn accepted")
            .operation_sequence
            .0,
        2
    );
}

#[tokio::test]
async fn driver_turn_started_ack_reuses_the_runtime_owned_turn_identity() {
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
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-792"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    let turn_id: RuntimeTurnId = id("turn-op-2");

    assert_eq!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::TurnStarted {
                turn_id: turn_id.clone(),
                presentation_turn_id: id("presentation-turn-792"),
            }))
            .await
            .expect("driver acknowledgement"),
        DriverEventAdmission::Observed
    );
    let snapshot = thread_snapshot(&runtime, thread_id).await;
    assert_eq!(snapshot.revision, RuntimeRevision(5));
    assert_eq!(snapshot.active_turn_id, Some(turn_id));
    assert_eq!(snapshot.status, RuntimeThreadStatus::Active);
}

fn turn_started_presentation(thread_id: &str, turn_id: &str) -> RuntimeJournalFact {
    let event = serde_json::from_value(serde_json::json!({
        "type": "turn_started",
        "payload": {
            "threadId": thread_id,
            "turn": {
                "id": turn_id,
                "items": [],
                "itemsView": "notLoaded",
                "status": "inProgress",
                "error": null,
                "startedAt": 1,
                "completedAt": null,
                "durationMs": null
            }
        }
    }))
    .expect("typed turn started presentation");
    RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
        PresentationDurability::Durable,
        event,
    ))
}

fn presentation_append_request(
    thread_id: RuntimeThreadId,
    key: &str,
    source_turn_id: &str,
) -> RuntimePresentationAppendRequest {
    let RuntimeJournalFact::Presentation(event) =
        turn_started_presentation("presentation-thread-1", source_turn_id)
    else {
        unreachable!("presentation fixture")
    };
    RuntimePresentationAppendRequest {
        runtime_thread_id: thread_id,
        producer: "workspace_module".to_string(),
        idempotency_key: id(key),
        events: vec![RuntimePresentationInput {
            coordinate: RuntimePresentationCoordinate {
                runtime_turn_id: None,
                presentation_turn_id: Some(id(source_turn_id)),
                runtime_item_id: None,
                interaction_id: None,
                source_thread_id: None,
                source_turn_id: Some(source_turn_id.to_string()),
                source_item_id: None,
                source_request_id: None,
                source_entry_index: None,
            },
            event,
        }],
    }
}

#[tokio::test]
async fn canonical_presentation_append_is_ordered_idempotent_and_thread_checked() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    let gateway: &dyn AgentRuntimeGateway = &runtime;
    let mut request = presentation_append_request(thread_id.clone(), "append-key", "source-turn");
    request.events[0].coordinate.source_request_id = Some("vendor-request-42".to_string());
    request.events[0].coordinate.source_entry_index = Some(0);
    let mut second = request.events[0].clone();
    second.coordinate.source_entry_index = None;
    request.events.push(second);
    let first = gateway
        .append_presentation(request.clone())
        .await
        .expect("append presentation");
    assert!(!first.duplicate);
    let replay = gateway
        .append_presentation(request.clone())
        .await
        .expect("replay presentation");
    assert!(replay.duplicate);
    assert_eq!(replay.first_sequence, first.first_sequence);
    assert_eq!(replay.last_sequence, first.last_sequence);

    let appended = store
        .journal_records_after(&thread_id, None)
        .await
        .expect("appended journal")
        .records
        .into_iter()
        .filter(|record| record.carrier().append_idempotency_key.is_some())
        .collect::<Vec<_>>();
    assert_eq!(appended.len(), 2);
    assert_eq!(
        appended[0]
            .carrier()
            .coordinate
            .source_request_id
            .as_deref(),
        Some("vendor-request-42")
    );
    assert_eq!(appended[0].carrier().coordinate.source_entry_index, Some(0));
    assert_eq!(appended[1].carrier().coordinate.source_entry_index, None);

    let mut conflict = request;
    conflict.events.swap(0, 1);
    assert_eq!(
        gateway.append_presentation(conflict).await,
        Err(RuntimePresentationAppendError::IdempotencyConflict)
    );

    let mut wrong_thread =
        presentation_append_request(thread_id.clone(), "wrong-thread-key", "source-turn-2");
    let RuntimeJournalFact::Presentation(event) =
        turn_started_presentation("another-presentation-thread", "source-turn-2")
    else {
        unreachable!("presentation fixture")
    };
    wrong_thread.events = vec![RuntimePresentationInput {
        coordinate: wrong_thread.events[0].coordinate.clone(),
        event,
    }];
    assert!(matches!(
        gateway.append_presentation(wrong_thread).await,
        Err(RuntimePresentationAppendError::Invalid(_))
    ));

    let presentation_count = store
        .journal_records_after(&thread_id, None)
        .await
        .expect("journal")
        .records
        .into_iter()
        .filter(|record| record.as_presentation().is_some())
        .count();
    assert_eq!(presentation_count, 2);
}

#[tokio::test]
async fn driver_turn_started_ack_suppresses_only_its_correlated_presentation() {
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
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-966"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    let turn_id: RuntimeTurnId = id("turn-op-2");
    assert_eq!(
        thread_snapshot(&runtime, thread_id.clone())
            .await
            .active_turn_id,
        Some(turn_id.clone())
    );
    let mut source = driver_facts(vec![
        RuntimeJournalFact::Internal(RuntimeEvent::TurnStarted {
            turn_id: turn_id.clone(),
            presentation_turn_id: id("presentation-turn-966"),
        }),
        turn_started_presentation("source-1", "source-turn-1"),
        turn_started_presentation("source-1", "unrelated-source-turn"),
    ]);
    source.source_turn_id = Some(id("source-turn-1"));

    assert!(matches!(
        runtime
            .ingest_driver_event(source)
            .await
            .expect("driver acknowledgement"),
        DriverEventAdmission::Durable { .. }
    ));
    let records = store
        .journal_records_after(&thread_id, None)
        .await
        .expect("presentation journal")
        .records;
    let presentation = records
        .iter()
        .filter(|record| record.as_presentation().is_some())
        .collect::<Vec<_>>();
    assert_eq!(presentation.len(), 1);
    let event = &presentation[0]
        .as_presentation()
        .expect("presentation")
        .event;
    assert!(matches!(
        event,
        agentdash_agent_protocol::BackboneEvent::TurnStarted(notification)
            if notification.turn.id == "unrelated-source-turn"
    ));

    let snapshot = thread_snapshot(&runtime, thread_id).await;
    assert_eq!(snapshot.active_turn_id, Some(turn_id));
}

#[tokio::test]
async fn presentation_turn_started_without_internal_ack_is_preserved() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    let mut source = driver_facts(vec![turn_started_presentation(
        "source-1",
        "source-turn-without-ack",
    )]);
    source.source_turn_id = Some(id("source-turn-without-ack"));
    source.source_entry_index = Some(17);

    assert!(matches!(
        runtime
            .ingest_driver_event(source)
            .await
            .expect("standalone presentation"),
        DriverEventAdmission::Durable { .. }
    ));
    let records = store
        .journal_records_after(&thread_id, None)
        .await
        .expect("presentation journal")
        .records;
    let presentation = records
        .iter()
        .filter(|record| record.as_presentation().is_some())
        .collect::<Vec<_>>();
    assert_eq!(presentation.len(), 1);
    assert_eq!(
        presentation[0].carrier().coordinate.source_entry_index,
        Some(17)
    );
    assert!(matches!(
        &presentation[0]
            .as_presentation()
            .expect("presentation")
            .event,
        agentdash_agent_protocol::BackboneEvent::TurnStarted(notification)
            if notification.turn.id == "source-turn-without-ack"
    ));
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
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-1088"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    let mut changed_actor_and_payload = command(
        "op-3",
        "shared-key",
        Some(5),
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
        Some(3),
        RuntimeCommand::TurnStart {
            thread_id: thread_id.clone(),
            presentation_turn_id: id("presentation-turn-1129"),
            input: Vec::new(),
        },
    );
    let settings = command(
        "op-3",
        "key-3",
        Some(3),
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
        .internal_events_after(&thread_id, None)
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
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-1187"),
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
                transient_after: None,
                stream_generation: None,
            })
            .await,
        Err(RuntimeSubscribeError::CursorGap {
            requested: EventSequence(1),
            earliest_available: EventSequence(3),
            latest_available: EventSequence(5),
        })
    ));
    assert!(matches!(
        runtime
            .events(RuntimeEventSubscription {
                thread_id,
                after: Some(EventSequence(6)),
                include_transient: false,
                transient_after: None,
                stream_generation: None,
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
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-1242"),
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
            diagnostic: None,
        }))
        .await
        .expect("lost");
    assert!(matches!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::TurnTerminal {
                turn_id,
                terminal: RuntimeTurnTerminal::Completed,
                message: None,
                diagnostic: None,
            }))
            .await
            .expect("critical protocol fact"),
        DriverEventAdmission::Terminalized { .. }
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
async fn driver_batch_transition_failure_discards_staged_prefix_and_terminalizes_atomically() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("thread id");
    runtime
        .execute(command(
            "op-staged-violation",
            "key-staged-violation",
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-staged-violation"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");

    let admission = runtime
        .ingest_driver_event(driver_facts(vec![
            RuntimeJournalFact::Internal(RuntimeEvent::ConversationError {
                turn_id: Some(id("turn-op-staged-violation")),
                error: RuntimeConversationError {
                    code: Some("staged-prefix-must-not-commit".into()),
                    message: "valid prefix".into(),
                    retryable: true,
                    details: None,
                },
            }),
            RuntimeJournalFact::Internal(RuntimeEvent::ItemTerminal {
                turn_id: id("turn-op-staged-violation"),
                item_id: id("missing-item"),
                terminal: RuntimeItemTerminal::Lost {
                    message: Some("invalid suffix".into()),
                },
            }),
        ]))
        .await
        .expect("critical transition violation must commit from the base revision");
    assert!(matches!(
        admission,
        DriverEventAdmission::Terminalized { .. }
    ));

    let snapshot = thread_snapshot(&runtime, thread_id.clone()).await;
    assert_eq!(snapshot.status, RuntimeThreadStatus::Lost);
    assert!(snapshot.active_turn_id.is_none());
    let records = store
        .journal_records_after(&thread_id, None)
        .await
        .expect("journal")
        .records;
    assert!(records.iter().all(|record| !matches!(
        record.fact(),
        RuntimeJournalFact::Internal(RuntimeEvent::ConversationError { error, .. })
            if error.code.as_deref() == Some("staged-prefix-must-not-commit")
    )));
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(
                record.fact(),
                RuntimeJournalFact::Internal(RuntimeEvent::ProtocolViolation {
                    critical: true,
                    ..
                })
            ))
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(
                record.fact(),
                RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                    terminal: RuntimeTurnTerminal::Lost,
                    ..
                })
            ))
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| is_turn_terminal_record(record))
            .count(),
        1
    );
    assert_eq!(store.terminal_application_effects().await.len(), 1);
    assert_eq!(store.quarantined().await.len(), 1);
    assert!(
        store
            .find_operation(&id("op-staged-violation"))
            .await
            .expect("operation read")
            .expect("operation")
            .terminal
            .is_some()
    );
}

#[tokio::test]
async fn application_terminal_projection_is_committed_after_all_connector_facts_for_any_batch_order()
 {
    async fn capture(terminal_first: bool) -> Vec<RuntimeJournalRecord> {
        let store = Arc::new(RuntimeStoreFixture::default());
        let runtime =
            ManagedAgentRuntime::new(store.clone(), Arc::new(TestTerminalPresentationProjector));
        let thread_id = runtime
            .execute(start())
            .await
            .expect("thread")
            .thread_id
            .expect("thread id");
        runtime
            .execute(command(
                "op-terminal-order",
                "key-terminal-order",
                Some(3),
                RuntimeCommand::TurnStart {
                    thread_id: thread_id.clone(),
                    presentation_turn_id: id("presentation-turn-terminal-order"),
                    input: Vec::new(),
                },
            ))
            .await
            .expect("turn");
        let terminal = RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
            turn_id: id("turn-op-terminal-order"),
            terminal: RuntimeTurnTerminal::Completed,
            message: None,
            diagnostic: None,
        });
        let connector = RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
            PresentationDurability::Durable,
            agentdash_agent_protocol::BackboneEvent::Platform(
                agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate {
                    key: "connector_terminal_marker".into(),
                    value: serde_json::json!({"status": "completed"}),
                },
            ),
        ));
        let facts = if terminal_first {
            vec![terminal, connector]
        } else {
            vec![connector, terminal]
        };
        runtime
            .ingest_driver_event(driver_facts(facts))
            .await
            .expect("terminal batch admission");
        store
            .journal_records_after(&thread_id, None)
            .await
            .expect("journal")
            .records
    }

    for terminal_first in [false, true] {
        let records = capture(terminal_first).await;
        let presentation_keys = records
            .iter()
            .filter_map(|record| {
                let RuntimeJournalFact::Presentation(event) = record.fact() else {
                    return None;
                };
                let agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, .. },
                ) = &event.event
                else {
                    return None;
                };
                Some(key.as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            &presentation_keys[presentation_keys.len() - 2..],
            ["connector_terminal_marker", "turn_terminal"]
        );
        let terminal_recorded_at = records
            .iter()
            .find_map(|record| {
                matches!(
                    record.fact(),
                    RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal { .. })
                )
                .then_some(record.carrier().recorded_at_ms)
            })
            .expect("terminal carrier time");
        let projected_completed_at = records
            .iter()
            .find_map(|record| {
                let RuntimeJournalFact::Presentation(event) = record.fact() else {
                    return None;
                };
                let agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value },
                ) = &event.event
                else {
                    return None;
                };
                (key == "turn_terminal")
                    .then(|| value["completed_at_ms"].as_u64())
                    .flatten()
            })
            .expect("projected completed time");
        assert_eq!(terminal_recorded_at, projected_completed_at);
    }
}

#[tokio::test]
async fn invalid_driver_terminals_are_atomically_terminalized_and_quarantined() {
    async fn assert_terminalized(facts: Vec<RuntimeJournalFact>) {
        let (store, runtime) = fixture();
        let thread_id = runtime
            .execute(start())
            .await
            .expect("thread")
            .thread_id
            .expect("thread id");
        runtime
            .execute(command(
                "op-terminal-reject",
                "key-terminal-reject",
                Some(3),
                RuntimeCommand::TurnStart {
                    thread_id: thread_id.clone(),
                    presentation_turn_id: id("presentation-turn-terminal-reject"),
                    input: Vec::new(),
                },
            ))
            .await
            .expect("turn");
        assert!(matches!(
            runtime
                .ingest_driver_event(driver_facts(facts))
                .await
                .expect("invalid driver lifecycle must commit its critical terminal"),
            DriverEventAdmission::Terminalized { .. }
        ));
        let snapshot = thread_snapshot(&runtime, thread_id.clone()).await;
        assert_eq!(snapshot.status, RuntimeThreadStatus::Lost);
        assert!(snapshot.active_turn_id.is_none());
        assert_eq!(store.quarantined().await.len(), 1);
        assert_eq!(store.terminal_application_effects().await.len(), 1);
        assert_eq!(
            store
                .journal_records_after(&thread_id, None)
                .await
                .expect("journal")
                .records
                .iter()
                .filter(|record| is_turn_terminal_record(record))
                .count(),
            1
        );
    }

    let terminal = || {
        RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
            turn_id: id("turn-op-terminal-reject"),
            terminal: RuntimeTurnTerminal::Completed,
            message: None,
            diagnostic: None,
        })
    };
    assert_terminalized(vec![terminal(), terminal()]).await;
    assert_terminalized(vec![RuntimeJournalFact::Internal(
        RuntimeEvent::TurnTerminal {
            turn_id: id("unknown-terminal-turn"),
            terminal: RuntimeTurnTerminal::Failed,
            message: Some("missing mapping".into()),
            diagnostic: None,
        },
    )])
    .await;
}

#[tokio::test]
async fn explicit_terminal_diagnostic_wins_over_same_batch_and_historical_diagnostics() {
    fn diagnostic(label: &str) -> agentdash_agent_protocol::RuntimeTerminalDiagnostic {
        agentdash_agent_protocol::RuntimeTerminalDiagnostic {
            kind: "provider".into(),
            code: Some(label.into()),
            http_status: None,
            provider: Some("fixture".into()),
            model: None,
            message: label.into(),
            retryable: true,
        }
    }

    async fn capture(
        explicit: Option<agentdash_agent_protocol::RuntimeTerminalDiagnostic>,
    ) -> serde_json::Value {
        let store = Arc::new(RuntimeStoreFixture::default());
        let runtime =
            ManagedAgentRuntime::new(store.clone(), Arc::new(TestTerminalPresentationProjector));
        let thread_id = runtime
            .execute(start())
            .await
            .expect("thread")
            .thread_id
            .expect("thread id");
        runtime
            .execute(command(
                "op-terminal-diagnostic",
                "key-terminal-diagnostic",
                Some(3),
                RuntimeCommand::TurnStart {
                    thread_id: thread_id.clone(),
                    presentation_turn_id: id("presentation-turn-terminal-diagnostic"),
                    input: Vec::new(),
                },
            ))
            .await
            .expect("turn");
        let runtime_turn_id: RuntimeTurnId = id("turn-op-terminal-diagnostic");
        runtime
            .ingest_driver_event(driver_facts(vec![
                RuntimeJournalFact::Internal(RuntimeEvent::TurnStarted {
                    turn_id: runtime_turn_id.clone(),
                    presentation_turn_id: id("presentation-turn-terminal-diagnostic"),
                }),
                RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::RuntimeTerminalDiagnostic(
                            diagnostic("historical"),
                        ),
                    ),
                )),
            ]))
            .await
            .expect("historical diagnostic");
        runtime
            .ingest_driver_event(driver_facts(vec![
                RuntimeJournalFact::Internal(RuntimeEvent::TurnTerminal {
                    turn_id: runtime_turn_id,
                    terminal: RuntimeTurnTerminal::Failed,
                    message: Some("failed".into()),
                    diagnostic: explicit,
                }),
                RuntimeJournalFact::Presentation(ImmutablePresentationEvent::new(
                    PresentationDurability::Durable,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::RuntimeTerminalDiagnostic(
                            diagnostic("same-batch"),
                        ),
                    ),
                )),
            ]))
            .await
            .expect("terminal diagnostic");
        store
            .journal_records_after(&thread_id, None)
            .await
            .expect("journal")
            .records
            .iter()
            .find_map(|record| {
                let RuntimeJournalFact::Presentation(event) = record.fact() else {
                    return None;
                };
                let agentdash_agent_protocol::BackboneEvent::Platform(
                    agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value },
                ) = &event.event
                else {
                    return None;
                };
                (key == "turn_terminal").then(|| value["diagnostic"].clone())
            })
            .expect("projected diagnostic")
    }

    assert_eq!(capture(None).await["code"], "same-batch");
    assert_eq!(
        capture(Some(diagnostic("explicit-terminal"))).await["code"],
        "explicit-terminal"
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
        .internal_events_after(&thread_id, None)
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
            .internal_events_after(&thread_id, None)
            .await
            .expect("events")
            .events
            .len(),
        before
    );
}

#[tokio::test]
async fn lost_thread_rebinds_in_place_and_fences_old_binding_events() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::BindingLost {
            binding_id: id("binding-1"),
            reason: "relay disconnected".to_string(),
        }))
        .await
        .expect("binding loss");
    let lost = thread_snapshot(&runtime, thread_id.clone()).await;
    assert_eq!(lost.status, RuntimeThreadStatus::Lost);
    let new_profile = profile();
    let new_digest = runtime_profile_digest(&new_profile);
    runtime
        .execute(command(
            "op-rebind-1",
            "key-rebind-1",
            Some(lost.revision.0),
            RuntimeCommand::ThreadRebind {
                thread_id: thread_id.clone(),
                recovery_intent_id: id("recovery-1"),
                binding_epoch: BindingEpoch(2),
                expected_binding_id: id("binding-1"),
                expected_driver_generation: RuntimeDriverGeneration(7),
                new_binding_id: id("binding-2"),
                new_driver_generation: RuntimeDriverGeneration(7),
                source_thread_id: id("source-1"),
                profile_digest: new_digest.clone(),
                bound_profile: Box::new(new_profile),
            },
        ))
        .await
        .expect("same-thread rebind");
    let active = thread_snapshot(&runtime, thread_id.clone()).await;
    assert_eq!(active.thread_id, thread_id);
    assert_eq!(active.status, RuntimeThreadStatus::Active);
    assert_eq!(active.binding_id, id("binding-2"));
    assert_eq!(active.binding_epoch, BindingEpoch(2));
    assert_eq!(active.profile_digest, new_digest);
    let outbox = store.outbox().await;
    assert_eq!(outbox.len(), 1, "rebind must not create driver work");
    assert_eq!(outbox[0].generation, RuntimeDriverGeneration(7));
    assert!(
        !outbox[0].matches_thread_binding(
            &store
                .load_thread(&id("thread-source-1"))
                .await
                .expect("thread read")
                .expect("thread")
        ),
        "same generation cannot make an old binding epoch dispatchable"
    );
    assert!(
        store
            .find_operation(&id("op-rebind-1"))
            .await
            .expect("operation read")
            .expect("operation")
            .terminal
            .is_some(),
        "runtime-owned rebind operation must finish atomically"
    );

    assert_eq!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::ThreadStatusChanged {
                status: RuntimeThreadStatus::Lost,
            }))
            .await
            .expect("old event is quarantined"),
        DriverEventAdmission::Quarantined
    );
    assert_eq!(
        thread_snapshot(&runtime, id("thread-source-1"))
            .await
            .status,
        RuntimeThreadStatus::Active
    );
}

#[tokio::test]
async fn thread_rebind_rejects_active_or_stale_coordinates() {
    let (_store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    let candidate = profile();
    let error = runtime
        .execute(command(
            "op-rebind-invalid",
            "key-rebind-invalid",
            None,
            RuntimeCommand::ThreadRebind {
                thread_id,
                recovery_intent_id: id("recovery-invalid"),
                binding_epoch: BindingEpoch(2),
                expected_binding_id: id("binding-stale"),
                expected_driver_generation: RuntimeDriverGeneration(6),
                new_binding_id: id("binding-2"),
                new_driver_generation: RuntimeDriverGeneration(1),
                source_thread_id: id("source-1"),
                profile_digest: runtime_profile_digest(&candidate),
                bound_profile: Box::new(candidate),
            },
        ))
        .await
        .expect_err("active thread cannot rebind");
    assert!(matches!(error, RuntimeExecuteError::InvalidCommand { .. }));
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
async fn driver_cannot_forge_runtime_owned_hook_transitions() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::HookPlanBound {
            plan_revision: HookPlanRevision(1),
            plan_digest: id("forged-hook-plan"),
        }))
        .await
        .expect("protocol violation persisted");
    let projection = store
        .load_thread(&thread_id)
        .await
        .expect("thread")
        .expect("state");
    assert_eq!(projection.status, RuntimeThreadStatus::Lost);
    assert_eq!(projection.hook_plan_revision, Some(HookPlanRevision(1)));
    assert!(matches!(
        store.quarantined().await.as_slice(),
        [agentdash_agent_runtime::QuarantinedDriverEvent {
            reason:
                agentdash_agent_runtime::DriverEventQuarantineReason::DriverRuntimeOwnedHookEvent,
            ..
        }]
    ));
}

#[tokio::test]
async fn ephemeral_presentation_has_live_cursor_without_durable_journal_entry() {
    let (store, runtime) = fixture();
    let thread_id = runtime
        .execute(start())
        .await
        .expect("thread")
        .thread_id
        .expect("id");
    let durable_before = store
        .journal_records_after(&thread_id, None)
        .await
        .expect("journal before")
        .records
        .len();
    let protected: agentdash_agent_protocol::BackboneEvent =
        serde_json::from_value(serde_json::json!({
            "type": "agent_message_delta",
            "payload": {
                "threadId": "presentation-thread-1",
                "turnId": "presentation-turn-1",
                "itemId": "presentation-item-1",
                "delta": "token"
            }
        }))
        .expect("typed presentation delta");
    let mut live = store.subscribe_presentation(&thread_id).await;

    assert_eq!(
        runtime
            .ingest_driver_event(driver_facts(vec![RuntimeJournalFact::Presentation(
                ImmutablePresentationEvent::new(
                    PresentationDurability::Ephemeral,
                    protected.clone(),
                ),
            )]))
            .await
            .expect("delta"),
        DriverEventAdmission::Transient
    );
    assert_eq!(
        store
            .journal_records_after(&thread_id, None)
            .await
            .expect("journal after")
            .records
            .len(),
        durable_before
    );
    let transient = store
        .read_presentation(&thread_id, None, None)
        .await
        .into_iter()
        .next()
        .expect("ephemeral presentation replay");
    assert_eq!(
        transient.as_presentation().expect("presentation").event,
        protected
    );
    let coordinate = transient
        .carrier()
        .transient
        .as_ref()
        .expect("stable transient coordinate");
    assert_eq!(coordinate.sequence.0, 1);
    assert_eq!(coordinate.stream_generation, RuntimeDriverGeneration(7));
    let first_live = tokio::time::timeout(Duration::from_secs(1), live.recv())
        .await
        .expect("ephemeral presentation live delivery timed out")
        .expect("ephemeral presentation live sender closed");
    assert_eq!(
        first_live.as_presentation().expect("presentation").event,
        protected
    );

    store.clear(&thread_id).await;
    assert!(
        store
            .read_presentation(&thread_id, None, None)
            .await
            .is_empty()
    );
    runtime
        .ingest_driver_event(driver_facts(vec![RuntimeJournalFact::Presentation(
            ImmutablePresentationEvent::new(PresentationDurability::Ephemeral, protected.clone()),
        )]))
        .await
        .expect("delta after replay clear");
    let second_live = tokio::time::timeout(Duration::from_secs(1), live.recv())
        .await
        .expect("existing presentation receiver timed out after replay clear")
        .expect("presentation sender was replaced by replay clear");
    assert_eq!(
        second_live.as_presentation().expect("presentation").event,
        protected
    );
}

#[tokio::test]
async fn presentation_only_driver_event_inherits_the_accepted_turn_coordinate() {
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
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-coordinate"),
                input: Vec::new(),
            },
        ))
        .await
        .expect("turn");
    let protected: agentdash_agent_protocol::BackboneEvent =
        serde_json::from_value(serde_json::json!({
            "type": "reasoning_text_delta",
            "payload": {
                "threadId": "presentation-thread-1",
                "turnId": "presentation-turn-coordinate",
                "itemId": "reasoning-coordinate",
                "delta": "thinking",
                "contentIndex": 0
            }
        }))
        .expect("typed reasoning presentation");
    let mut source = driver_facts(vec![RuntimeJournalFact::Presentation(
        ImmutablePresentationEvent::new(PresentationDurability::Ephemeral, protected),
    )]);
    source.operation_id = Some(id("op-2"));

    assert_eq!(
        runtime
            .ingest_driver_event(source)
            .await
            .expect("presentation-only delta"),
        DriverEventAdmission::Transient
    );
    let transient = store
        .read_presentation(&thread_id, None, None)
        .await
        .into_iter()
        .next()
        .expect("transient presentation");
    assert_eq!(
        transient
            .carrier()
            .coordinate
            .runtime_turn_id
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("turn-op-2")
    );
    assert_eq!(
        transient
            .carrier()
            .coordinate
            .presentation_turn_id
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("presentation-turn-coordinate")
    );
}

#[tokio::test]
async fn driver_transient_internal_summary_is_quarantined_instead_of_replayed() {
    let (store, runtime) = fixture();
    runtime.execute(start()).await.expect("thread");

    runtime
        .ingest_driver_event(driver(RuntimeEvent::ConversationDelta {
            turn_id: id("forged-turn"),
            item_id: id("forged-item"),
            delta: RuntimeConversationDelta::AgentMessage {
                delta: "summary".to_string(),
            },
        }))
        .await
        .expect("protocol violation persisted");

    assert!(matches!(
        store.quarantined().await.as_slice(),
        [agentdash_agent_runtime::QuarantinedDriverEvent {
            reason: agentdash_agent_runtime::DriverEventQuarantineReason::TransientInternalFact,
            ..
        }]
    ));
    assert!(
        store
            .read_presentation(&id("thread-source-1"), None, None)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn closed_live_channels_deliver_terminal_then_release_sender_entries() {
    let store = RuntimeStoreFixture::default();
    let thread_id: RuntimeThreadId = id("closed-thread");
    let mut receiver = store.subscribe(&thread_id).await;
    store
        .publish_durable(RuntimeEventEnvelope {
            thread_id: thread_id.clone(),
            occurred_at_ms: 0,
            sequence: Some(EventSequence(1)),
            transient: None,
            revision: RuntimeRevision(1),
            event: RuntimeEvent::ThreadStatusChanged {
                status: RuntimeThreadStatus::Closed,
            },
        })
        .await;
    let terminal = receiver
        .recv()
        .await
        .expect("existing receiver gets terminal");
    assert!(matches!(
        terminal.event,
        RuntimeEvent::ThreadStatusChanged {
            status: RuntimeThreadStatus::Closed
        }
    ));
    assert_eq!(store.live_sender_count().await, 0);

    for index in 0..128 {
        let closed: RuntimeThreadId = id(&format!("closed-{index}"));
        let _receiver = store.subscribe(&closed).await;
        store
            .publish_durable(RuntimeEventEnvelope {
                thread_id: closed,
                occurred_at_ms: 0,
                sequence: Some(EventSequence(1)),
                transient: None,
                revision: RuntimeRevision(1),
                event: RuntimeEvent::ThreadStatusChanged {
                    status: RuntimeThreadStatus::Closed,
                },
            })
            .await;
    }
    assert_eq!(
        store.live_sender_count().await,
        0,
        "closed threads cannot grow sender map"
    );
    let _new_receiver = store.subscribe(&thread_id).await;
    assert_eq!(
        store.live_sender_count().await,
        1,
        "durable replay subscription recreates a channel"
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
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-1723"),
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
            initial_content: RuntimeItemContent::agent_message(item_id.as_str(), "fixture"),
        }))
        .await
        .expect("item");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::InteractionRequested {
            turn_id: turn_id.clone(),
            item_id: Some(item_id.clone()),
            interaction_id: interaction_id.clone(),
            request: RuntimeInteractionRequest::temporary_command_approval(
                thread_id.as_str(),
                turn_id.as_str(),
                item_id.as_str(),
                "fixture",
            ),
        }))
        .await
        .expect("interaction");
    runtime
        .execute(command(
            "op-3",
            "key-3",
            Some(7),
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
                final_content: RuntimeItemContent::agent_message(item_id.as_str(), "done"),
            },
        }))
        .await
        .expect("item terminal");
    assert!(matches!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::ConversationDelta {
                turn_id,
                item_id,
                delta: RuntimeConversationDelta::AgentMessage {
                    delta: "late".to_string()
                },
            }))
            .await
            .expect("late delta protocol fact"),
        DriverEventAdmission::Terminalized { .. }
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
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-1836"),
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
            initial_content: RuntimeItemContent::agent_message(item_id.as_str(), "fixture"),
        }))
        .await
        .expect("item");
    runtime
        .ingest_driver_event(driver(RuntimeEvent::InteractionRequested {
            turn_id: turn_id.clone(),
            item_id: Some(item_id.clone()),
            interaction_id,
            request: RuntimeInteractionRequest::temporary_command_approval(
                thread_id.as_str(),
                turn_id.as_str(),
                item_id.as_str(),
                "fixture",
            ),
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
    let records = store
        .journal_records_after(&thread_id, None)
        .await
        .expect("binding-loss journal")
        .records;
    assert!(records.iter().any(|record| {
        matches!(
            record.fact(),
            RuntimeJournalFact::Presentation(event)
                if matches!(
                    &event.event,
                    agentdash_agent_protocol::BackboneEvent::Platform(
                        agentdash_agent_protocol::PlatformEvent::SessionMetaUpdate { key, value }
                    ) if key == "turn_terminal"
                        && value["terminal_type"] == "turn_lost"
                )
        )
    }), "BindingLost must project the generated lost turn terminal into the Session stream");
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
            Some(3),
            RuntimeCommand::TurnStart {
                thread_id: thread_id.clone(),
                presentation_turn_id: id("presentation-turn-1920"),
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
            initial_content: RuntimeItemContent::agent_message("item-1", String::new()),
        }))
        .await
        .expect("item");

    assert!(matches!(
        runtime
            .ingest_driver_event(driver(RuntimeEvent::TurnTerminal {
                turn_id,
                terminal: RuntimeTurnTerminal::Completed,
                message: None,
                diagnostic: None,
            }))
            .await
            .expect("critical fact"),
        DriverEventAdmission::Terminalized { .. }
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
        .internal_events_after(&thread_id, None)
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
