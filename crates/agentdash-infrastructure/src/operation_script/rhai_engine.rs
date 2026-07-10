use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};

use agentdash_application_ports::operation_script::{
    OPERATION_SCRIPT_HOST_API_V1, OperationScriptAllowedOperation, OperationScriptCallEvidence,
    OperationScriptCallStatus, OperationScriptEngine, OperationScriptError,
    OperationScriptExecutionContext, OperationScriptLimits, OperationScriptOperationCall,
    OperationScriptOperationExecutor, OperationScriptPreflightRequest,
    OperationScriptPreflightResult, OperationScriptPreflightToken, OperationScriptProgram,
    OperationScriptResultAccess, OperationScriptResultRef, OperationScriptResultStore,
    OperationScriptResultValue, OperationScriptRunOutcome, OperationScriptRunRequest,
    RHAI_V1_DIALECT,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use futures::{StreamExt, stream};
use rhai::{AST, Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map, Scope};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::runtime::Handle;
use tokio::sync::{OwnedSemaphorePermit, RwLock as AsyncRwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::script_runtime::{RhaiScriptLimits, RhaiScriptRuntime};

#[derive(Debug, Clone)]
pub struct RhaiOperationScriptConfig {
    pub max_concurrent_scripts: usize,
    pub preflight_ttl: Duration,
    pub cancellation_grace: Duration,
    pub result_ttl: Duration,
    pub max_inline_result_bytes: usize,
    pub max_ast_cache_entries: usize,
    pub max_ast_cache_source_bytes: usize,
    pub maximum_limits: OperationScriptLimits,
}

impl Default for RhaiOperationScriptConfig {
    fn default() -> Self {
        Self {
            max_concurrent_scripts: 4,
            preflight_ttl: Duration::minutes(5),
            cancellation_grace: Duration::seconds(2),
            result_ttl: Duration::minutes(10),
            max_inline_result_bytes: 64 * 1024,
            max_ast_cache_entries: 128,
            max_ast_cache_source_bytes: 8 * 1024 * 1024,
            maximum_limits: OperationScriptLimits::default(),
        }
    }
}

struct CachedAst {
    ast: AST,
    source_bytes: usize,
    last_used: u64,
}

#[derive(Default)]
struct AstCache {
    entries: HashMap<String, CachedAst>,
    source_bytes: usize,
    clock: u64,
}

impl AstCache {
    fn get(&mut self, digest: &str) -> Option<AST> {
        self.clock = self.clock.wrapping_add(1);
        let entry = self.entries.get_mut(digest)?;
        entry.last_used = self.clock;
        Some(entry.ast.clone())
    }

    fn insert(
        &mut self,
        digest: String,
        ast: AST,
        source_bytes: usize,
        config: &RhaiOperationScriptConfig,
    ) {
        if source_bytes > config.max_ast_cache_source_bytes {
            return;
        }
        if let Some(previous) = self.entries.remove(&digest) {
            self.source_bytes -= previous.source_bytes;
        }
        self.clock = self.clock.wrapping_add(1);
        self.source_bytes += source_bytes;
        self.entries.insert(
            digest,
            CachedAst {
                ast,
                source_bytes,
                last_used: self.clock,
            },
        );
        while self.entries.len() > config.max_ast_cache_entries
            || self.source_bytes > config.max_ast_cache_source_bytes
        {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(digest, _)| digest.clone())
            else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.source_bytes -= removed.source_bytes;
            }
        }
    }
}

struct StoredResult {
    value: Value,
    access: OperationScriptResultAccess,
}

#[derive(Default)]
pub struct InMemoryOperationScriptResultStore {
    results: AsyncRwLock<HashMap<Uuid, StoredResult>>,
}

#[async_trait]
impl OperationScriptResultStore for InMemoryOperationScriptResultStore {
    async fn put(
        &self,
        value: Value,
        access: OperationScriptResultAccess,
    ) -> Result<OperationScriptResultRef, OperationScriptError> {
        let result_ref = OperationScriptResultRef {
            result_id: Uuid::new_v4(),
        };
        let mut results = self.results.write().await;
        results.retain(|_, item| item.access.expires_at > Utc::now());
        results.insert(result_ref.result_id, StoredResult { value, access });
        Ok(result_ref)
    }

    async fn resolve(
        &self,
        result_ref: &OperationScriptResultRef,
        current: &OperationScriptExecutionContext,
        cancel: CancellationToken,
    ) -> Result<Option<Value>, OperationScriptError> {
        if cancel.is_cancelled() {
            return Err(OperationScriptError::Cancelled);
        }
        let results = self.results.read().await;
        Ok(results.get(&result_ref.result_id).and_then(|item| {
            (item.access.expires_at > Utc::now()
                && item.access.principal == current.principal
                && item.access.scope == current.scope
                && item
                    .access
                    .required_capabilities
                    .is_subset(&current.granted_capabilities))
            .then(|| item.value.clone())
        }))
    }
}

pub struct RhaiOperationScriptEngine {
    config: RhaiOperationScriptConfig,
    signing_secret: Arc<[u8]>,
    permits: Arc<Semaphore>,
    ast_cache: Arc<RwLock<AstCache>>,
    result_store: Arc<dyn OperationScriptResultStore>,
}

impl RhaiOperationScriptEngine {
    pub fn new(
        signing_secret: &[u8],
        config: RhaiOperationScriptConfig,
    ) -> Result<Self, OperationScriptError> {
        Self::with_result_store(
            signing_secret,
            config,
            Arc::new(InMemoryOperationScriptResultStore::default()),
        )
    }

    pub fn with_result_store(
        signing_secret: &[u8],
        config: RhaiOperationScriptConfig,
        result_store: Arc<dyn OperationScriptResultStore>,
    ) -> Result<Self, OperationScriptError> {
        if signing_secret.len() < 32 {
            return Err(invalid("signing_secret", "至少需要 32 bytes"));
        }
        if config.max_concurrent_scripts == 0
            || config.preflight_ttl <= Duration::zero()
            || config.cancellation_grace <= Duration::zero()
            || config.result_ttl <= Duration::zero()
            || config.max_inline_result_bytes == 0
            || config.max_ast_cache_entries == 0
            || config.max_ast_cache_source_bytes == 0
        {
            return Err(invalid(
                "engine_config",
                "capacity、cache 与 duration 必须大于 0",
            ));
        }
        Ok(Self {
            permits: Arc::new(Semaphore::new(config.max_concurrent_scripts)),
            signing_secret: Arc::from(signing_secret),
            ast_cache: Arc::new(RwLock::new(AstCache::default())),
            result_store,
            config,
        })
    }

    pub async fn resolve_result(
        &self,
        result_ref: &OperationScriptResultRef,
        current_context: &OperationScriptExecutionContext,
        cancel: CancellationToken,
    ) -> Result<Option<Value>, OperationScriptError> {
        validate_context(current_context)?;
        self.result_store
            .resolve(result_ref, current_context, cancel)
            .await
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

#[derive(Clone)]
struct RhaiOperationHost {
    core: Arc<HostCore>,
}

struct HostCore {
    runtime: Handle,
    executor: Arc<dyn OperationScriptOperationExecutor>,
    manifest: HashMap<String, OperationScriptAllowedOperation>,
    context: OperationScriptExecutionContext,
    execution_id: Uuid,
    deadline: DateTime<Utc>,
    cancel: CancellationToken,
    max_calls: usize,
    max_parallel: usize,
    next_call: Mutex<usize>,
    evidence: Arc<Mutex<Vec<OperationScriptCallEvidence>>>,
}

impl RhaiOperationHost {
    fn invoke(
        &mut self,
        operation: ImmutableString,
        input: Dynamic,
    ) -> Result<Dynamic, Box<EvalAltResult>> {
        let input: Value = rhai::serde::from_dynamic(&input).map_err(rhai_error)?;
        let value = self
            .core
            .runtime
            .block_on(self.core.invoke_one(operation.as_str().to_owned(), input))
            .map_err(|error| rhai_error(error.to_string()))?;
        Ok(crate::script_runtime::json_to_dynamic(&value))
    }

    fn invoke_all(&mut self, requests: Array) -> Result<Array, Box<EvalAltResult>> {
        let mut parsed = Vec::with_capacity(requests.len());
        for request in requests {
            let map = request
                .try_cast::<Map>()
                .ok_or_else(|| rhai_error("invoke_all item must be a map"))?;
            let operation = map
                .get("operation")
                .and_then(|value| value.clone().try_cast::<ImmutableString>())
                .ok_or_else(|| rhai_error("invoke_all item.operation must be a string"))?;
            let input = map
                .get("input")
                .ok_or_else(|| rhai_error("invoke_all item.input is required"))?;
            let input: Value = rhai::serde::from_dynamic(input).map_err(rhai_error)?;
            parsed.push((operation.to_string(), input));
        }
        let core = self.core.clone();
        let values = self.core.runtime.block_on(async move {
            stream::iter(
                parsed
                    .into_iter()
                    .enumerate()
                    .map(|(index, (operation, input))| {
                        let core = core.clone();
                        async move { (index, core.invoke_one(operation, input).await) }
                    }),
            )
            .buffer_unordered(core.max_parallel)
            .collect::<Vec<_>>()
            .await
        });
        let mut ordered = vec![Value::Null; values.len()];
        for (index, result) in values {
            ordered[index] = result.map_err(|error| rhai_error(error.to_string()))?;
        }
        Ok(ordered
            .iter()
            .map(crate::script_runtime::json_to_dynamic)
            .collect())
    }
}

impl HostCore {
    async fn invoke_one(
        &self,
        script_key: String,
        input: Value,
    ) -> Result<Value, OperationScriptError> {
        let allowed = self.manifest.get(&script_key).ok_or_else(|| {
            OperationScriptError::OperationDenied {
                operation_key: script_key.clone(),
            }
        })?;
        let call_index = {
            let mut next = self
                .next_call
                .lock()
                .map_err(|_| OperationScriptError::Internal {
                    code: "call_counter_poisoned",
                })?;
            if *next >= self.max_calls {
                return Err(OperationScriptError::CallLimitExceeded {
                    maximum: self.max_calls,
                });
            }
            let index = *next;
            *next += 1;
            index
        };
        let child_trace_id = format!("{}:ops:{}", self.context.trace_id, call_index);
        let call = OperationScriptOperationCall {
            execution_id: self.execution_id,
            call_index,
            operation_ref: allowed.operation_ref.clone(),
            input,
            context: self.context.clone(),
            parent_trace_id: self.context.trace_id.clone(),
            child_trace_id: child_trace_id.clone(),
            deadline: self.deadline,
        };
        let result = self.executor.execute(call, self.cancel.child_token()).await;
        let (status, code, unknown) = match &result {
            Ok(result) if result.outcome_unknown => {
                (OperationScriptCallStatus::OutcomeUnknown, None, true)
            }
            Ok(_) => (OperationScriptCallStatus::Succeeded, None, false),
            Err(error) => {
                let unknown = error_outcome_unknown(error);
                (
                    if unknown {
                        OperationScriptCallStatus::OutcomeUnknown
                    } else {
                        OperationScriptCallStatus::Failed
                    },
                    Some(error_code(error)),
                    unknown,
                )
            }
        };
        self.evidence
            .lock()
            .map_err(|_| OperationScriptError::Internal {
                code: "evidence_poisoned",
            })?
            .push(OperationScriptCallEvidence {
                call_index,
                operation_ref: allowed.operation_ref.clone(),
                child_trace_id,
                status,
                error_code: code,
            });
        match result {
            Ok(_result) if unknown => Err(OperationScriptError::NestedOperation {
                code: "outcome_unknown".into(),
                outcome_unknown: true,
            }),
            Ok(result) => Ok(result.value),
            Err(error) => Err(error),
        }
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
        let source_bytes = source.len();
        let source_digest = digests.source_digest.clone();
        let limits = request.program.limits;
        let cache = self.ast_cache.clone();
        let config = self.config.clone();
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
                .insert(source_digest, ast, source_bytes, &config);
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
        executor: Arc<dyn OperationScriptOperationExecutor>,
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
        let execution_id = Uuid::new_v4();
        let execution_cancel = cancel.child_token();
        let evidence = Arc::new(Mutex::new(Vec::new()));
        let host = RhaiOperationHost {
            core: Arc::new(HostCore {
                runtime: Handle::current(),
                executor,
                manifest: request
                    .program
                    .allowed_operations
                    .iter()
                    .cloned()
                    .map(|item| (item.script_key(), item))
                    .collect(),
                context: request.context.clone(),
                execution_id,
                deadline,
                cancel: execution_cancel.clone(),
                max_calls: request.program.limits.max_operation_calls,
                max_parallel: request.program.limits.max_parallel_operations,
                next_call: Mutex::new(0),
                evidence: evidence.clone(),
            }),
        };
        let source = request.program.source.clone();
        let input = request.program.input.clone();
        let limits = request.program.limits;
        let source_digest = digests.source_digest;
        let cache = self.ast_cache.clone();
        let worker_cancel = execution_cancel.clone();
        let worker = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut engine = Engine::new();
            RhaiScriptRuntime::apply_limits(&mut engine, rhai_limits(limits));
            let progress_cancel = worker_cancel.clone();
            engine.on_progress(move |_| {
                (progress_cancel.is_cancelled() || Utc::now() >= deadline)
                    .then(|| Dynamic::from("operation_script_interrupted"))
            });
            engine.register_type_with_name::<RhaiOperationHost>("OperationHost");
            engine.register_fn("invoke", RhaiOperationHost::invoke);
            engine.register_fn("invoke_all", RhaiOperationHost::invoke_all);
            let ast = cache
                .write()
                .ok()
                .and_then(|mut cache| cache.get(&source_digest))
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
            scope.push("ops", host);
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
        let value = match await_worker(
            worker,
            cancel,
            execution_cancel,
            deadline,
            self.config.cancellation_grace,
            true,
        )
        .await
        {
            Ok(value) => value,
            Err(error) => return Err(execution_failure(error, evidence_snapshot(&evidence))),
        };
        let output_bytes = serde_json::to_vec(&value)
            .map_err(|_| OperationScriptError::Internal {
                code: "output_serialize",
            })?
            .len();
        if output_bytes > request.program.limits.max_output_bytes {
            return Err(execution_failure(
                OperationScriptError::OutputLimitExceeded {
                    actual: output_bytes,
                    maximum: request.program.limits.max_output_bytes,
                },
                evidence_snapshot(&evidence),
            ));
        }
        let expires_at = std::cmp::min(
            request.token.expires_at,
            Utc::now() + self.config.result_ttl,
        );
        let result_access = OperationScriptResultAccess {
            principal: request.context.principal,
            scope: request.context.scope,
            authority_revision: request.context.authority_revision,
            required_capabilities: request.context.granted_capabilities,
            expires_at,
        };
        let result = if output_bytes <= self.config.max_inline_result_bytes {
            OperationScriptResultValue::Inline { value }
        } else {
            OperationScriptResultValue::Ref {
                result_ref: match self.result_store.put(value, result_access.clone()).await {
                    Ok(result_ref) => result_ref,
                    Err(error) => {
                        return Err(execution_failure(error, evidence_snapshot(&evidence)));
                    }
                },
            }
        };
        let calls = evidence_snapshot(&evidence);
        Ok(OperationScriptRunOutcome {
            execution_id,
            plan_id: request.token.plan_id,
            value: result,
            partial: calls
                .iter()
                .any(|call| call.status == OperationScriptCallStatus::Succeeded),
            outcome_unknown: calls
                .iter()
                .any(|call| call.status == OperationScriptCallStatus::OutcomeUnknown),
            calls,
            result_access,
        })
    }

    async fn resolve_result(
        &self,
        result_ref: &OperationScriptResultRef,
        current_context: &OperationScriptExecutionContext,
        cancel: CancellationToken,
    ) -> Result<Option<Value>, OperationScriptError> {
        RhaiOperationScriptEngine::resolve_result(self, result_ref, current_context, cancel).await
    }
}

fn evidence_snapshot(
    evidence: &Mutex<Vec<OperationScriptCallEvidence>>,
) -> Vec<OperationScriptCallEvidence> {
    let mut calls = evidence
        .lock()
        .map(|calls| calls.clone())
        .unwrap_or_default();
    calls.sort_by_key(|call| call.call_index);
    calls
}

fn execution_failure(
    error: OperationScriptError,
    calls: Vec<OperationScriptCallEvidence>,
) -> OperationScriptError {
    let partial = calls
        .iter()
        .any(|call| call.status == OperationScriptCallStatus::Succeeded);
    let outcome_unknown = error_outcome_unknown(&error)
        || calls
            .iter()
            .any(|call| call.status == OperationScriptCallStatus::OutcomeUnknown);
    OperationScriptError::ExecutionFailed {
        diagnostic: bounded_diagnostic(&error.to_string()),
        calls,
        partial,
        outcome_unknown,
    }
}

fn error_outcome_unknown(error: &OperationScriptError) -> bool {
    matches!(
        error,
        OperationScriptError::ExecutionInterrupted {
            outcome_unknown: true,
            ..
        } | OperationScriptError::NestedOperation {
            outcome_unknown: true,
            ..
        } | OperationScriptError::ExecutionFailed {
            outcome_unknown: true,
            ..
        }
    )
}

fn error_code(error: &OperationScriptError) -> String {
    match error {
        OperationScriptError::NestedOperation { code, .. } => code.clone(),
        OperationScriptError::Cancelled => "cancelled".into(),
        OperationScriptError::DeadlineExceeded => "deadline_exceeded".into(),
        OperationScriptError::OperationDenied { .. } => "operation_denied".into(),
        OperationScriptError::CallLimitExceeded { .. } => "call_limit_exceeded".into(),
        _ => "operation_failed".into(),
    }
}

fn rhai_error(message: impl ToString) -> Box<EvalAltResult> {
    message.to_string().into()
}

#[derive(Serialize)]
struct PlanBinding<'a> {
    dialect: &'a str,
    host_api_version: u16,
    source_digest: &'a str,
    input_digest: &'a str,
    manifest_digest: &'a str,
    limits: OperationScriptLimits,
    context: &'a OperationScriptExecutionContext,
}
struct PlanDigests {
    source_digest: String,
    manifest_digest: String,
    binding_digest: String,
}

fn plan_digests(
    program: &OperationScriptProgram,
    context: &OperationScriptExecutionContext,
) -> Result<PlanDigests, OperationScriptError> {
    let source_digest = sha256(program.source.as_bytes());
    let input_digest = digest_json(&program.input, "input")?;
    let mut manifest = program.allowed_operations.clone();
    manifest.sort_by_key(OperationScriptAllowedOperation::script_key);
    let manifest_digest = digest_json(&manifest, "allowed_operations")?;
    let binding_digest = digest_json(
        &PlanBinding {
            dialect: &program.dialect,
            host_api_version: program.host_api_version,
            source_digest: &source_digest,
            input_digest: &input_digest,
            manifest_digest: &manifest_digest,
            limits: program.limits,
            context,
        },
        "plan_binding",
    )?;
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
    if serde_json::to_vec(&program.input)
        .map_err(|_| OperationScriptError::Internal {
            code: "input_serialize",
        })?
        .len()
        > program.limits.max_input_bytes
    {
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

fn validate_context(context: &OperationScriptExecutionContext) -> Result<(), OperationScriptError> {
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
            .granted_capabilities
            .iter()
            .any(|capability| capability.trim().is_empty() || capability.trim() != capability)
        || context
            .attachment_ref
            .as_ref()
            .is_some_and(|value| value.trim().is_empty() || value.trim() != value)
    {
        return Err(invalid(
            "context",
            "authority、capabilities、trace 与 attachment ref 必须已规范化",
        ));
    }
    Ok(())
}

fn validate_limits(
    l: &OperationScriptLimits,
    m: &OperationScriptLimits,
) -> Result<(), OperationScriptError> {
    let valid = l.timeout_ms > 0
        && l.max_source_bytes > 0
        && l.max_input_bytes > 0
        && l.max_output_bytes > 0
        && l.max_rhai_operations > 0
        && l.max_call_levels > 0
        && l.max_string_size > 0
        && l.max_array_size > 0
        && l.max_map_size > 0
        && l.max_operation_calls > 0
        && l.max_parallel_operations > 0
        && l.timeout_ms <= m.timeout_ms
        && l.max_source_bytes <= m.max_source_bytes
        && l.max_input_bytes <= m.max_input_bytes
        && l.max_output_bytes <= m.max_output_bytes
        && l.max_rhai_operations <= m.max_rhai_operations
        && l.max_call_levels <= m.max_call_levels
        && l.max_string_size <= m.max_string_size
        && l.max_array_size <= m.max_array_size
        && l.max_map_size <= m.max_map_size
        && l.max_operation_calls <= m.max_operation_calls
        && l.max_parallel_operations <= m.max_parallel_operations;
    valid
        .then_some(())
        .ok_or_else(|| invalid("limits", "所有 limits 必须大于 0 且不超过 engine maximum"))
}

fn rhai_limits(l: OperationScriptLimits) -> RhaiScriptLimits {
    RhaiScriptLimits {
        max_operations: l.max_rhai_operations,
        max_call_levels: l.max_call_levels,
        max_string_size: l.max_string_size,
        max_array_size: l.max_array_size,
        max_map_size: l.max_map_size,
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
    let trigger = tokio::select! { result = &mut worker => return join_worker(result), _ = root_cancel.cancelled() => "cancelled", _ = tokio::time::sleep((deadline - Utc::now()).to_std().unwrap_or_default()) => "deadline" };
    execution_cancel.cancel();
    match tokio::time::timeout(
        grace
            .to_std()
            .map_err(|_| invalid("cancellation_grace", "duration 无效"))?,
        &mut worker,
    )
    .await
    {
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
    Ok(hex(&hmac_sha256(
        secret,
        format!(
            "{}|{}|{}|{}",
            token.plan_id,
            token.binding_digest,
            token.issued_at.timestamp_millis(),
            token.expires_at.timestamp_millis()
        )
        .as_bytes(),
    )))
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
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner.finalize());
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
    serde_json::to_vec(value)
        .map(|bytes| sha256(&bytes))
        .map_err(|_| invalid(field, "无法序列化"))
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
    message.chars().take(2_048).collect()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
