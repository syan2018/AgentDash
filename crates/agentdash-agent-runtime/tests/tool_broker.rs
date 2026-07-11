use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use agentdash_agent_runtime::*;
use agentdash_agent_runtime_contract::*;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

fn id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("valid id")
}

fn tool_meta() -> ContributionMeta {
    ContributionMeta {
        key: "tool:code.scan".to_string(),
        source: SurfaceSourceRef {
            layer: "project".to_string(),
            key: "mcp:code".to_string(),
        },
        priority: 10,
        requirement: ContributionRequirement::Required,
    }
}

fn catalog() -> ToolCatalogRevision {
    ToolCatalogRevision {
        revision: ToolSetRevision(4),
        digest: "sha256:catalog".to_string(),
        tools: vec![ToolContribution {
            meta: tool_meta(),
            runtime_name: "code_scan".to_string(),
            description: "Scan code".to_string(),
            parameters_schema: serde_json::json!({"type":"object"}),
            capability_key: "mcp:code".to_string(),
            tool_path: "mcp:code::scan".to_string(),
            allowed_channels: [ToolChannel::DirectCallback, ToolChannel::McpFacade].into(),
            configuration_boundary: ConfigurationBoundary::Binding,
        }],
        mcp_servers: vec![McpContribution {
            meta: ContributionMeta {
                key: "mcp:code".to_string(),
                source: SurfaceSourceRef {
                    layer: "project".to_string(),
                    key: "project:mcp".to_string(),
                },
                priority: 10,
                requirement: ContributionRequirement::Required,
            },
            server_key: "mcp:code".to_string(),
            credential_refs: vec!["credential:code".to_string()],
        }],
    }
}

fn invocation(item: &str) -> ToolBrokerInvocation {
    ToolBrokerInvocation {
        coordinates: ToolCallCoordinates {
            thread_id: id("thread-broker"),
            turn_id: id("turn-broker"),
            item_id: id(item),
            binding_id: id("binding-broker"),
            binding_generation: RuntimeDriverGeneration(3),
            tool_set_revision: ToolSetRevision(4),
        },
        tool_name: "code_scan".to_string(),
        arguments: serde_json::json!({"path":"crates"}),
        timeout_ms: 1_000,
    }
}

#[derive(Default)]
struct RecordingPolicy {
    checks: AtomicUsize,
    approval_required: AtomicBool,
    deny_vfs: AtomicBool,
}

#[async_trait]
impl ToolBrokerPolicyPort for RecordingPolicy {
    async fn validate_binding(
        &self,
        _invocation: &ToolBrokerInvocation,
    ) -> Result<ToolGuardDecision, ToolBrokerError> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        Ok(ToolGuardDecision::Allowed(ToolPolicyCheck { revision: 1 }))
    }

    async fn authorize_capability(
        &self,
        _invocation: &ToolBrokerInvocation,
        _tool: &ToolContribution,
    ) -> Result<ToolGuardDecision, ToolBrokerError> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        Ok(ToolGuardDecision::Allowed(ToolPolicyCheck { revision: 2 }))
    }

    async fn authorize_permission(
        &self,
        _invocation: &ToolBrokerInvocation,
        _tool: &ToolContribution,
    ) -> Result<ToolPermissionDecision, ToolBrokerError> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        if self.approval_required.load(Ordering::SeqCst) {
            Ok(ToolPermissionDecision::ApprovalRequired {
                interaction_id: id("interaction-approval"),
                reason: "supervised".to_string(),
            })
        } else {
            Ok(ToolPermissionDecision::Allowed(ToolPolicyCheck {
                revision: 3,
            }))
        }
    }

    async fn authorize_vfs(
        &self,
        _invocation: &ToolBrokerInvocation,
        _tool: &ToolContribution,
    ) -> Result<ToolGuardDecision, ToolBrokerError> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        if self.deny_vfs.load(Ordering::SeqCst) {
            Ok(ToolGuardDecision::Denied {
                reason: "path outside mounted workspace".to_string(),
            })
        } else {
            Ok(ToolGuardDecision::Allowed(ToolPolicyCheck { revision: 4 }))
        }
    }
}

#[derive(Default)]
struct RecordingCredentials {
    refs: tokio::sync::Mutex<Vec<String>>,
}

#[derive(Default)]
struct RecordingJournal {
    accepted: AtomicUsize,
    approvals: AtomicUsize,
    terminals: AtomicUsize,
}

#[async_trait]
impl ToolBrokerRuntimeJournal for RecordingJournal {
    async fn accept_tool_call(
        &self,
        _invocation: &ToolBrokerInvocation,
        _tool: &ToolContribution,
    ) -> Result<(), ToolBrokerError> {
        self.accepted.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn record_tool_terminal(&self, call: &ToolBrokerCall) -> Result<(), ToolBrokerError> {
        assert!(call.status.is_terminal());
        self.terminals.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn request_tool_approval(
        &self,
        _invocation: &ToolBrokerInvocation,
        _interaction_id: &RuntimeInteractionId,
        _reason: &str,
    ) -> Result<(), ToolBrokerError> {
        self.approvals.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl ToolCredentialResolver for RecordingCredentials {
    async fn resolve(
        &self,
        credential_refs: &[String],
    ) -> Result<CredentialMaterial, ToolBrokerError> {
        *self.refs.lock().await = credential_refs.to_vec();
        Ok(CredentialMaterial::new(BTreeMap::from([(
            "token".to_string(),
            "secret".to_string(),
        )])))
    }
}

struct RecordingExecutor {
    calls: AtomicUsize,
    delay: Duration,
}

#[derive(Default)]
struct RewritingHooks {
    before: AtomicUsize,
    after: AtomicUsize,
}

#[async_trait]
impl ToolBrokerHookPort for RewritingHooks {
    async fn before_tool(
        &self,
        invocation: &ToolBrokerInvocation,
    ) -> Result<ToolBrokerHookDecision, ToolBrokerError> {
        self.before.fetch_add(1, Ordering::SeqCst);
        let mut arguments = invocation.arguments.clone();
        arguments["hooked"] = serde_json::Value::Bool(true);
        Ok(ToolBrokerHookDecision::Continue { arguments })
    }

    async fn after_tool(
        &self,
        _invocation: &ToolBrokerInvocation,
        mut result: ToolBrokerResult,
    ) -> Result<ToolBrokerResult, ToolBrokerError> {
        self.after.fetch_add(1, Ordering::SeqCst);
        result.output["hooked"] = serde_json::Value::Bool(true);
        Ok(result)
    }
}

#[async_trait]
impl ToolExecutionPort for RecordingExecutor {
    async fn execute(
        &self,
        request: ToolExecutionRequest,
    ) -> Result<ToolBrokerResult, ToolBrokerError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            request.idempotency_key,
            request.invocation.coordinates.item_id
        );
        assert_eq!(
            request
                .credentials
                .expose_to_local_executor()
                .get("token")
                .map(String::as_str),
            Some("secret")
        );
        tokio::time::sleep(self.delay).await;
        Ok(ToolBrokerResult {
            output: serde_json::json!({
                "ok": true,
                "arguments": request.invocation.arguments,
            }),
            is_error: false,
        })
    }
}

struct Fixture {
    broker: PlatformToolBroker,
    repository: Arc<ToolBrokerRepositoryFixture>,
    journal: Arc<RecordingJournal>,
    policy: Arc<RecordingPolicy>,
    credentials: Arc<RecordingCredentials>,
    executor: Arc<RecordingExecutor>,
}

fn fixture(delay: Duration) -> Fixture {
    let repository = Arc::new(ToolBrokerRepositoryFixture::default());
    let policy = Arc::new(RecordingPolicy::default());
    let journal = Arc::new(RecordingJournal::default());
    let credentials = Arc::new(RecordingCredentials::default());
    let executor = Arc::new(RecordingExecutor {
        calls: AtomicUsize::new(0),
        delay,
    });
    let broker = PlatformToolBroker::new(
        catalog(),
        id("binding-broker"),
        RuntimeDriverGeneration(3),
        PlatformToolBrokerDeps {
            repository: repository.clone(),
            journal: journal.clone(),
            policy: policy.clone(),
            credentials: credentials.clone(),
            executor: executor.clone(),
        },
    );
    Fixture {
        broker,
        repository,
        journal,
        policy,
        credentials,
        executor,
    }
}

#[tokio::test]
async fn direct_callback_validates_all_guards_and_deduplicates_side_effect() {
    let fixture = fixture(Duration::ZERO);
    let invocation = invocation("item-direct");
    let first = fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation.clone(),
            CancellationToken::new(),
        )
        .await
        .expect("first invocation");
    let replay = fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation,
            CancellationToken::new(),
        )
        .await
        .expect("replay");

    assert!(matches!(
        first,
        ToolBrokerOutcome::Terminal {
            duplicate: false,
            ..
        }
    ));
    assert!(matches!(
        replay,
        ToolBrokerOutcome::Terminal {
            duplicate: true,
            ..
        }
    ));
    assert_eq!(fixture.policy.checks.load(Ordering::SeqCst), 4);
    assert_eq!(fixture.executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.journal.accepted.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.journal.terminals.load(Ordering::SeqCst), 2);
    assert_eq!(
        fixture.credentials.refs.lock().await.as_slice(),
        &["credential:code".to_string()]
    );
}

#[tokio::test]
async fn approval_waits_without_executing_then_resumes_same_call() {
    let fixture = fixture(Duration::ZERO);
    fixture
        .policy
        .approval_required
        .store(true, Ordering::SeqCst);
    let invocation = invocation("item-approval");
    let pending = fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation.clone(),
            CancellationToken::new(),
        )
        .await
        .expect("pending approval");
    assert!(matches!(
        pending,
        ToolBrokerOutcome::ApprovalRequired { .. }
    ));
    assert_eq!(fixture.executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        fixture
            .repository
            .load(&id("item-approval"))
            .await
            .expect("load")
            .expect("call")
            .pending_interaction_id,
        Some(id("interaction-approval"))
    );

    fixture
        .policy
        .approval_required
        .store(false, Ordering::SeqCst);
    let completed = fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation,
            CancellationToken::new(),
        )
        .await
        .expect("approved replay");
    assert!(matches!(completed, ToolBrokerOutcome::Terminal { .. }));
    assert_eq!(fixture.executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.journal.approvals.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn running_call_replays_with_persisted_arguments_and_canonical_item_key() {
    let fixture = fixture(Duration::ZERO);
    fixture
        .policy
        .approval_required
        .store(true, Ordering::SeqCst);
    let invocation = invocation("item-running-recovery");
    fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation.clone(),
            CancellationToken::new(),
        )
        .await
        .expect("persist awaiting approval");
    let checks_before_recovery = fixture.policy.checks.load(Ordering::SeqCst);
    fixture
        .repository
        .transition(
            &invocation.coordinates.item_id,
            ToolBrokerTransition {
                expected: vec![ToolBrokerCallStatus::AwaitingApproval],
                next: ToolBrokerCallStatus::Running,
                effective_arguments: Some(invocation.arguments.clone()),
                pending_interaction_id: None,
                result: None,
                message: None,
            },
        )
        .await
        .expect("simulate crash after running commit");
    fixture
        .policy
        .approval_required
        .store(false, Ordering::SeqCst);

    let recovered = fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation,
            CancellationToken::new(),
        )
        .await
        .expect("recover running call");

    assert!(matches!(
        recovered,
        ToolBrokerOutcome::Terminal {
            duplicate: true,
            ..
        }
    ));
    assert_eq!(
        fixture.policy.checks.load(Ordering::SeqCst),
        checks_before_recovery,
        "a durable Running call must not reinterpret admission policy"
    );
    assert_eq!(fixture.executor.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancellation_and_timeout_are_durable_terminals() {
    let cancelled_fixture = fixture(Duration::ZERO);
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let cancelled = cancelled_fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation("item-cancelled"),
            cancellation,
        )
        .await
        .expect("cancelled outcome");
    assert!(matches!(
        cancelled,
        ToolBrokerOutcome::Terminal {
            result: ToolBrokerResult { is_error: true, .. },
            ..
        }
    ));
    assert_eq!(cancelled_fixture.executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        cancelled_fixture
            .repository
            .load(&id("item-cancelled"))
            .await
            .expect("load")
            .expect("call")
            .status,
        ToolBrokerCallStatus::Cancelled
    );

    let timeout_fixture = fixture(Duration::from_millis(50));
    let mut timed_invocation = invocation("item-timeout");
    timed_invocation.timeout_ms = 1;
    timeout_fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            timed_invocation,
            CancellationToken::new(),
        )
        .await
        .expect("timeout outcome");
    assert_eq!(
        timeout_fixture
            .repository
            .load(&id("item-timeout"))
            .await
            .expect("load")
            .expect("call")
            .status,
        ToolBrokerCallStatus::TimedOut
    );
}

#[tokio::test]
async fn active_execution_cancellation_wins_over_a_non_cooperative_executor() {
    let fixture = fixture(Duration::from_secs(5));
    let cancellation = CancellationToken::new();
    let cancel = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        cancel.cancel();
    });

    fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation("item-active-cancel"),
            cancellation,
        )
        .await
        .expect("cancelled outcome");

    assert_eq!(
        fixture
            .repository
            .load(&id("item-active-cancel"))
            .await
            .expect("load")
            .expect("call")
            .status,
        ToolBrokerCallStatus::Cancelled
    );
}

#[tokio::test]
async fn mcp_facade_is_session_scoped_and_does_not_publish_credentials() {
    let fixture = fixture(Duration::ZERO);
    let facade = SessionToolMcpFacade::new(fixture.broker, id("thread-broker"), id("turn-broker"));
    let schemas = facade.list_tools();
    assert_eq!(schemas.len(), 1);
    let serialized = serde_json::to_string(&schemas).expect("serialize schemas");
    assert!(!serialized.contains("credential:code"));
    assert!(!serialized.contains("secret"));

    let outcome = facade
        .call(
            id("item-mcp"),
            "code_scan".to_string(),
            serde_json::json!({"path":"src"}),
            1_000,
            CancellationToken::new(),
        )
        .await
        .expect("mcp call");
    assert!(matches!(outcome, ToolBrokerOutcome::Terminal { .. }));
}

#[tokio::test]
async fn stale_generation_is_rejected_before_acceptance() {
    let fixture = fixture(Duration::ZERO);
    let mut stale = invocation("item-stale");
    stale.coordinates.binding_generation = RuntimeDriverGeneration(2);

    let error = fixture
        .broker
        .invoke(ToolChannel::DirectCallback, stale, CancellationToken::new())
        .await
        .expect_err("stale generation");

    assert_eq!(error, ToolBrokerError::StaleCoordinates);
    assert!(
        fixture
            .repository
            .load(&id("item-stale"))
            .await
            .expect("load")
            .is_none()
    );
}

#[tokio::test]
async fn broker_hooks_run_once_at_the_synchronous_tool_boundary() {
    let mut fixture = fixture(Duration::ZERO);
    let hooks = Arc::new(RewritingHooks::default());
    fixture.broker = fixture.broker.with_hooks(hooks.clone());

    let outcome = fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation("item-hooked"),
            CancellationToken::new(),
        )
        .await
        .expect("hooked call");

    let ToolBrokerOutcome::Terminal { result, .. } = outcome else {
        panic!("expected completed tool call")
    };
    assert_eq!(result.output["hooked"], true);
    assert_eq!(result.output["arguments"]["hooked"], true);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.after.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn broker_post_hook_observes_a_typed_timeout_result_before_terminal_commit() {
    let mut fixture = fixture(Duration::from_millis(50));
    let hooks = Arc::new(RewritingHooks::default());
    fixture.broker = fixture.broker.with_hooks(hooks.clone());
    let mut invocation = invocation("item-hooked-timeout");
    invocation.timeout_ms = 1;

    let outcome = fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation,
            CancellationToken::new(),
        )
        .await
        .expect("hooked timeout");

    let ToolBrokerOutcome::Terminal { status, result, .. } = outcome else {
        panic!("expected timeout terminal")
    };
    assert_eq!(status, ToolBrokerCallStatus::TimedOut);
    assert_eq!(result.output["hooked"], true);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.after.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn vfs_denial_is_terminal_before_credentials_or_executor_side_effect() {
    let fixture = fixture(Duration::ZERO);
    fixture.policy.deny_vfs.store(true, Ordering::SeqCst);

    let outcome = fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation("item-vfs-denied"),
            CancellationToken::new(),
        )
        .await
        .expect("typed denial");

    assert!(matches!(
        outcome,
        ToolBrokerOutcome::Denied {
            stage: ToolPolicyStage::Vfs,
            ..
        }
    ));
    assert_eq!(fixture.executor.calls.load(Ordering::SeqCst), 0);
    assert!(fixture.credentials.refs.lock().await.is_empty());
    assert_eq!(
        fixture
            .repository
            .load(&id("item-vfs-denied"))
            .await
            .expect("load")
            .expect("call")
            .status,
        ToolBrokerCallStatus::Failed
    );
}

#[test]
fn catalog_cannot_publish_unknown_channel() {
    let fixture = fixture(Duration::ZERO);
    let channels: BTreeSet<_> = fixture
        .broker
        .published_tools(ToolChannel::DriverNative)
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert!(channels.is_empty());
}

fn runtime_profile() -> RuntimeProfile {
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
            channels: [ToolChannel::DirectCallback].into(),
            configuration_boundary: ConfigurationBoundary::Binding,
            cancellation: true,
        },
        workspace: WorkspaceProfile {
            capabilities: BTreeSet::new(),
            mechanism: DeliveryMechanism::Native,
        },
        interactions: InteractionProfile {
            kinds: [RuntimeInteractionKind::PermissionApproval].into(),
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

#[tokio::test]
async fn managed_runtime_journal_converges_one_terminal_for_replayed_broker_result() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let runtime = ManagedAgentRuntime::new(store.clone());
    runtime
        .execute(RuntimeCommandEnvelope {
            meta: OperationMeta {
                operation_id: id("broker-thread-start"),
                idempotency_key: id("broker-thread-start-key"),
                expected_thread_revision: None,
                actor: RuntimeActor::System {
                    component: "tool-broker-test".to_string(),
                },
            },
            command: RuntimeCommand::ThreadStart {
                thread_id: id("thread-broker"),
                binding_id: id("binding-broker"),
                driver_generation: RuntimeDriverGeneration(3),
                source_thread_id: id("source-thread-broker"),
                profile_digest: id("profile-broker"),
                bound_profile: Box::new(runtime_profile()),
                input: Vec::new(),
                surface_digest: id("surface-broker"),
                settings_revision: ThreadSettingsRevision(0),
                tool_set_revision: ToolSetRevision(4),
                hook_plan: BoundRuntimeHookPlan {
                    revision: HookPlanRevision(1),
                    digest: id("hook-plan-broker"),
                    entries: Vec::new(),
                },
            },
        })
        .await
        .expect("start thread");
    runtime
        .execute(RuntimeCommandEnvelope {
            meta: OperationMeta {
                operation_id: id("broker-turn-start"),
                idempotency_key: id("broker-turn-start-key"),
                expected_thread_revision: Some(RuntimeRevision(3)),
                actor: RuntimeActor::System {
                    component: "tool-broker-test".to_string(),
                },
            },
            command: RuntimeCommand::TurnStart {
                thread_id: id("thread-broker"),
                input: Vec::new(),
            },
        })
        .await
        .expect("start turn");
    let turn_id: RuntimeTurnId = id("turn-broker-turn-start");
    let item_id: RuntimeItemId = id("item-runtime-journal");
    let arguments = serde_json::json!({"path":"crates"});
    runtime
        .ingest_driver_event(DriverEventEnvelope {
            binding_id: id("binding-broker"),
            generation: RuntimeDriverGeneration(3),
            source_thread_id: id("source-thread-broker"),
            source_turn_id: Some(id("source-turn-broker")),
            source_item_id: Some(id("source-item-broker")),
            event: RuntimeEvent::ItemStarted {
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                initial_content: RuntimeItemContent::ToolCall {
                    name: "code_scan".to_string(),
                    arguments: arguments.clone(),
                },
            },
        })
        .await
        .expect("authoritative item start");

    let invocation = ToolBrokerInvocation {
        coordinates: ToolCallCoordinates {
            thread_id: id("thread-broker"),
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
            binding_id: id("binding-broker"),
            binding_generation: RuntimeDriverGeneration(3),
            tool_set_revision: ToolSetRevision(4),
        },
        tool_name: "code_scan".to_string(),
        arguments,
        timeout_ms: 1_000,
    };
    let journal = ManagedRuntimeToolJournal::new(store.clone());
    journal
        .accept_tool_call(&invocation, &catalog().tools[0])
        .await
        .expect("started item matches broker invocation");
    let call = ToolBrokerCall {
        invocation,
        invocation_digest: "sha256:runtime-journal".to_string(),
        capability_key: "mcp:code".to_string(),
        tool_path: "mcp:code::scan".to_string(),
        channel: ToolChannel::DirectCallback,
        status: ToolBrokerCallStatus::Completed,
        effective_arguments: Some(serde_json::json!({"path":"crates"})),
        pending_interaction_id: None,
        result: Some(ToolBrokerResult {
            output: serde_json::json!({"matches": 2}),
            is_error: false,
        }),
        terminal_message: None,
    };
    journal
        .record_tool_terminal(&call)
        .await
        .expect("first terminal");
    journal
        .record_tool_terminal(&call)
        .await
        .expect("terminal replay converges");

    let events = store
        .events_after(&id("thread-broker"), None)
        .await
        .expect("events")
        .events;
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.event, RuntimeEvent::ItemTerminal { .. }))
            .count(),
        1
    );
}
