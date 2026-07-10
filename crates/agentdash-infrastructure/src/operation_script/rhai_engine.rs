use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use agentdash_application_ports::operation_script::{
    OPERATION_SCRIPT_HOST_API_V1, OperationScriptEngine, OperationScriptError,
    OperationScriptLimits, OperationScriptOperationExecutor, OperationScriptPreflightRequest,
    OperationScriptPreflightResult, OperationScriptPreflightToken, OperationScriptProgram,
    OperationScriptResultAccess, OperationScriptRunOutcome, OperationScriptRunRequest,
    RHAI_V1_DIALECT,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use rhai::{AST, Dynamic, Engine, Scope};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::script_runtime::{RhaiScriptLimits, RhaiScriptRuntime};

#[derive(Debug, Clone)]
pub struct RhaiOperationScriptConfig {
    pub max_concurrent_scripts: usize,
    pub preflight_ttl: Duration,
    pub cancellation_grace: Duration,
    pub result_ttl: Duration,
    pub maximum_limits: OperationScriptLimits,
}

impl Default for RhaiOperationScriptConfig {
    fn default() -> Self {
        Self {
            max_concurrent_scripts: 4,
            preflight_ttl: Duration::minutes(5),
            cancellation_grace: Duration::seconds(2),
            result_ttl: Duration::minutes(10),
            maximum_limits: OperationScriptLimits::default(),
        }
    }
}

pub struct RhaiOperationScriptEngine {
    config: RhaiOperationScriptConfig,
    signing_secret: Arc<[u8]>,
    permits: Arc<Semaphore>,
    ast_cache: Arc<RwLock<HashMap<String, AST>>>,
}

impl RhaiOperationScriptEngine {
    pub fn new(
        signing_secret: &[u8],
        config: RhaiOperationScriptConfig,
    ) -> Result<Self, OperationScriptError> {
        if signing_secret.len() < 32 {
            return Err(invalid("signing_secret", "至少需要 32 bytes"));
        }
        if config.max_concurrent_scripts == 0
            || config.preflight_ttl <= Duration::zero()
            || config.cancellation_grace <= Duration::zero()
            || config.result_ttl <= Duration::zero()
        {
            return Err(invalid("engine_config", "并发数与 duration 必须大于 0"));
        }
        Ok(Self {
            permits: Arc::new(Semaphore::new(config.max_concurrent_scripts)),
            signing_secret: Arc::from(signing_secret),
            ast_cache: Arc::new(RwLock::new(HashMap::new())),
            config,
        })
    }

    fn acquire(&self) -> Result<OwnedSemaphorePermit, OperationScriptError> {
        self.permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| OperationScriptError::CapacityExceeded)
    }

    fn verify_token(
        &self,
        request: &OperationScriptRunRequest,
        now: DateTime<Utc>,
    ) -> Result<PlanDigests, OperationScriptError> {
        if request.token.expires_at <= now {
            return Err(OperationScriptError::TokenExpired);
        }
        let digests = plan_digests(&request.program, &request.context)?;
        if request.token.binding_digest != digests.binding_digest {
            return Err(OperationScriptError::InvalidPlan {
                reason: "binding_digest_mismatch",
            });
        }
        let expected = sign_token(&self.signing_secret, &request.token)?;
        if !constant_time_eq(expected.as_bytes(), request.token.signature.as_bytes()) {
            return Err(OperationScriptError::InvalidPlan {
                reason: "signature_mismatch",
            });
        }
        Ok(digests)
    }
}

#[async_trait]
impl OperationScriptEngine for RhaiOperationScriptEngine {
    async fn preflight(
        &self,
        request: OperationScriptPreflightRequest,
        cancel: CancellationToken,
    ) -> Result<OperationScriptPreflightResult, OperationScriptError> {
        validate_program(&request.program, &self.config.maximum_limits)?;
        validate_context(&request.context)?;
        let digests = plan_digests(&request.program, &request.context)?;
        let permit = self.acquire()?;
        let source = request.program.source.clone();
        let source_digest = digests.source_digest.clone();
        let limits = request.program.limits;
        let cache = self.ast_cache.clone();
        let worker = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut engine = Engine::new();
            RhaiScriptRuntime::apply_limits(&mut engine, rhai_limits(limits));
            let ast = engine
                .compile(&source)
                .map_err(|error| OperationScriptError::Compile {
                    diagnostic: bounded_diagnostic(&error.to_string()),
                })?;
            cache
                .write()
                .map_err(|_| OperationScriptError::Internal {
                    code: "ast_cache_poisoned",
                })?
                .insert(source_digest, ast);
            Ok(())
        });
        await_worker(
            worker,
            cancel.clone(),
            cancel,
            Utc::now() + self.config.preflight_ttl,
            self.config.cancellation_grace,
            false,
        )
        .await?;

        let issued_at = Utc::now();
        let mut token = OperationScriptPreflightToken {
            plan_id: Uuid::new_v4(),
            binding_digest: digests.binding_digest,
            issued_at,
            expires_at: issued_at + self.config.preflight_ttl,
            signature: String::new(),
        };
        token.signature = sign_token(&self.signing_secret, &token)?;
        Ok(OperationScriptPreflightResult {
            token,
            source_digest: digests.source_digest,
            manifest_digest: digests.manifest_digest,
        })
    }

    async fn run(
        &self,
        request: OperationScriptRunRequest,
        _operation_executor: Arc<dyn OperationScriptOperationExecutor>,
        cancel: CancellationToken,
    ) -> Result<OperationScriptRunOutcome, OperationScriptError> {
        validate_program(&request.program, &self.config.maximum_limits)?;
        validate_context(&request.context)?;
        let now = Utc::now();
        let digests = self.verify_token(&request, now)?;
        let timeout_ms = i64::try_from(request.program.limits.timeout_ms)
            .map_err(|_| invalid("limits.timeout_ms", "超出 i64"))?;
        let deadline = std::cmp::min(
            now + Duration::milliseconds(timeout_ms),
            request.token.expires_at,
        );
        let permit = self.acquire()?;
        let execution_cancel = cancel.child_token();
        let worker_cancel = execution_cancel.clone();
        let source = request.program.source.clone();
        let input = request.program.input.clone();
        let limits = request.program.limits;
        let source_digest = digests.source_digest;
        let cache = self.ast_cache.clone();
        let worker = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut engine = Engine::new();
            RhaiScriptRuntime::apply_limits(&mut engine, rhai_limits(limits));
            let progress_cancel = worker_cancel.clone();
            engine.on_progress(move |_| {
                if progress_cancel.is_cancelled() || Utc::now() >= deadline {
                    Some(Dynamic::from("operation_script_interrupted"))
                } else {
                    None
                }
            });
            let ast = cache
                .read()
                .ok()
                .and_then(|cache| cache.get(&source_digest).cloned())
                .map(Ok)
                .unwrap_or_else(|| {
                    engine
                        .compile(&source)
                        .map_err(|error| OperationScriptError::Compile {
                            diagnostic: bounded_diagnostic(&error.to_string()),
                        })
                })?;
            let mut scope = Scope::new();
            scope.push_dynamic("input", crate::script_runtime::json_to_dynamic(&input));
            let value: Dynamic = engine
                .eval_ast_with_scope(&mut scope, &ast)
                .map_err(|error| OperationScriptError::Runtime {
                    diagnostic: bounded_diagnostic(&error.to_string()),
                })?;
            if worker_cancel.is_cancelled() {
                return Err(OperationScriptError::Cancelled);
            }
            if Utc::now() >= deadline {
                return Err(OperationScriptError::DeadlineExceeded);
            }
            if value.is_unit() {
                Ok(Value::Null)
            } else {
                rhai::serde::from_dynamic(&value).map_err(|error| OperationScriptError::Runtime {
                    diagnostic: bounded_diagnostic(&error.to_string()),
                })
            }
        });
        let value = await_worker(
            worker,
            cancel,
            execution_cancel,
            deadline,
            self.config.cancellation_grace,
            true,
        )
        .await?;
        let output_bytes = serde_json::to_vec(&value)
            .map_err(|_| OperationScriptError::Internal {
                code: "output_serialize",
            })?
            .len();
        if output_bytes > request.program.limits.max_output_bytes {
            return Err(OperationScriptError::OutputLimitExceeded {
                actual: output_bytes,
                maximum: request.program.limits.max_output_bytes,
            });
        }
        let expires_at = std::cmp::min(
            request.token.expires_at,
            Utc::now() + self.config.result_ttl,
        );
        Ok(OperationScriptRunOutcome {
            execution_id: request.token.plan_id,
            value,
            calls: Vec::new(),
            partial: false,
            outcome_unknown: false,
            result_access: OperationScriptResultAccess {
                principal: request.context.principal,
                scope: request.context.scope,
                authority_revision: request.context.authority_revision,
                expires_at,
            },
        })
    }
}

#[derive(Serialize)]
struct PlanBinding<'a> {
    dialect: &'a str,
    host_api_version: u16,
    source_digest: &'a str,
    input_digest: &'a str,
    manifest_digest: &'a str,
    limits: OperationScriptLimits,
    context: &'a agentdash_application_ports::operation_script::OperationScriptExecutionContext,
}

struct PlanDigests {
    source_digest: String,
    manifest_digest: String,
    binding_digest: String,
}

fn plan_digests(
    program: &OperationScriptProgram,
    context: &agentdash_application_ports::operation_script::OperationScriptExecutionContext,
) -> Result<PlanDigests, OperationScriptError> {
    let source_digest = sha256(program.source.as_bytes());
    let input_digest = digest_json(&program.input, "input")?;
    let mut manifest = program.allowed_operations.clone();
    manifest.sort_by_key(|entry| entry.script_key());
    let manifest_digest = digest_json(&manifest, "allowed_operations")?;
    let binding = PlanBinding {
        dialect: &program.dialect,
        host_api_version: program.host_api_version,
        source_digest: &source_digest,
        input_digest: &input_digest,
        manifest_digest: &manifest_digest,
        limits: program.limits,
        context,
    };
    let binding_digest = digest_json(&binding, "plan_binding")?;
    Ok(PlanDigests {
        source_digest,
        manifest_digest,
        binding_digest,
    })
}

fn validate_program(
    program: &OperationScriptProgram,
    maximum: &OperationScriptLimits,
) -> Result<(), OperationScriptError> {
    if program.dialect != RHAI_V1_DIALECT {
        return Err(invalid("dialect", "只支持 rhai_v1"));
    }
    if program.host_api_version != OPERATION_SCRIPT_HOST_API_V1 {
        return Err(invalid("host_api_version", "只支持 V1 host API"));
    }
    if program.source.is_empty() || program.source.len() > program.limits.max_source_bytes {
        return Err(invalid("source", "source 为空或超过请求限制"));
    }
    let input_bytes = serde_json::to_vec(&program.input)
        .map_err(|_| OperationScriptError::Internal {
            code: "input_serialize",
        })?
        .len();
    if input_bytes > program.limits.max_input_bytes {
        return Err(invalid("input", "input 超过请求限制"));
    }
    validate_limits(&program.limits, maximum)?;
    let mut keys = HashSet::new();
    for operation in &program.allowed_operations {
        operation
            .operation_ref
            .validate()
            .map_err(|error| invalid("allowed_operations", &error.to_string()))?;
        if operation.recursive_operation_script {
            return Err(invalid(
                "allowed_operations",
                "rhai_v1 禁止递归 OperationScript",
            ));
        }
        if !valid_sha256(&operation.descriptor_digest) {
            return Err(invalid(
                "allowed_operations.descriptor_digest",
                "必须是 sha256 digest",
            ));
        }
        if !keys.insert(operation.script_key()) {
            return Err(invalid("allowed_operations", "OperationRef 必须唯一"));
        }
    }
    Ok(())
}

fn validate_context(
    context: &agentdash_application_ports::operation_script::OperationScriptExecutionContext,
) -> Result<(), OperationScriptError> {
    context
        .principal
        .validate()
        .map_err(|error| invalid("context.principal", &error.to_string()))?;
    context
        .scope
        .validate()
        .map_err(|error| invalid("context.scope", &error.to_string()))?;
    context
        .origin
        .validate()
        .map_err(|error| invalid("context.origin", &error.to_string()))?;
    if context.authority_revision.trim().is_empty()
        || context.authority_revision.trim() != context.authority_revision
        || context.trace_id.trim().is_empty()
        || context.trace_id.trim() != context.trace_id
        || context
            .attachment_ref
            .as_ref()
            .is_some_and(|value| value.trim().is_empty() || value.trim() != value)
    {
        return Err(invalid(
            "context",
            "authority revision、trace 与 attachment ref 必须已规范化",
        ));
    }
    Ok(())
}

fn validate_limits(
    limits: &OperationScriptLimits,
    maximum: &OperationScriptLimits,
) -> Result<(), OperationScriptError> {
    let valid = limits.timeout_ms > 0
        && limits.max_source_bytes > 0
        && limits.max_input_bytes > 0
        && limits.max_output_bytes > 0
        && limits.max_rhai_operations > 0
        && limits.max_call_levels > 0
        && limits.max_string_size > 0
        && limits.max_array_size > 0
        && limits.max_map_size > 0
        && limits.max_operation_calls > 0
        && limits.max_parallel_operations > 0
        && limits.timeout_ms <= maximum.timeout_ms
        && limits.max_source_bytes <= maximum.max_source_bytes
        && limits.max_input_bytes <= maximum.max_input_bytes
        && limits.max_output_bytes <= maximum.max_output_bytes
        && limits.max_rhai_operations <= maximum.max_rhai_operations
        && limits.max_call_levels <= maximum.max_call_levels
        && limits.max_string_size <= maximum.max_string_size
        && limits.max_array_size <= maximum.max_array_size
        && limits.max_map_size <= maximum.max_map_size
        && limits.max_operation_calls <= maximum.max_operation_calls
        && limits.max_parallel_operations <= maximum.max_parallel_operations;
    if valid {
        Ok(())
    } else {
        Err(invalid(
            "limits",
            "所有 limits 必须大于 0 且不超过 engine maximum",
        ))
    }
}

fn rhai_limits(limits: OperationScriptLimits) -> RhaiScriptLimits {
    RhaiScriptLimits {
        max_operations: limits.max_rhai_operations,
        max_call_levels: limits.max_call_levels,
        max_string_size: limits.max_string_size,
        max_array_size: limits.max_array_size,
        max_map_size: limits.max_map_size,
    }
}

async fn await_worker<T: Send + 'static>(
    mut worker: tokio::task::JoinHandle<Result<T, OperationScriptError>>,
    root_cancel: CancellationToken,
    execution_cancel: CancellationToken,
    deadline: DateTime<Utc>,
    grace: Duration,
    outcome_unknown: bool,
) -> Result<T, OperationScriptError> {
    let until_deadline = (deadline - Utc::now()).to_std().unwrap_or_default();
    let trigger = tokio::select! {
        result = &mut worker => return join_worker(result),
        _ = root_cancel.cancelled() => "cancelled",
        _ = tokio::time::sleep(until_deadline) => "deadline",
    };
    execution_cancel.cancel();
    let grace = grace
        .to_std()
        .map_err(|_| invalid("cancellation_grace", "duration 无效"))?;
    match tokio::time::timeout(grace, &mut worker).await {
        Ok(result) => match join_worker(result) {
            Err(OperationScriptError::Runtime { .. }) if trigger == "cancelled" => {
                Err(OperationScriptError::Cancelled)
            }
            Err(OperationScriptError::Runtime { .. }) if trigger == "deadline" => {
                Err(OperationScriptError::DeadlineExceeded)
            }
            other => other,
        },
        Err(_) => Err(OperationScriptError::ExecutionInterrupted {
            reason: trigger,
            outcome_unknown,
        }),
    }
}

fn join_worker<T>(
    result: Result<Result<T, OperationScriptError>, tokio::task::JoinError>,
) -> Result<T, OperationScriptError> {
    result.map_err(|_| OperationScriptError::Internal {
        code: "blocking_worker_join",
    })?
}

fn sign_token(
    secret: &[u8],
    token: &OperationScriptPreflightToken,
) -> Result<String, OperationScriptError> {
    let payload = format!(
        "{}|{}|{}|{}",
        token.plan_id,
        token.binding_digest,
        token.issued_at.timestamp_millis(),
        token.expires_at.timestamp_millis()
    );
    Ok(hex(&hmac_sha256(secret, payload.as_bytes())))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut normalized = [0_u8; BLOCK];
    if key.len() > BLOCK {
        normalized[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; BLOCK];
    let mut outer_pad = [0x5c_u8; BLOCK];
    for index in 0..BLOCK {
        inner_pad[index] ^= normalized[index];
        outer_pad[index] ^= normalized[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner);
    outer.finalize().into()
}

fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
    let maximum = expected.len().max(actual.len());
    let mut difference = expected.len() ^ actual.len();
    for index in 0..maximum {
        difference |=
            usize::from(*expected.get(index).unwrap_or(&0) ^ *actual.get(index).unwrap_or(&0));
    }
    difference == 0
}

fn digest_json(
    value: &impl Serialize,
    field: &'static str,
) -> Result<String, OperationScriptError> {
    let bytes = serde_json::to_vec(value).map_err(|_| invalid(field, "无法序列化"))?;
    Ok(sha256(&bytes))
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn valid_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.chars().all(|character| character.is_ascii_hexdigit())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn invalid(field: &'static str, reason: &str) -> OperationScriptError {
    OperationScriptError::InvalidRequest {
        field,
        reason: reason.to_string(),
    }
}

fn bounded_diagnostic(message: &str) -> String {
    const MAX: usize = 2_048;
    message.chars().take(MAX).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_application_ports::operation_script::{
        OperationOriginRef, OperationPrincipalRef, OperationScopeRef,
        OperationScriptExecutionContext, OperationScriptOperationCall,
        OperationScriptOperationResult,
    };

    struct RejectingExecutor;
    #[async_trait]
    impl OperationScriptOperationExecutor for RejectingExecutor {
        async fn execute(
            &self,
            _call: OperationScriptOperationCall,
            _cancel: CancellationToken,
        ) -> Result<OperationScriptOperationResult, OperationScriptError> {
            Err(OperationScriptError::Internal {
                code: "unexpected_nested_call",
            })
        }
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
                project_id: Uuid::new_v4(),
            },
            authority_revision: "authority:1".into(),
            origin: OperationOriginRef::UserWorkshop,
            trace_id: "trace-1".into(),
            attachment_ref: None,
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
    async fn preflight(
        engine: &RhaiOperationScriptEngine,
        program: OperationScriptProgram,
        context: OperationScriptExecutionContext,
    ) -> OperationScriptPreflightResult {
        engine
            .preflight(
                OperationScriptPreflightRequest { program, context },
                CancellationToken::new(),
            )
            .await
            .expect("preflight")
    }

    #[tokio::test]
    async fn preflight_and_run_plain_rhai_on_blocking_worker() {
        let engine = engine(7);
        let context = context();
        let program = program("input.values.filter(|value| value > 1)");
        let plan = preflight(&engine, program.clone(), context.clone()).await;
        let outcome = engine
            .run(
                OperationScriptRunRequest {
                    program,
                    context,
                    token: plan.token,
                },
                Arc::new(RejectingExecutor),
                CancellationToken::new(),
            )
            .await
            .expect("run");
        assert_eq!(outcome.value, serde_json::json!([2, 3]));
        assert!(outcome.calls.is_empty());
    }

    #[tokio::test]
    async fn secret_rotation_invalidates_ephemeral_plan() {
        let first = engine(7);
        let second = engine(8);
        let context = context();
        let program = program("input");
        let plan = preflight(&first, program.clone(), context.clone()).await;
        let error = second
            .run(
                OperationScriptRunRequest {
                    program,
                    context,
                    token: plan.token,
                },
                Arc::new(RejectingExecutor),
                CancellationToken::new(),
            )
            .await
            .expect_err("rotated secret must reject");
        assert!(matches!(
            error,
            OperationScriptError::InvalidPlan {
                reason: "signature_mismatch"
            }
        ));
    }

    #[tokio::test]
    async fn token_binds_input_and_source() {
        let engine = engine(7);
        let context = context();
        let program = program("input");
        let plan = preflight(&engine, program.clone(), context.clone()).await;
        let mut changed = program;
        changed.input = serde_json::json!({"different":true});
        let error = engine
            .run(
                OperationScriptRunRequest {
                    program: changed,
                    context,
                    token: plan.token,
                },
                Arc::new(RejectingExecutor),
                CancellationToken::new(),
            )
            .await
            .expect_err("binding mismatch");
        assert!(matches!(
            error,
            OperationScriptError::InvalidPlan {
                reason: "binding_digest_mismatch"
            }
        ));
    }

    #[tokio::test]
    async fn pure_rhai_loop_observes_cancellation() {
        let engine = engine(7);
        let context = context();
        let mut program = program("loop { }");
        program.limits.max_rhai_operations = OperationScriptLimits::default().max_rhai_operations;
        let plan = preflight(&engine, program.clone(), context.clone()).await;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let error = engine
            .run(
                OperationScriptRunRequest {
                    program,
                    context,
                    token: plan.token,
                },
                Arc::new(RejectingExecutor),
                cancel,
            )
            .await
            .expect_err("cancelled");
        assert!(matches!(
            error,
            OperationScriptError::Cancelled | OperationScriptError::ExecutionInterrupted { .. }
        ));
    }

    #[tokio::test]
    async fn output_limit_is_enforced_after_json_bridge() {
        let engine = engine(7);
        let context = context();
        let mut program = program(r#""too-large""#);
        program.limits.max_output_bytes = 4;
        let plan = preflight(&engine, program.clone(), context.clone()).await;
        let error = engine
            .run(
                OperationScriptRunRequest {
                    program,
                    context,
                    token: plan.token,
                },
                Arc::new(RejectingExecutor),
                CancellationToken::new(),
            )
            .await
            .expect_err("output limit");
        assert!(matches!(
            error,
            OperationScriptError::OutputLimitExceeded { .. }
        ));
    }

    #[tokio::test]
    async fn recursive_operation_script_manifest_is_rejected() {
        let engine = engine(7);
        let mut program = program("input");
        program.allowed_operations.push(agentdash_application_ports::operation_script::OperationScriptAllowedOperation {
            operation_ref: agentdash_domain::operation::OperationRef::new("agentdash", "operation_script", "run", 1).expect("operation ref"),
            descriptor_digest: sha256(b"descriptor"),
            effect: agentdash_application_ports::operation_script::OperationEffect::ExternalSideEffect,
            replay_policy: agentdash_application_ports::operation_script::OperationReplayPolicy::NonReplayable,
            recursive_operation_script: true,
        });
        let error = engine
            .preflight(
                OperationScriptPreflightRequest {
                    program,
                    context: context(),
                },
                CancellationToken::new(),
            )
            .await
            .expect_err("recursive manifest");
        assert!(matches!(
            error,
            OperationScriptError::InvalidRequest {
                field: "allowed_operations",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn concurrent_script_admission_is_bounded() {
        let mut config = RhaiOperationScriptConfig::default();
        config.max_concurrent_scripts = 1;
        config.maximum_limits.max_rhai_operations = u64::MAX;
        let engine = Arc::new(RhaiOperationScriptEngine::new(&[7; 32], config).expect("engine"));
        let context = context();
        let mut program = program("loop { }");
        program.limits.max_rhai_operations = u64::MAX;
        let first_plan = preflight(&engine, program.clone(), context.clone()).await;
        let second_plan = preflight(&engine, program.clone(), context.clone()).await;
        let first_cancel = CancellationToken::new();
        let first_task = {
            let engine = engine.clone();
            let program = program.clone();
            let context = context.clone();
            let cancel = first_cancel.clone();
            tokio::spawn(async move {
                engine
                    .run(
                        OperationScriptRunRequest {
                            program,
                            context,
                            token: first_plan.token,
                        },
                        Arc::new(RejectingExecutor),
                        cancel,
                    )
                    .await
            })
        };
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let second = engine
            .run(
                OperationScriptRunRequest {
                    program,
                    context,
                    token: second_plan.token,
                },
                Arc::new(RejectingExecutor),
                CancellationToken::new(),
            )
            .await;
        assert!(matches!(
            second,
            Err(OperationScriptError::CapacityExceeded)
        ));
        first_cancel.cancel();
        let _ = first_task.await.expect("first task join");
    }

    #[test]
    fn mac_comparison_checks_equal_and_mismatched_lengths() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"diff"));
        assert!(!constant_time_eq(b"same", b"same-longer"));
    }
}
