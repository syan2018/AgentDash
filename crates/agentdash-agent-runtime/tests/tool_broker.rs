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

mod support;
use support::TestTerminalPresentationProjector;

struct AllowSurface;

#[async_trait]
impl RuntimeSurfaceReferenceValidator for AllowSurface {
    async fn validate_surface_reference(
        &self,
        _binding_id: &RuntimeBindingId,
        _runtime_thread_id: &RuntimeThreadId,
        _target: &RuntimeSurfaceDescriptor,
    ) -> Result<(), String> {
        Ok(())
    }
}

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
            protocol_projection: ToolProtocolProjection::Dynamic {
                namespace: Some("test".to_string()),
            },
            presentation_emitter: ToolPresentationEmitter::ToolBroker,
            parity_fixture_id: "main_tool_code_scan_lifecycle".into(),
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
            presentation_item_id: id(item),
            source_thread_id: id("source-thread-broker"),
            source_turn_id: id("source-turn-broker"),
            source_item_id: id(item),
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
    updates: AtomicUsize,
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
        _tool: &ToolContribution,
        _interaction_id: &RuntimeInteractionId,
        _reason: &str,
    ) -> Result<(), ToolBrokerError> {
        self.approvals.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn record_tool_update(
        &self,
        _invocation: &ToolBrokerInvocation,
        _tool: &ToolContribution,
        content_items: Vec<agentdash_agent_protocol::DynamicToolCallOutputContentItem>,
    ) -> Result<(), ToolBrokerError> {
        assert!(!content_items.is_empty());
        self.updates.fetch_add(1, Ordering::SeqCst);
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
    fails: AtomicBool,
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
        for index in 1..=3 {
            let _ = request.updates.send(vec![
                agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                    text: format!("progress-{index}"),
                },
            ]);
        }
        tokio::time::sleep(self.delay).await;
        if self.fails.load(Ordering::SeqCst) {
            return Err(ToolBrokerError::Execution(
                "Canvas surface adoption failed".to_string(),
            ));
        }
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
        fails: AtomicBool::new(false),
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
async fn executor_failure_is_projected_as_typed_diagnostic_content() {
    let fixture = fixture(Duration::ZERO);
    fixture.executor.fails.store(true, Ordering::SeqCst);

    let outcome = fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation("item-executor-failure"),
            CancellationToken::new(),
        )
        .await
        .expect("failed tool outcome");

    let ToolBrokerOutcome::Terminal { status, result, .. } = outcome else {
        panic!("executor failure must produce a terminal outcome");
    };
    assert_eq!(status, ToolBrokerCallStatus::Failed);
    assert_eq!(
        result.output["content_items"][0]["text"],
        "tool executor failed: Canvas surface adoption failed"
    );
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
    assert_eq!(fixture.journal.updates.load(Ordering::SeqCst), 3);
    assert_eq!(
        fixture.credentials.refs.lock().await.as_slice(),
        &["credential:code".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_duplicate_invocations_share_one_execution_terminal() {
    let fixture = fixture(Duration::from_millis(50));
    let invocation = invocation("item-concurrent-duplicate");
    let left_broker = fixture.broker.clone();
    let right_broker = fixture.broker.clone();
    let left_invocation = invocation.clone();

    let (left, right) = tokio::join!(
        left_broker.invoke(
            ToolChannel::DirectCallback,
            left_invocation,
            CancellationToken::new(),
        ),
        right_broker.invoke(
            ToolChannel::DirectCallback,
            invocation,
            CancellationToken::new(),
        )
    );
    let left = left.expect("left invocation terminal");
    let right = right.expect("right invocation terminal");
    let terminal = |outcome: &ToolBrokerOutcome| match outcome {
        ToolBrokerOutcome::Terminal {
            status,
            result,
            duplicate,
        } => (*status, result.clone(), *duplicate),
        other => panic!("expected shared terminal, got {other:?}"),
    };
    let left = terminal(&left);
    let right = terminal(&right);

    assert_eq!(left.0, ToolBrokerCallStatus::Completed);
    assert_eq!(left.0, right.0);
    assert_eq!(left.1, right.1);
    assert_ne!(left.2, right.2);
    assert_eq!(fixture.executor.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn persisted_running_call_is_not_replayed_after_executor_owner_disappears() {
    let fixture = fixture(Duration::from_secs(10));
    let mut invocation = invocation("item-running-after-restart");
    invocation.timeout_ms = 100;
    let owner_broker = fixture.broker.clone();
    let owner_invocation = invocation.clone();
    let owner = tokio::spawn(async move {
        owner_broker
            .invoke(
                ToolChannel::DirectCallback,
                owner_invocation,
                CancellationToken::new(),
            )
            .await
    });

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let call = fixture
                .repository
                .load(&id("item-running-after-restart"))
                .await
                .expect("load running call");
            if call.is_some_and(|call| call.status == ToolBrokerCallStatus::Running) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("execution claim persisted");
    owner.abort();
    let _ = owner.await;

    let replay = fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            invocation,
            CancellationToken::new(),
        )
        .await;
    assert!(matches!(
        replay,
        Err(ToolBrokerError::ExecutionInProgress { ref item_id })
            if item_id == &id("item-running-after-restart")
    ));
    assert_eq!(fixture.executor.calls.load(Ordering::SeqCst), 1);
    assert!(
        fixture
            .repository
            .recoverable()
            .await
            .expect("recovery scan")
            .is_empty(),
        "Running is an unresolved side-effect boundary, not replay permission"
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
async fn orphaned_running_call_preserves_arguments_without_replaying_the_executor() {
    let fixture = fixture(Duration::ZERO);
    fixture
        .policy
        .approval_required
        .store(true, Ordering::SeqCst);
    let mut invocation = invocation("item-running-recovery");
    invocation.timeout_ms = 10;
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
        .await;

    assert!(matches!(
        recovered,
        Err(ToolBrokerError::ExecutionInProgress { ref item_id })
            if item_id == &id("item-running-recovery")
    ));
    assert_eq!(
        fixture.policy.checks.load(Ordering::SeqCst),
        checks_before_recovery,
        "a durable Running call must not reinterpret admission policy"
    );
    assert_eq!(fixture.executor.calls.load(Ordering::SeqCst), 0);
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
        &cancelled,
        ToolBrokerOutcome::Terminal {
            result: ToolBrokerResult { is_error: true, .. },
            ..
        }
    ));
    let ToolBrokerOutcome::Terminal {
        result: cancelled_result,
        ..
    } = &cancelled
    else {
        panic!("cancelled invocation must be terminal");
    };
    assert_eq!(
        cancelled_result.output["content_items"][0]["text"],
        "tool execution cancelled"
    );
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
    let timed_out = timeout_fixture
        .broker
        .invoke(
            ToolChannel::DirectCallback,
            timed_invocation,
            CancellationToken::new(),
        )
        .await
        .expect("timeout outcome");
    let ToolBrokerOutcome::Terminal {
        result: timed_out_result,
        ..
    } = timed_out
    else {
        panic!("timed-out invocation must be terminal");
    };
    assert_eq!(
        timed_out_result.output["content_items"][0]["text"],
        "tool execution timed out"
    );
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
            id("turn_001:tool_001"),
            id("source-thread-broker"),
            id("source-turn-broker"),
            id("source-item-mcp"),
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
        &outcome,
        ToolBrokerOutcome::Denied {
            stage: ToolPolicyStage::Vfs,
            ..
        }
    ));
    assert_eq!(fixture.executor.calls.load(Ordering::SeqCst), 0);
    assert!(fixture.credentials.refs.lock().await.is_empty());
    let denied_call = fixture
        .repository
        .load(&id("item-vfs-denied"))
        .await
        .expect("load")
        .expect("call");
    assert_eq!(denied_call.status, ToolBrokerCallStatus::Failed);
    assert_eq!(
        denied_call.result.expect("denial result").output["content_items"][0]["text"],
        "path outside mounted workspace"
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
            LifecycleCapability::SurfaceAdopt,
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
async fn managed_runtime_journal_preserves_true_mcp_owner_lifecycle() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let runtime = Arc::new(ManagedAgentRuntime::new(
        store.clone(),
        Arc::new(TestTerminalPresentationProjector),
    ));
    runtime
        .execute(RuntimeCommandEnvelope {
            presentation: Vec::new(),
            meta: OperationMeta {
                operation_id: id("mcp-broker-thread-start"),
                idempotency_key: id("mcp-broker-thread-start-key"),
                expected_thread_revision: None,
                actor: RuntimeActor::System {
                    component: "mcp-tool-broker-test".to_string(),
                },
            },
            command: RuntimeCommand::ThreadStart {
                thread_id: id("thread-mcp-broker"),
                presentation_thread_id: id("presentation-thread-mcp-broker"),
                presentation_turn_id: None,
                binding_id: id("binding-mcp-broker"),
                driver_generation: RuntimeDriverGeneration(3),
                source_thread_id: id("source-thread-mcp-broker"),
                profile_digest: id("profile-mcp-broker"),
                bound_profile: Box::new(runtime_profile()),
                input: Vec::new(),
                surface: Box::new(RuntimeSurfaceDescriptor {
                    source_frame_id: "frame-mcp-broker".to_string(),
                    surface_revision: SurfaceRevision(1),
                    surface_digest: id("surface-mcp-broker"),
                    vfs_digest: "vfs-mcp-broker".to_string(),
                    context_recipe_revision: ContextRecipeRevision(1),
                    context_digest: id("context-mcp-broker"),
                    settings_revision: ThreadSettingsRevision(0),
                    tool_set_revision: ToolSetRevision(4),
                    tool_set_digest: "tools-mcp-broker".to_string(),
                    hook_plan: BoundRuntimeHookPlan {
                        revision: HookPlanRevision(1),
                        digest: id("hook-plan-mcp-broker"),
                        entries: Vec::new(),
                    },
                    terminal_hook_effect_binding: None,
                }),
                settings_revision: ThreadSettingsRevision(0),
            },
        })
        .await
        .expect("start MCP broker thread");
    runtime
        .execute(RuntimeCommandEnvelope {
            presentation: Vec::new(),
            meta: OperationMeta {
                operation_id: id("mcp-broker-turn-start"),
                idempotency_key: id("mcp-broker-turn-start-key"),
                expected_thread_revision: Some(RuntimeRevision(3)),
                actor: RuntimeActor::System {
                    component: "mcp-tool-broker-test".to_string(),
                },
            },
            command: RuntimeCommand::TurnStart {
                thread_id: id("thread-mcp-broker"),
                presentation_turn_id: id("presentation-turn-mcp-broker"),
                input: Vec::new(),
            },
        })
        .await
        .expect("start MCP broker turn");

    let tool = ToolContribution {
        meta: tool_meta(),
        runtime_name: "mcp_code_analyzer_scan_repo".to_string(),
        description: "Scan through a true MCP presentation owner".to_string(),
        parameters_schema: serde_json::json!({"type":"object"}),
        capability_key: "mcp:code-analyzer".to_string(),
        tool_path: "mcp:code-analyzer::scan_repo".to_string(),
        allowed_channels: [ToolChannel::McpFacade].into(),
        configuration_boundary: ConfigurationBoundary::Binding,
        protocol_projection: ToolProtocolProjection::Mcp {
            server_key: "code-analyzer".to_string(),
        },
        presentation_emitter: ToolPresentationEmitter::ToolBroker,
        parity_fixture_id: "main_tool_mcp_true_owner_lifecycle".to_string(),
    };
    let invocation = ToolBrokerInvocation {
        coordinates: ToolCallCoordinates {
            thread_id: id("thread-mcp-broker"),
            turn_id: id("turn-mcp-broker-turn-start"),
            item_id: id("item-mcp-broker"),
            presentation_item_id: id("turn_001:tool_001"),
            source_thread_id: id("source-thread-mcp-broker"),
            source_turn_id: id("source-turn-mcp-broker"),
            source_item_id: id("source-item-mcp-broker"),
            binding_id: id("binding-mcp-broker"),
            binding_generation: RuntimeDriverGeneration(3),
            tool_set_revision: ToolSetRevision(4),
        },
        tool_name: tool.runtime_name.clone(),
        arguments: serde_json::json!({"query":"rust","nullable":null}),
        timeout_ms: 1_000,
    };
    let journal = ManagedRuntimeToolJournal::new(runtime.clone());
    journal
        .accept_tool_call(&invocation, &tool)
        .await
        .expect("MCP owner start");
    journal
        .record_tool_update(
            &invocation,
            &tool,
            vec![
                agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                    text: "first".to_string(),
                },
                agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                    text: " second".to_string(),
                },
            ],
        )
        .await
        .expect("MCP progress segments");
    assert!(
        journal
            .record_tool_update(
                &invocation,
                &tool,
                vec![
                    agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                        text: "must-not-publish".to_string(),
                    },
                    agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputImage {
                        image_url: "data:image/png;base64,AA==".to_string(),
                    },
                ],
            )
            .await
            .is_err(),
        "MCP scalar progress must reject image content"
    );
    assert_eq!(
        store
            .read_presentation(
                &id("thread-mcp-broker"),
                Some(RuntimeDriverGeneration(3)),
                None,
            )
            .await
            .len(),
        2,
        "mixed invalid MCP update must publish zero partial segments"
    );
    journal
        .record_tool_terminal(&ToolBrokerCall {
            invocation: invocation.clone(),
            invocation_digest: "sha256:mcp-runtime-journal".to_string(),
            capability_key: tool.capability_key.clone(),
            tool_path: tool.tool_path.clone(),
            tool: tool.clone(),
            channel: ToolChannel::McpFacade,
            status: ToolBrokerCallStatus::Completed,
            effective_arguments: Some(invocation.arguments.clone()),
            pending_interaction_id: None,
            result: Some(ToolBrokerResult {
                output: serde_json::json!({"content":[]}),
                is_error: false,
            }),
            terminal_message: None,
        })
        .await
        .expect("MCP owner terminal");

    let durable = store
        .journal_records_after(&id("thread-mcp-broker"), None)
        .await
        .expect("MCP durable journal")
        .records
        .into_iter()
        .filter_map(|record| record.fact().as_presentation().cloned())
        .collect::<Vec<_>>();
    assert_eq!(durable.len(), 2);
    assert!(matches!(
        durable[0].event,
        agentdash_agent_protocol::BackboneEvent::ItemStarted(_)
    ));
    assert!(matches!(
        durable[1].event,
        agentdash_agent_protocol::BackboneEvent::ItemCompleted(_)
    ));
    for event in &durable {
        assert!(!matches!(
            event.event,
            agentdash_agent_protocol::BackboneEvent::ItemUpdated(_)
        ));
        let value = serde_json::to_value(&event.event).expect("MCP protected body");
        assert_eq!(
            value["payload"]["item"]["arguments"]["nullable"],
            serde_json::Value::Null
        );
        assert_eq!(value["payload"]["item"]["type"], "mcpToolCall");
    }

    let transient = store
        .read_presentation(
            &id("thread-mcp-broker"),
            Some(RuntimeDriverGeneration(3)),
            None,
        )
        .await;
    assert_eq!(transient.len(), 2);
    assert_eq!(
        transient
            .iter()
            .map(
                |record| match &record.as_presentation().expect("MCP progress").event {
                    agentdash_agent_protocol::BackboneEvent::McpToolCallProgress(notification) => {
                        notification.message.as_str()
                    }
                    other => panic!("unexpected MCP progress event: {other:?}"),
                }
            )
            .collect::<Vec<_>>(),
        vec!["first", " second"]
    );
}

#[tokio::test]
async fn managed_runtime_journal_keeps_accepted_tool_alive_across_surface_hot_replace() {
    let store = Arc::new(RuntimeStoreFixture::default());
    let runtime = Arc::new(
        ManagedAgentRuntime::new(store.clone(), Arc::new(TestTerminalPresentationProjector))
            .with_surface_validator(Arc::new(AllowSurface)),
    );
    runtime
        .execute(RuntimeCommandEnvelope {
            presentation: Vec::new(),
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
                presentation_thread_id: id("presentation-thread-broker"),
                presentation_turn_id: None,
                binding_id: id("binding-broker"),
                driver_generation: RuntimeDriverGeneration(3),
                source_thread_id: id("source-thread-broker"),
                profile_digest: id("profile-broker"),
                bound_profile: Box::new(runtime_profile()),
                input: Vec::new(),
                surface: Box::new(RuntimeSurfaceDescriptor {
                    source_frame_id: "frame-broker".to_string(),
                    surface_revision: SurfaceRevision(1),
                    surface_digest: id("surface-broker"),
                    vfs_digest: "vfs-broker".to_string(),
                    context_recipe_revision: ContextRecipeRevision(1),
                    context_digest: id("context-broker"),
                    settings_revision: ThreadSettingsRevision(0),
                    tool_set_revision: ToolSetRevision(4),
                    tool_set_digest: "tools-broker".to_string(),
                    hook_plan: BoundRuntimeHookPlan {
                        revision: HookPlanRevision(1),
                        digest: id("hook-plan-broker"),
                        entries: Vec::new(),
                    },
                    terminal_hook_effect_binding: None,
                }),
                settings_revision: ThreadSettingsRevision(0),
            },
        })
        .await
        .expect("start thread");
    runtime
        .execute(RuntimeCommandEnvelope {
            presentation: Vec::new(),
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
                presentation_turn_id: id("presentation-turn-broker"),
                input: Vec::new(),
            },
        })
        .await
        .expect("start turn");
    let turn_id: RuntimeTurnId = id("turn-broker-turn-start");
    let item_id: RuntimeItemId = id("item-runtime-journal");
    let arguments = serde_json::json!({"path":"crates"});
    let invocation = ToolBrokerInvocation {
        coordinates: ToolCallCoordinates {
            thread_id: id("thread-broker"),
            turn_id: turn_id.clone(),
            item_id: item_id.clone(),
            presentation_item_id: id("turn_001:tool_001"),
            source_thread_id: id("source-thread-broker"),
            source_turn_id: id("source-turn-broker"),
            source_item_id: id("source-item-runtime-journal"),
            binding_id: id("binding-broker"),
            binding_generation: RuntimeDriverGeneration(3),
            tool_set_revision: ToolSetRevision(4),
        },
        tool_name: "code_scan".to_string(),
        arguments: arguments.clone(),
        timeout_ms: 1_000,
    };
    let journal = ManagedRuntimeToolJournal::new(runtime.clone());
    journal
        .accept_tool_call(&invocation, &catalog().tools[0])
        .await
        .expect("broker creates owner-projected item");
    journal
        .accept_tool_call(&invocation, &catalog().tools[0])
        .await
        .expect("repeated accept is idempotent");
    let before_adoption = store
        .load_thread(&id("thread-broker"))
        .await
        .expect("load runtime before surface adoption")
        .expect("runtime thread before surface adoption");
    let mut adopted_surface = before_adoption.surface.clone();
    adopted_surface.source_frame_id = "frame-broker-adopted".to_string();
    adopted_surface.surface_revision = SurfaceRevision(2);
    adopted_surface.surface_digest = id("surface-broker-adopted");
    adopted_surface.vfs_digest = "vfs-broker-adopted".to_string();
    adopted_surface.context_recipe_revision = ContextRecipeRevision(2);
    adopted_surface.context_digest = id("context-broker-adopted");
    adopted_surface.tool_set_revision = ToolSetRevision(5);
    adopted_surface.tool_set_digest = "tools-broker-adopted".to_string();
    runtime
        .execute(RuntimeCommandEnvelope {
            presentation: Vec::new(),
            meta: OperationMeta {
                operation_id: id("broker-surface-adopt"),
                idempotency_key: id("broker-surface-adopt-key"),
                expected_thread_revision: Some(before_adoption.revision),
                actor: RuntimeActor::System {
                    component: "tool-broker-test".to_string(),
                },
            },
            command: RuntimeCommand::SurfaceAdopt {
                thread_id: id("thread-broker"),
                expected_surface_revision: before_adoption.surface.surface_revision,
                expected_surface_digest: before_adoption.surface.surface_digest,
                target: Box::new(adopted_surface),
            },
        })
        .await
        .expect("hot-replace surface while the accepted tool is active");
    let mut stale_new_call = invocation.clone();
    stale_new_call.coordinates.item_id = id("item-stale-after-surface-adoption");
    stale_new_call.coordinates.presentation_item_id =
        id("presentation-item-stale-after-surface-adoption");
    stale_new_call.coordinates.source_item_id = id("source-item-stale-after-surface-adoption");
    assert!(matches!(
        journal
            .accept_tool_call(&stale_new_call, &catalog().tools[0])
            .await,
        Err(ToolBrokerError::StaleCoordinates)
    ));
    let snapshot = store
        .load_thread(&id("thread-broker"))
        .await
        .expect("load runtime")
        .expect("runtime thread");
    assert_eq!(
        snapshot
            .items
            .get(&item_id)
            .expect("broker-started item")
            .initial_content,
        catalog().tools[0]
            .project_started(item_id.as_str(), arguments.clone())
            .expect("owner projector")
    );
    for index in 1..=3 {
        journal
            .record_tool_update(
                &invocation,
                &catalog().tools[0],
                vec![
                    agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                        text: format!("progress-{index}"),
                    },
                ],
            )
            .await
            .expect("tool progress");
    }
    let replay = store
        .read_presentation(&id("thread-broker"), Some(RuntimeDriverGeneration(3)), None)
        .await;
    assert_eq!(replay.len(), 3);
    assert!(replay.iter().all(|record| {
        record.carrier().coordinate.source_thread_id.as_deref() == Some("source-thread-broker")
            && record.carrier().coordinate.source_turn_id.as_deref() == Some("source-turn-broker")
            && record.carrier().coordinate.source_item_id.as_deref()
                == Some("source-item-runtime-journal")
    }));
    assert_eq!(
        replay
            .iter()
            .map(|record| record
                .carrier()
                .transient
                .as_ref()
                .expect("transient coordinate")
                .sequence
                .0)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    let event_ids = replay
        .iter()
        .map(|record| {
            record
                .carrier()
                .transient
                .as_ref()
                .expect("transient coordinate")
                .event_id
                .clone()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(event_ids.len(), 3);
    assert_eq!(
        store
            .read_presentation(
                &id("thread-broker"),
                Some(RuntimeDriverGeneration(3)),
                Some(RuntimeTransientSequence(1)),
            )
            .await
            .len(),
        2
    );
    let call = ToolBrokerCall {
        invocation,
        invocation_digest: "sha256:runtime-journal".to_string(),
        capability_key: "mcp:code".to_string(),
        tool_path: "mcp:code::scan".to_string(),
        tool: catalog().tools[0].clone(),
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
        .internal_events_after(&id("thread-broker"), None)
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
    let presentation_records = store
        .journal_records_after(&id("thread-broker"), None)
        .await
        .expect("presentation journal")
        .records
        .into_iter()
        .filter(|record| matches!(record.fact(), RuntimeJournalFact::Presentation(_)))
        .collect::<Vec<_>>();
    assert!(presentation_records.iter().all(|record| {
        record.carrier().coordinate.runtime_item_id.as_ref() == Some(&item_id)
            && record.carrier().coordinate.source_thread_id.as_deref()
                == Some("source-thread-broker")
            && record.carrier().coordinate.source_turn_id.as_deref() == Some("source-turn-broker")
            && record.carrier().coordinate.source_item_id.as_deref()
                == Some("source-item-runtime-journal")
    }));
    let presentation = presentation_records
        .into_iter()
        .filter_map(|record| record.as_presentation().map(|event| event.event.clone()))
        .collect::<Vec<_>>();
    assert_eq!(presentation.len(), 2);
    assert!(matches!(
        presentation[0],
        agentdash_agent_protocol::BackboneEvent::ItemStarted(_)
    ));
    assert!(matches!(
        presentation[1],
        agentdash_agent_protocol::BackboneEvent::ItemCompleted(_)
    ));
    let presentation_item_ids = presentation
        .iter()
        .map(|event| match event {
            agentdash_agent_protocol::BackboneEvent::ItemStarted(notification) => {
                notification.item.id()
            }
            agentdash_agent_protocol::BackboneEvent::ItemCompleted(notification) => {
                notification.item.id()
            }
            other => panic!("unexpected broker presentation event: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(presentation_item_ids, vec!["turn_001:tool_001"; 2]);
    assert_ne!(presentation_item_ids[0], item_id.as_str());

    let vendor_item_id: RuntimeItemId = id("item-vendor-stream");
    let vendor_invocation = ToolBrokerInvocation {
        coordinates: ToolCallCoordinates {
            thread_id: id("thread-broker"),
            turn_id,
            item_id: vendor_item_id.clone(),
            presentation_item_id: id("turn_001:tool_002"),
            source_thread_id: id("source-thread-broker"),
            source_turn_id: id("source-turn-broker"),
            source_item_id: id("source-item-vendor-stream"),
            binding_id: id("binding-broker"),
            binding_generation: RuntimeDriverGeneration(3),
            tool_set_revision: ToolSetRevision(5),
        },
        tool_name: "code_scan".to_string(),
        arguments: serde_json::json!({"path":"vendor"}),
        timeout_ms: 1_000,
    };
    let mut vendor_tool = catalog().tools[0].clone();
    vendor_tool.presentation_emitter = ToolPresentationEmitter::VendorStream;
    journal
        .accept_tool_call(&vendor_invocation, &vendor_tool)
        .await
        .expect("vendor stream tool still creates the canonical internal item");
    journal
        .record_tool_update(
            &vendor_invocation,
            &vendor_tool,
            vec![
                agentdash_agent_protocol::DynamicToolCallOutputContentItem::InputText {
                    text: "vendor progress".to_string(),
                },
            ],
        )
        .await
        .expect("vendor stream owns its progress presentation");
    journal
        .record_tool_terminal(&ToolBrokerCall {
            invocation: vendor_invocation,
            invocation_digest: "sha256:vendor-runtime-journal".to_string(),
            capability_key: vendor_tool.capability_key.clone(),
            tool_path: vendor_tool.tool_path.clone(),
            tool: vendor_tool,
            channel: ToolChannel::DirectCallback,
            status: ToolBrokerCallStatus::Completed,
            effective_arguments: Some(serde_json::json!({"path":"vendor"})),
            pending_interaction_id: None,
            result: Some(ToolBrokerResult {
                output: serde_json::json!({"matches":1}),
                is_error: false,
            }),
            terminal_message: None,
        })
        .await
        .expect("vendor stream tool still terminalizes the canonical internal item");

    let snapshot = store
        .load_thread(&id("thread-broker"))
        .await
        .expect("load vendor stream runtime")
        .expect("vendor stream runtime thread");
    assert!(
        snapshot
            .items
            .get(&vendor_item_id)
            .is_some_and(|item| matches!(item.phase, EntityPhase::Terminal(_)))
    );
    assert_eq!(
        store
            .journal_records_after(&id("thread-broker"), None)
            .await
            .expect("presentation journal after vendor stream tool")
            .records
            .into_iter()
            .filter(|record| matches!(record.fact(), RuntimeJournalFact::Presentation(_)))
            .count(),
        2,
        "vendor-owned start and terminal must not be duplicated by ToolBroker",
    );
    assert_eq!(
        store
            .read_presentation(&id("thread-broker"), Some(RuntimeDriverGeneration(3)), None)
            .await
            .len(),
        3,
        "vendor-owned progress must not be duplicated by ToolBroker",
    );
}
