use super::*;
use agentdash_application_ports::operation_script::{
    OperationEffect, OperationOriginRef, OperationPrincipalRef, OperationReplayPolicy,
    OperationScopeRef, OperationScriptOperationResult,
};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

struct FixtureExecutor {
    active: AtomicUsize,
    peak: AtomicUsize,
}
#[async_trait]
impl OperationScriptOperationExecutor for FixtureExecutor {
    async fn execute(
        &self,
        call: OperationScriptOperationCall,
        cancel: CancellationToken,
    ) -> Result<OperationScriptOperationResult, OperationScriptError> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
        tokio::select! { _ = cancel.cancelled() => { self.active.fetch_sub(1, Ordering::SeqCst); return Err(OperationScriptError::NestedOperation { code: "cancelled".into(), outcome_unknown: true }); }, _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {} }
        self.active.fetch_sub(1, Ordering::SeqCst);
        if call.input.get("fail").and_then(Value::as_bool) == Some(true) {
            return Err(OperationScriptError::NestedOperation {
                code: "provider_failed".into(),
                outcome_unknown: true,
            });
        }
        Ok(OperationScriptOperationResult {
            value: serde_json::json!({"index":call.call_index,"input":call.input}),
            outcome_unknown: false,
        })
    }
}
fn executor() -> Arc<FixtureExecutor> {
    Arc::new(FixtureExecutor {
        active: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
    })
}
fn engine(secret: u8) -> RhaiOperationScriptEngine {
    RhaiOperationScriptEngine::new(&[secret; 32], RhaiOperationScriptConfig::default())
        .expect("engine")
}
fn context() -> OperationScriptExecutionContext {
    OperationScriptExecutionContext {
        principal: OperationPrincipalRef::User {
            user_id: "u".into(),
        },
        scope: OperationScopeRef::Project {
            project_id: Uuid::from_u128(1),
        },
        authority_revision: "authority:1".into(),
        granted_capabilities: BTreeSet::from(["read".into()]),
        origin: OperationOriginRef::UserWorkshop,
        trace_id: "trace-1".into(),
        attachment_ref: None,
    }
}
fn allowed() -> OperationScriptAllowedOperation {
    OperationScriptAllowedOperation {
        operation_ref: agentdash_domain::operation::OperationRef::new(
            "agentdash",
            "fixture",
            "echo",
            1,
        )
        .expect("ref"),
        descriptor_digest: sha256(b"descriptor"),
        effect: OperationEffect::Read,
        replay_policy: OperationReplayPolicy::ReplaySafe,
        recursive_operation_script: false,
    }
}
fn program(source: &str) -> OperationScriptProgram {
    OperationScriptProgram {
        dialect: RHAI_V1_DIALECT.into(),
        host_api_version: OPERATION_SCRIPT_HOST_API_V1,
        source: source.into(),
        input: serde_json::json!({"values":[1,2,3]}),
        allowed_operations: vec![],
        limits: OperationScriptLimits::default(),
    }
}
async fn run(
    engine: &RhaiOperationScriptEngine,
    program: OperationScriptProgram,
    context: OperationScriptExecutionContext,
    executor: Arc<dyn OperationScriptOperationExecutor>,
) -> Result<OperationScriptRunOutcome, OperationScriptError> {
    let plan = engine
        .preflight(
            OperationScriptPreflightRequest {
                program: program.clone(),
                context: context.clone(),
            },
            CancellationToken::new(),
        )
        .await
        .expect("preflight");
    engine
        .run(
            OperationScriptRunRequest {
                program,
                context,
                token: plan.token,
            },
            executor,
            CancellationToken::new(),
        )
        .await
}

#[tokio::test]
async fn plain_run_uses_unique_execution_id_per_token() {
    let engine = engine(7);
    let context = context();
    let program = program("input.values");
    let plan = engine
        .preflight(
            OperationScriptPreflightRequest {
                program: program.clone(),
                context: context.clone(),
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let first = engine
        .run(
            OperationScriptRunRequest {
                program: program.clone(),
                context: context.clone(),
                token: plan.token.clone(),
            },
            executor(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let second = engine
        .run(
            OperationScriptRunRequest {
                program,
                context,
                token: plan.token.clone(),
            },
            executor(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_ne!(first.execution_id, second.execution_id);
    assert_eq!(first.plan_id, plan.token.plan_id);
}

#[tokio::test]
async fn invoke_and_invoke_all_are_exact_ordered_and_bounded() {
    let engine = engine(7);
    let mut program = program(
        r#"let one = ops.invoke("agentdash:fixture:echo:v1", #{value: 1}); let many = ops.invoke_all([#{operation:"agentdash:fixture:echo:v1",input:#{value:2}},#{operation:"agentdash:fixture:echo:v1",input:#{value:3}},#{operation:"agentdash:fixture:echo:v1",input:#{value:4}}]); #{one:one,many:many}"#,
    );
    program.allowed_operations.push(allowed());
    program.limits.max_parallel_operations = 2;
    let executor = executor();
    let outcome = run(&engine, program, context(), executor.clone())
        .await
        .unwrap();
    assert_eq!(outcome.calls.len(), 4);
    assert!(executor.peak.load(Ordering::SeqCst) <= 2);
    let OperationScriptResultValue::Inline { value } = outcome.value else {
        panic!("inline")
    };
    assert_eq!(value["many"][1]["input"]["value"], 3);
    assert_eq!(value["many"][2]["input"]["value"], 4);
}

#[tokio::test]
async fn call_limit_failure_keeps_completed_evidence() {
    let mut program = program(
        r#"ops.invoke("agentdash:fixture:echo:v1", #{}); ops.invoke("agentdash:fixture:echo:v1", #{})"#,
    );
    program.allowed_operations.push(allowed());
    program.limits.max_operation_calls = 1;
    let error = run(&engine(7), program, context(), executor())
        .await
        .unwrap_err();
    assert!(
        matches!(error, OperationScriptError::ExecutionFailed { calls, partial: true, .. } if calls.len() == 1)
    );
}

#[tokio::test]
async fn nested_call_cancellation_keeps_outcome_unknown_evidence() {
    let engine = Arc::new(engine(7));
    let mut program = program(r#"ops.invoke("agentdash:fixture:echo:v1", #{})"#);
    program.allowed_operations.push(allowed());
    let context = context();
    let plan = engine
        .preflight(
            OperationScriptPreflightRequest {
                program: program.clone(),
                context: context.clone(),
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let task = {
        let engine = engine.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            engine
                .run(
                    OperationScriptRunRequest {
                        program,
                        context,
                        token: plan.token,
                    },
                    executor(),
                    cancel,
                )
                .await
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    cancel.cancel();
    let error = task.await.unwrap().unwrap_err();
    assert!(
        matches!(error, OperationScriptError::ExecutionFailed { calls, outcome_unknown: true, .. } if calls.len() == 1)
    );
}

#[tokio::test]
async fn denied_operation_is_not_dispatched() {
    let executor = executor();
    let error = run(
        &engine(7),
        program(r#"ops.invoke("agentdash:fixture:echo:v1", #{})"#),
        context(),
        executor.clone(),
    )
    .await
    .unwrap_err();
    assert_eq!(executor.peak.load(Ordering::SeqCst), 0);
    assert!(
        matches!(error, OperationScriptError::ExecutionFailed { calls, .. } if calls.is_empty())
    );
}

#[tokio::test]
async fn failure_keeps_partial_and_outcome_unknown_evidence() {
    let mut program = program(
        r#"ops.invoke("agentdash:fixture:echo:v1", #{ok:true}); ops.invoke("agentdash:fixture:echo:v1", #{fail:true})"#,
    );
    program.allowed_operations.push(allowed());
    let error = run(&engine(7), program, context(), executor())
        .await
        .unwrap_err();
    assert!(
        matches!(error, OperationScriptError::ExecutionFailed { calls, partial: true, outcome_unknown: true, .. } if calls.len() == 2)
    );
}

#[tokio::test]
async fn ast_cache_evicts_by_entry_and_source_budget() {
    let mut config = RhaiOperationScriptConfig::default();
    config.max_ast_cache_entries = 2;
    config.max_ast_cache_source_bytes = 16;
    let engine = RhaiOperationScriptEngine::new(&[7; 32], config).unwrap();
    for source in ["1 + 1", "2 + 2", "3 + 3"] {
        let program = program(source);
        engine
            .preflight(
                OperationScriptPreflightRequest {
                    program,
                    context: context(),
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
    }
    let cache = engine.ast_cache.read().unwrap();
    assert!(cache.entries.len() <= 2);
    assert!(cache.source_bytes <= 16);
}

#[tokio::test]
async fn large_result_ref_is_scoped_and_rechecks_capabilities() {
    let mut config = RhaiOperationScriptConfig::default();
    config.max_inline_result_bytes = 4;
    let engine = RhaiOperationScriptEngine::new(&[7; 32], config).unwrap();
    let context = context();
    let outcome = run(
        &engine,
        program(r#""large-result""#),
        context.clone(),
        executor(),
    )
    .await
    .unwrap();
    let OperationScriptResultValue::Ref { result_ref } = outcome.value else {
        panic!("ref")
    };
    assert_eq!(
        engine
            .resolve_result(&result_ref, &context, CancellationToken::new())
            .await
            .unwrap(),
        Some(Value::String("large-result".into()))
    );
    let mut denied = context;
    denied.granted_capabilities.clear();
    assert_eq!(
        engine
            .resolve_result(&result_ref, &denied, CancellationToken::new())
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn scoped_result_ref_expires_without_bearer_access() {
    let mut config = RhaiOperationScriptConfig::default();
    config.max_inline_result_bytes = 4;
    config.result_ttl = Duration::milliseconds(1);
    let engine = RhaiOperationScriptEngine::new(&[7; 32], config).unwrap();
    let context = context();
    let outcome = run(
        &engine,
        program(r#""large-result""#),
        context.clone(),
        executor(),
    )
    .await
    .unwrap();
    let OperationScriptResultValue::Ref { result_ref } = outcome.value else {
        panic!("ref")
    };
    tokio::time::sleep(std::time::Duration::from_millis(3)).await;
    assert_eq!(
        engine
            .resolve_result(&result_ref, &context, CancellationToken::new())
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn secret_rotation_invalidates_plan() {
    let first = engine(7);
    let second = engine(8);
    let context = context();
    let program = program("input");
    let plan = first
        .preflight(
            OperationScriptPreflightRequest {
                program: program.clone(),
                context: context.clone(),
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let error = second
        .run(
            OperationScriptRunRequest {
                program,
                context,
                token: plan.token,
            },
            executor(),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        OperationScriptError::InvalidPlan {
            reason: "signature_mismatch"
        }
    ));
}

#[test]
fn mac_comparison_is_constant_shape() {
    assert!(constant_time_eq(b"same", b"same"));
    assert!(!constant_time_eq(b"same", b"diff"));
    assert!(!constant_time_eq(b"same", b"same-longer"));
}
