use super::*;
use agentdash_application_ports::operation_script::{
    OperationOriginRef, OperationPrincipalRef, OperationScopeRef, OperationScriptOperationResult,
};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

struct FixtureExecutor {
    active: AtomicUsize,
    peak: AtomicUsize,
}

struct SkillInventoryExecutor;

#[async_trait]
impl OperationScriptOperationExecutor for SkillInventoryExecutor {
    async fn execute(
        &self,
        call: OperationScriptOperationCall,
        _: CancellationToken,
    ) -> Result<OperationScriptOperationResult, OperationScriptError> {
        let value = match call.operation_ref.operation_key.as_str() {
            "fs_glob" => {
                let mut paths = vec![".agents/skills/alpha/SKILL.md".to_owned()];
                paths.extend((1..100).map(|index| format!("skills/skill-{index:03}/SKILL.md")));
                serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": paths.join("\n")
                    }],
                    "is_error": false
                })
            }
            "fs_read" => {
                let path = call.input["path"].as_str().unwrap_or_default();
                serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("---\nname: {path}\ndescription: fixture\n---")
                    }],
                    "is_error": false
                })
            }
            _ => unreachable!("exact fixture manifest"),
        };
        Ok(OperationScriptOperationResult {
            value,
            outcome_unknown: false,
        })
    }
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
fn engine() -> RhaiOperationScriptEngine {
    RhaiOperationScriptEngine::new(RhaiOperationScriptConfig::default()).expect("engine")
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
fn allowed() -> OperationRef {
    OperationRef::new("agentdash", "fixture", "echo", 1).expect("ref")
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
) -> Result<OperationScriptOutcome, OperationScriptError> {
    engine
        .execute(
            OperationScriptExecuteRequest { program, context },
            executor,
            CancellationToken::new(),
        )
        .await
}

#[tokio::test]
async fn plain_execute_uses_unique_execution_id() {
    let engine = engine();
    let context = context();
    let program = program("input.values");
    let first = engine
        .execute(
            OperationScriptExecuteRequest {
                program: program.clone(),
                context: context.clone(),
            },
            executor(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let second = engine
        .execute(
            OperationScriptExecuteRequest { program, context },
            executor(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_ne!(first.execution_id, second.execution_id);
}

#[tokio::test]
async fn invoke_and_invoke_all_are_exact_ordered_and_bounded() {
    let engine = engine();
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
async fn canvas_skill_inventory_script_globs_then_reads_in_parallel() {
    let glob = OperationRef::new("platform", "vfs", "fs_glob", 1).expect("glob ref");
    let read = OperationRef::new("platform", "vfs", "fs_read", 1).expect("read ref");
    let mut program = program(
        r#"
            fn frontmatter_field(text, key) {
                let prefix = key + ":";
                for line in text.split("\n") {
                    if line.starts_with(prefix) {
                        let conventional = prefix + " ";
                        if line.starts_with(conventional) {
                            let value = line;
                            value.replace(conventional, "");
                            return value;
                        }
                        let value = line;
                        value.replace(prefix, "");
                        return value;
                    }
                }
                ""
            }

            let found = ops.invoke(
                "platform:vfs:fs_glob:v1",
                #{ path: "main://", pattern: "**/SKILL.md" }
            );
            let requests = [];
            for path in found.content[0].text.split("\n") {
                if path.ends_with("SKILL.md") {
                    requests.push(#{
                        operation: "platform:vfs:fs_read:v1",
                        input: #{ path: "main://" + path }
                    });
                }
            }
            let files = ops.invoke_all(requests);
            let skills = [];
            for index in 0..files.len() {
                let text = files[index].content[0].text;
                skills.push(#{
                    path: requests[index].input.path,
                    name: frontmatter_field(text, "name"),
                    description: frontmatter_field(text, "description")
                });
            }
            #{
                skills: skills
            }
        "#,
    );
    program.allowed_operations = vec![glob, read];

    let outcome = run(
        &engine(),
        program,
        context(),
        Arc::new(SkillInventoryExecutor),
    )
    .await
    .expect("skill inventory script");
    let OperationScriptResultValue::Inline { value } = outcome.value else {
        panic!("inline result");
    };

    assert_eq!(outcome.calls.len(), 101);
    assert_eq!(value["skills"].as_array().map(Vec::len), Some(100));
    assert_eq!(
        value["skills"][0]["path"],
        "main://.agents/skills/alpha/SKILL.md"
    );
    assert_eq!(
        value["skills"][0]["name"],
        "main://.agents/skills/alpha/SKILL.md"
    );
    assert_eq!(value["skills"][0]["description"], "fixture");
    assert_eq!(
        value["skills"][99]["path"],
        "main://skills/skill-099/SKILL.md"
    );
}

#[tokio::test]
async fn call_limit_failure_keeps_completed_evidence() {
    let mut program = program(
        r#"ops.invoke("agentdash:fixture:echo:v1", #{}); ops.invoke("agentdash:fixture:echo:v1", #{})"#,
    );
    program.allowed_operations.push(allowed());
    program.limits.max_operation_calls = 1;
    let error = run(&engine(), program, context(), executor())
        .await
        .unwrap_err();
    assert!(
        matches!(error, OperationScriptError::ExecutionFailed { calls, partial: true, .. } if calls.len() == 1)
    );
}

#[tokio::test]
async fn nested_call_cancellation_keeps_outcome_unknown_evidence() {
    let engine = Arc::new(engine());
    let mut program = program(r#"ops.invoke("agentdash:fixture:echo:v1", #{})"#);
    program.allowed_operations.push(allowed());
    let context = context();
    let cancel = CancellationToken::new();
    let executor = executor();
    let task = {
        let engine = engine.clone();
        let cancel = cancel.clone();
        let executor = executor.clone();
        tokio::spawn(async move {
            engine
                .execute(
                    OperationScriptExecuteRequest { program, context },
                    executor,
                    cancel,
                )
                .await
        })
    };
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while executor.active.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("nested operation should start before cancellation");
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
        &engine(),
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
    let error = run(&engine(), program, context(), executor())
        .await
        .unwrap_err();
    assert!(
        matches!(error, OperationScriptError::ExecutionFailed { calls, partial: true, outcome_unknown: true, .. } if calls.len() == 2)
    );
}

#[tokio::test]
async fn ast_cache_evicts_by_entry_and_source_budget() {
    let config = RhaiOperationScriptConfig {
        max_ast_cache_entries: 2,
        max_ast_cache_source_bytes: 16,
        ..Default::default()
    };
    let engine = RhaiOperationScriptEngine::new(config).unwrap();
    for source in ["1 + 1", "2 + 2", "3 + 3"] {
        engine
            .execute(
                OperationScriptExecuteRequest {
                    program: program(source),
                    context: context(),
                },
                executor(),
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
    let config = RhaiOperationScriptConfig {
        max_inline_result_bytes: 4,
        ..Default::default()
    };
    let engine = RhaiOperationScriptEngine::new(config).unwrap();
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
    let config = RhaiOperationScriptConfig {
        max_inline_result_bytes: 4,
        result_ttl: Duration::milliseconds(1),
        ..Default::default()
    };
    let engine = RhaiOperationScriptEngine::new(config).unwrap();
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
