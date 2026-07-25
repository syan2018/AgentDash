//! Default [`FunctionRunner`] implementation.
//!
//! Owns the workflow function-activity IO: `tera` template rendering,
//! `reqwest` HTTP requests, and `tokio::process` command execution. Returns
//! raw outcomes; the application layer decides success / failure and builds
//! the corresponding activity event.

use agentdash_domain::workflow::{ApiRequestExecutorSpec, BashExecExecutorSpec};
use agentdash_platform_spi::{
    ApiRequestOutcome, BashExecOutcome, FunctionEffectObservation, FunctionEffectRawOutcome,
    FunctionEffectRequest, FunctionEffectSpec, FunctionRunner,
};
use agentdash_process::{
    ProcessDomain, background_tokio_command, background_tokio_command_with_cwd,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// Function runner backed by `reqwest` + `tokio::process` + `tera`.
#[derive(Debug, Clone)]
pub struct DefaultFunctionRunner {
    pool: PgPool,
    owner_id: String,
    lease_duration: std::time::Duration,
    #[cfg(test)]
    disable_heartbeat: bool,
    #[cfg(test)]
    fail_after_io: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl DefaultFunctionRunner {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            owner_id: format!("function-runner:{}", Uuid::new_v4()),
            lease_duration: std::time::Duration::from_secs(30),
            #[cfg(test)]
            disable_heartbeat: false,
            #[cfg(test)]
            fail_after_io: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    #[cfg(test)]
    fn new_with_fail_after_io(pool: PgPool) -> Self {
        let runner = Self::new(pool);
        runner
            .fail_after_io
            .store(true, std::sync::atomic::Ordering::SeqCst);
        runner
    }

    #[cfg(test)]
    fn with_test_lease(mut self, lease_duration: std::time::Duration) -> Self {
        self.lease_duration = lease_duration;
        self
    }

    #[cfg(test)]
    fn without_heartbeat(mut self) -> Self {
        self.disable_heartbeat = true;
        self
    }
}

#[async_trait]
impl FunctionRunner for DefaultFunctionRunner {
    async fn run_api_request(
        &self,
        spec: &ApiRequestExecutorSpec,
        context: &Value,
    ) -> Result<ApiRequestOutcome, String> {
        let method_text = render_template(&spec.method, context)?;
        let method = reqwest::Method::from_bytes(method_text.as_bytes())
            .map_err(|error| format!("API method 非法: {error}"))?;
        let url = render_template(&spec.url_template, context)?;

        let client = reqwest::Client::new();
        let mut request = client.request(method, url);
        if let Some(body_template) = &spec.body_template {
            let body = render_json_templates(body_template, context)?;
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string());
        }

        let response = request
            .send()
            .await
            .map_err(|error| format!("API request 失败: {error}"))?;
        let status = response.status();
        let body_text = response
            .text()
            .await
            .map_err(|error| format!("读取 API response 失败: {error}"))?;
        let body_json = serde_json::from_str::<Value>(&body_text).ok();

        Ok(ApiRequestOutcome {
            status: status.as_u16(),
            body_text,
            body_json,
        })
    }

    async fn run_bash(
        &self,
        spec: &BashExecExecutorSpec,
        context: &Value,
    ) -> Result<BashExecOutcome, String> {
        let command = render_template(&spec.command, context)?;
        let args = spec
            .args
            .iter()
            .map(|arg| render_template(arg, context))
            .collect::<Result<Vec<_>, _>>()?;

        let working_directory = spec
            .working_directory
            .as_ref()
            .map(|template| render_template(template, context))
            .transpose()?;
        let mut command_builder = match working_directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(cwd) => {
                background_tokio_command_with_cwd(ProcessDomain::FunctionRunner, &command, cwd)
            }
            None => background_tokio_command(ProcessDomain::FunctionRunner, &command),
        };
        command_builder.args(args);

        let output = command_builder
            .output()
            .await
            .map_err(|error| format!("Bash exec 启动失败: {error}"))?;
        Ok(BashExecOutcome {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
        })
    }

    async fn execute_effect(
        &self,
        request: FunctionEffectRequest,
    ) -> Result<FunctionEffectObservation, String> {
        let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;
        let row = sqlx::query(
            "SELECT payload_digest,request,runner_state,runner_evidence,runner_receipt
             FROM workflow_executor_effects
             WHERE effect_id=$1 AND effect_kind='function'
             FOR UPDATE",
        )
        .bind(&request.effect_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "durable Function effect was not prepared".to_owned())?;
        use sqlx::Row;
        let payload_digest: String = row.try_get("payload_digest").map_err(db_string)?;
        let prepared: Value = row.try_get("request").map_err(db_string)?;
        if payload_digest != request.payload_digest
            || prepared.get("payload_digest").and_then(Value::as_str)
                != Some(request.payload_digest.as_str())
            || prepared.get("spec") != Some(&function_spec_json(&request.spec)?)
            || prepared.get("context") != Some(&request.context)
        {
            return Err("durable Function effect payload conflict".to_owned());
        }
        let runner_state: String = row.try_get("runner_state").map_err(db_string)?;
        let runner_receipt = row
            .try_get::<Option<Value>, _>("runner_receipt")
            .map_err(db_string)?;
        match runner_state.as_str() {
            "not_applied" => {}
            "accepted" => {
                tx.commit().await.map_err(|error| error.to_string())?;
                return Ok(FunctionEffectObservation::Accepted);
            }
            "in_flight" => {
                tx.commit().await.map_err(|error| error.to_string())?;
                return Ok(FunctionEffectObservation::InFlight);
            }
            "succeeded" | "failed" => {
                tx.commit().await.map_err(|error| error.to_string())?;
                return observation_from_json(
                    runner_receipt
                        .ok_or_else(|| "terminal Function receipt is missing".to_owned())?,
                );
            }
            "lost" => {
                let evidence: Value = row.try_get("runner_evidence").map_err(db_string)?;
                tx.commit().await.map_err(|error| error.to_string())?;
                return Ok(lost_observation(evidence));
            }
            other => return Err(format!("unknown durable Function runner_state: {other}")),
        }

        let claim_id = Uuid::new_v4().to_string();
        let accepted_at = chrono::Utc::now();
        let lease_expires_at = accepted_at
            + chrono::Duration::from_std(self.lease_duration).map_err(|error| error.to_string())?;
        let accepted = observation_to_json(&FunctionEffectObservation::Accepted)?;
        let accepted_evidence = json!({
            "claim_id": claim_id,
            "claim_owner": self.owner_id,
            "accepted_at": accepted_at,
        });
        sqlx::query(
            "UPDATE workflow_executor_effects
             SET runner_state='accepted',runner_claim_id=$1,runner_claim_owner=$2,
                 runner_lease_expires_at=$3,runner_evidence=$4,
                 runner_receipt=$5,updated_at=NOW()
             WHERE effect_id=$6 AND effect_kind='function'
               AND runner_state='not_applied' AND runner_receipt IS NULL",
        )
        .bind(&claim_id)
        .bind(&self.owner_id)
        .bind(lease_expires_at)
        .bind(accepted_evidence)
        .bind(accepted)
        .bind(&request.effect_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        tx.commit().await.map_err(|error| error.to_string())?;

        let in_flight = sqlx::query(
            "UPDATE workflow_executor_effects
             SET runner_state='in_flight',runner_receipt=$1,updated_at=NOW()
             WHERE effect_id=$2 AND effect_kind='function'
               AND runner_state='accepted'
               AND runner_claim_id=$3 AND runner_claim_owner=$4",
        )
        .bind(observation_to_json(&FunctionEffectObservation::InFlight)?)
        .bind(&request.effect_id)
        .bind(&claim_id)
        .bind(&self.owner_id)
        .execute(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        if in_flight.rows_affected() != 1 {
            return Err("durable Function claim could not enter InFlight".to_owned());
        }

        let heartbeat_cancel = tokio_util::sync::CancellationToken::new();
        let heartbeat = {
            let pool = self.pool.clone();
            let effect_id = request.effect_id.clone();
            let claim_id = claim_id.clone();
            let owner_id = self.owner_id.clone();
            let lease_duration = self.lease_duration;
            let cancel = heartbeat_cancel.clone();
            #[cfg(test)]
            let disabled = self.disable_heartbeat;
            tokio::spawn(async move {
                #[cfg(test)]
                if disabled {
                    cancel.cancelled().await;
                    return;
                }
                let interval = (lease_duration / 3).max(std::time::Duration::from_millis(25));
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tokio::time::sleep(interval) => {
                            let extension = chrono::Duration::from_std(lease_duration)
                                .unwrap_or_else(|_| chrono::Duration::seconds(30));
                            let updated = sqlx::query(
                                "UPDATE workflow_executor_effects
                                 SET runner_lease_expires_at=$1,updated_at=NOW()
                                 WHERE effect_id=$2 AND effect_kind='function'
                                   AND runner_state IN ('accepted','in_flight')
                                   AND runner_claim_id=$3 AND runner_claim_owner=$4",
                            )
                            .bind(chrono::Utc::now() + extension)
                            .bind(&effect_id)
                            .bind(&claim_id)
                            .bind(&owner_id)
                            .execute(&pool)
                            .await;
                            if updated.is_err() || updated.is_ok_and(|result| result.rows_affected() == 0) {
                                return;
                            }
                        }
                    }
                }
            })
        };

        // The durable Accepted claim is visible before external IO. A process
        // restart therefore inspects Accepted and never automatically reissues
        // an ambiguous raw side effect.
        let observation = match &request.spec {
            FunctionEffectSpec::ApiRequest(spec) => self
                .run_api_request(spec, &request.context)
                .await
                .map(|outcome| {
                    FunctionEffectObservation::Succeeded(FunctionEffectRawOutcome::ApiRequest(
                        outcome,
                    ))
                }),
            FunctionEffectSpec::BashExec(spec) => {
                self.run_bash(spec, &request.context).await.map(|outcome| {
                    FunctionEffectObservation::Succeeded(FunctionEffectRawOutcome::BashExec(
                        outcome,
                    ))
                })
            }
        }
        .unwrap_or_else(|message| FunctionEffectObservation::Failed {
            message,
            retryable: true,
        });
        heartbeat_cancel.cancel();
        let _ = heartbeat.await;
        #[cfg(test)]
        if self
            .fail_after_io
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err("fault injected after Function IO before terminal receipt".to_owned());
        }
        let receipt = observation_to_json(&observation)?;
        let terminal_state = match &observation {
            FunctionEffectObservation::Succeeded(_) => "succeeded",
            FunctionEffectObservation::Failed { .. } => "failed",
            _ => return Err("Function execution produced a non-terminal observation".to_owned()),
        };
        let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;
        let terminal_update = sqlx::query(
            "UPDATE workflow_executor_effects
             SET runner_state=$1,runner_receipt=$2,
                 runner_evidence=$3,updated_at=NOW()
             WHERE effect_id=$4 AND effect_kind='function'
               AND runner_state='in_flight' AND runner_claim_id=$5
               AND runner_claim_owner=$6",
        )
        .bind(terminal_state)
        .bind(receipt)
        .bind(json!({
            "claim_id": claim_id,
            "terminal_at": chrono::Utc::now(),
        }))
        .bind(&request.effect_id)
        .bind(&claim_id)
        .bind(&self.owner_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        if terminal_update.rows_affected() != 1 {
            sqlx::query(
                "UPDATE workflow_executor_effects
                 SET runner_late_evidence=$1,updated_at=NOW()
                 WHERE effect_id=$2 AND effect_kind='function'",
            )
            .bind(json!({
                "claim_id": claim_id,
                "claim_owner": self.owner_id,
                "observed_at": chrono::Utc::now(),
                "terminal_observation": observation_to_json(&observation)?,
            }))
            .bind(&request.effect_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
            tx.commit().await.map_err(|error| error.to_string())?;
            return Err("late Function terminal receipt fenced after claim loss".to_owned());
        }
        tx.commit().await.map_err(|error| error.to_string())?;
        Ok(observation)
    }

    async fn inspect_effect(&self, effect_id: &str) -> Result<FunctionEffectObservation, String> {
        let row = sqlx::query(
            "SELECT runner_state,runner_claim_id,runner_claim_owner,
                    runner_lease_expires_at,runner_evidence,runner_receipt
             FROM workflow_executor_effects
             WHERE effect_id=$1 AND effect_kind='function'",
        )
        .bind(effect_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        let Some(row) = row else {
            return Ok(FunctionEffectObservation::NotApplied);
        };
        use sqlx::Row;
        let state: String = row.try_get("runner_state").map_err(db_string)?;
        let receipt: Option<Value> = row.try_get("runner_receipt").map_err(db_string)?;
        match state.as_str() {
            "not_applied" => Ok(FunctionEffectObservation::NotApplied),
            "succeeded" | "failed" => observation_from_json(
                receipt.ok_or_else(|| "terminal Function receipt is missing".to_owned())?,
            ),
            "accepted" | "in_flight" => {
                let claim_owner: String = row.try_get("runner_claim_owner").map_err(db_string)?;
                let lease_expires_at: chrono::DateTime<chrono::Utc> =
                    row.try_get("runner_lease_expires_at").map_err(db_string)?;
                if lease_expires_at > chrono::Utc::now() {
                    return Ok(if state == "accepted" {
                        FunctionEffectObservation::Accepted
                    } else {
                        FunctionEffectObservation::InFlight
                    });
                }
                let claim_id: String = row.try_get("runner_claim_id").map_err(db_string)?;
                let reason = "function_runner_claim_lease_expired";
                let lost_evidence = json!({
                    "claim_id": claim_id,
                    "claim_owner": claim_owner,
                    "lease_expires_at": lease_expires_at,
                    "lost_at": chrono::Utc::now(),
                    "reason": reason,
                });
                let lost = sqlx::query(
                    "UPDATE workflow_executor_effects
                     SET runner_state='lost',runner_receipt=NULL,
                         runner_evidence=$1,updated_at=NOW()
                     WHERE effect_id=$2 AND effect_kind='function'
                       AND runner_state IN ('accepted','in_flight') AND runner_claim_id=$3
                       AND runner_lease_expires_at <= NOW()
                     RETURNING effect_id",
                )
                .bind(&lost_evidence)
                .bind(effect_id)
                .bind(&claim_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| error.to_string())?;
                if lost.is_none() {
                    return self.inspect_effect(effect_id).await;
                }
                Ok(FunctionEffectObservation::Lost {
                    reason: reason.to_owned(),
                    evidence: lost_evidence,
                })
            }
            "lost" => {
                let evidence: Value = row.try_get("runner_evidence").map_err(db_string)?;
                Ok(lost_observation(evidence))
            }
            other => Err(format!("unknown durable Function runner_state: {other}")),
        }
    }
}

fn function_spec_json(spec: &FunctionEffectSpec) -> Result<Value, String> {
    match spec {
        FunctionEffectSpec::ApiRequest(value) => serde_json::to_value(value)
            .map(|value| tagged_spec("api_request", value))
            .map_err(|error| error.to_string()),
        FunctionEffectSpec::BashExec(value) => serde_json::to_value(value)
            .map(|value| tagged_spec("bash_exec", value))
            .map_err(|error| error.to_string()),
    }
}

fn tagged_spec(kind: &str, mut value: Value) -> Value {
    if let Value::Object(ref mut object) = value {
        object.insert("type".to_owned(), Value::String(kind.to_owned()));
    }
    value
}

fn observation_to_json(observation: &FunctionEffectObservation) -> Result<Value, String> {
    match observation {
        FunctionEffectObservation::Succeeded(FunctionEffectRawOutcome::ApiRequest(outcome)) => {
            Ok(json!({
                "kind": "api_request",
                "status": outcome.status,
                "body_text": outcome.body_text,
                "body_json": outcome.body_json,
            }))
        }
        FunctionEffectObservation::Succeeded(FunctionEffectRawOutcome::BashExec(outcome)) => {
            Ok(json!({
                "kind": "bash_exec",
                "exit_code": outcome.exit_code,
                "stdout": outcome.stdout,
                "stderr": outcome.stderr,
                "success": outcome.success,
            }))
        }
        FunctionEffectObservation::Failed { message, retryable } => Ok(json!({
            "kind": "failed",
            "message": message,
            "retryable": retryable,
        })),
        FunctionEffectObservation::Accepted => Ok(json!({"kind": "accepted"})),
        FunctionEffectObservation::InFlight => Ok(json!({"kind": "in_flight"})),
        FunctionEffectObservation::Lost { reason, evidence } => Ok(json!({
            "kind": "lost",
            "reason": reason,
            "evidence": evidence,
        })),
        FunctionEffectObservation::NotApplied => {
            Err("NotApplied Function observation cannot be persisted".to_owned())
        }
    }
}

fn observation_from_json(value: Value) -> Result<FunctionEffectObservation, String> {
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "durable Function receipt kind is missing".to_owned())?;
    match kind {
        "api_request" => Ok(FunctionEffectObservation::Succeeded(
            FunctionEffectRawOutcome::ApiRequest(ApiRequestOutcome {
                status: u16::try_from(
                    value
                        .get("status")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| "Function API receipt status is missing".to_owned())?,
                )
                .map_err(|error| error.to_string())?,
                body_text: json_string(&value, "body_text")?,
                body_json: value
                    .get("body_json")
                    .cloned()
                    .filter(|value| !value.is_null()),
            }),
        )),
        "bash_exec" => Ok(FunctionEffectObservation::Succeeded(
            FunctionEffectRawOutcome::BashExec(BashExecOutcome {
                exit_code: value
                    .get("exit_code")
                    .and_then(Value::as_i64)
                    .map(i32::try_from)
                    .transpose()
                    .map_err(|error| error.to_string())?,
                stdout: json_string(&value, "stdout")?,
                stderr: json_string(&value, "stderr")?,
                success: value
                    .get("success")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| "Function Bash receipt success is missing".to_owned())?,
            }),
        )),
        "failed" => Ok(FunctionEffectObservation::Failed {
            message: json_string(&value, "message")?,
            retryable: value
                .get("retryable")
                .and_then(Value::as_bool)
                .ok_or_else(|| "Function failure receipt retryable is missing".to_owned())?,
        }),
        "accepted" => Ok(FunctionEffectObservation::Accepted),
        "in_flight" => Ok(FunctionEffectObservation::InFlight),
        "lost" => Ok(FunctionEffectObservation::Lost {
            reason: json_string(&value, "reason")?,
            evidence: value.get("evidence").cloned().unwrap_or(Value::Null),
        }),
        _ => Err(format!("unknown durable Function receipt kind: {kind}")),
    }
}

fn lost_observation(evidence: Value) -> FunctionEffectObservation {
    let reason = evidence
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("durable Function outcome cannot be reconciled")
        .to_owned();
    FunctionEffectObservation::Lost { reason, evidence }
}

fn json_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("Function receipt {field} is missing"))
}

fn db_string(error: sqlx::Error) -> String {
    error.to_string()
}

fn render_template(template: &str, context: &Value) -> Result<String, String> {
    let context = tera::Context::from_serialize(context)
        .map_err(|error| format!("Function template context 非法: {error}"))?;
    tera::Tera::one_off(template, &context, false)
        .map_err(|error| format!("Function template 渲染失败: {error}"))
}

fn render_json_templates(value: &Value, context: &Value) -> Result<Value, String> {
    match value {
        Value::String(template) => render_template(template, context).map(Value::String),
        Value::Array(values) => values
            .iter()
            .map(|value| render_json_templates(value, context))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| Ok((key.clone(), render_json_templates(value, context)?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(Value::Object),
        other => Ok(other.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use agentdash_application_workflow::{
        OrchestrationExecutorLauncher, WorkflowRepositorySet,
        orchestration::activate_root_orchestration,
    };
    use agentdash_domain::workflow::{
        ApiRequestExecutorSpec, ExecutorSpec, FunctionActivityExecutorSpec, LifecycleRun,
        LifecycleRunRepository, OrchestrationLimits, OrchestrationPlanSnapshot,
        OrchestrationSourceRef, PlanNode, PlanNodeKind, RuntimeNodeStatus,
        WorkflowExecutorEffectIdentity, WorkflowExecutorEffectRepository,
        WorkflowFunctionEffectRequest,
    };
    use agentdash_platform_spi::{
        FunctionEffectObservation, FunctionEffectRequest, FunctionEffectSpec, FunctionRunner,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use uuid::Uuid;

    use crate::persistence::postgres::{
        PostgresLifecycleGateRepository, PostgresWorkflowExecutorEffectRepository,
        PostgresWorkflowRecoveryRepository, PostgresWorkflowRepository,
    };

    async fn isolated_pool() -> (PgPool, crate::postgres_runtime::PostgresRuntime) {
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/workflow-w8-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "workflow-w8-tests",
            8,
            data_root,
        )
        .await
        .expect("start Workflow W8 embedded PostgreSQL");
        let database_name = format!("workflow_w8_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create isolated Workflow W8 database");
        let options = runtime
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .database(&database_name);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .expect("connect isolated Workflow W8 database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("migrate isolated Workflow W8 database");
        (pool, runtime)
    }

    #[tokio::test]
    async fn restart_after_raw_io_before_terminal_receipt_never_reexecutes_effect() {
        let (pool, _runtime) = isolated_pool().await;
        let run_repo = PostgresWorkflowRepository::new(pool.clone());
        let run = LifecycleRun::new_control(Uuid::new_v4());
        run_repo.create(&run).await.expect("create LifecycleRun");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind Function test HTTP server");
        let address = listener.local_addr().expect("HTTP server address");
        let invocations = Arc::new(AtomicUsize::new(0));
        let recorded = invocations.clone();
        let server = tokio::spawn(async move {
            while let Ok(Ok((mut socket, _))) =
                tokio::time::timeout(Duration::from_millis(750), listener.accept()).await
            {
                recorded.fetch_add(1, Ordering::SeqCst);
                let mut request = [0_u8; 1024];
                let _ = socket.read(&mut request).await;
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await
                    .expect("write Function response");
            }
        });

        let spec = ApiRequestExecutorSpec {
            method: "GET".to_owned(),
            url_template: format!("http://{address}/effect"),
            body_template: None,
        };
        let identity = WorkflowExecutorEffectIdentity {
            effect_id: format!("workflow-function:{}", Uuid::new_v4()),
            lifecycle_run_id: run.id,
            orchestration_id: Uuid::new_v4(),
            node_path: "function".to_owned(),
            attempt: 1,
        };
        let prepared = WorkflowFunctionEffectRequest {
            identity: identity.clone(),
            payload_digest: "sha256:test-function-restart".to_owned(),
            spec: FunctionActivityExecutorSpec::ApiRequest(spec.clone()),
            context: json!({}),
        };
        PostgresWorkflowExecutorEffectRepository::new(pool.clone())
            .prepare_function(prepared.clone())
            .await
            .expect("prepare durable Function effect");
        let request = FunctionEffectRequest {
            effect_id: identity.effect_id.clone(),
            payload_digest: prepared.payload_digest,
            spec: FunctionEffectSpec::ApiRequest(spec),
            context: json!({}),
        };

        let first = DefaultFunctionRunner::new_with_fail_after_io(pool.clone())
            .with_test_lease(Duration::from_millis(150));
        first
            .execute_effect(request.clone())
            .await
            .expect_err("inject receipt-loss window");
        let restarted = DefaultFunctionRunner::new(pool.clone());
        assert_eq!(
            restarted
                .inspect_effect(&identity.effect_id)
                .await
                .expect("inspect claimed effect"),
            FunctionEffectObservation::InFlight
        );
        tokio::time::sleep(Duration::from_millis(175)).await;
        assert!(matches!(
            restarted
                .inspect_effect(&identity.effect_id)
                .await
                .expect("expired claim becomes Lost"),
            FunctionEffectObservation::Lost { .. }
        ));
        assert!(matches!(
            restarted
                .execute_effect(request)
                .await
                .expect("Lost effect replays"),
            FunctionEffectObservation::Lost { .. }
        ));
        server.await.expect("Function test server");
        assert_eq!(invocations.load(Ordering::SeqCst), 1);
        let runner_state = sqlx::query_scalar::<_, String>(
            "SELECT runner_state FROM workflow_executor_effects WHERE effect_id=$1",
        )
        .bind(&identity.effect_id)
        .fetch_one(&restarted.pool)
        .await
        .expect("load runner state");
        assert_eq!(runner_state, "lost");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind delayed Function server");
        let address = listener.local_addr().expect("delayed server address");
        let delayed_invocations = Arc::new(AtomicUsize::new(0));
        let delayed_recorded = delayed_invocations.clone();
        let request_started = Arc::new(tokio::sync::Notify::new());
        let started = request_started.clone();
        let delayed_server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept delayed request");
            delayed_recorded.fetch_add(1, Ordering::SeqCst);
            started.notify_one();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await;
            tokio::time::sleep(Duration::from_millis(250)).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .await
                .expect("write delayed Function response");
        });
        let delayed_spec = ApiRequestExecutorSpec {
            method: "GET".to_owned(),
            url_template: format!("http://{address}/delayed"),
            body_template: None,
        };
        let delayed_identity = WorkflowExecutorEffectIdentity {
            effect_id: format!("workflow-function:{}", Uuid::new_v4()),
            lifecycle_run_id: run.id,
            orchestration_id: Uuid::new_v4(),
            node_path: "delayed-function".to_owned(),
            attempt: 1,
        };
        let delayed_prepared = WorkflowFunctionEffectRequest {
            identity: delayed_identity.clone(),
            payload_digest: "sha256:test-function-late-receipt".to_owned(),
            spec: FunctionActivityExecutorSpec::ApiRequest(delayed_spec.clone()),
            context: json!({}),
        };
        PostgresWorkflowExecutorEffectRepository::new(pool.clone())
            .prepare_function(delayed_prepared.clone())
            .await
            .expect("prepare delayed Function effect");
        let delayed_request = FunctionEffectRequest {
            effect_id: delayed_identity.effect_id.clone(),
            payload_digest: delayed_prepared.payload_digest,
            spec: FunctionEffectSpec::ApiRequest(delayed_spec),
            context: json!({}),
        };
        let executing = DefaultFunctionRunner::new(pool.clone())
            .with_test_lease(Duration::from_millis(100))
            .without_heartbeat();
        let execution =
            tokio::spawn(async move { executing.execute_effect(delayed_request).await });
        request_started.notified().await;
        let inspector = DefaultFunctionRunner::new(pool.clone());
        assert_eq!(
            inspector
                .inspect_effect(&delayed_identity.effect_id)
                .await
                .expect("unexpired claim remains InFlight"),
            FunctionEffectObservation::InFlight
        );
        tokio::time::sleep(Duration::from_millis(125)).await;
        assert!(matches!(
            inspector
                .inspect_effect(&delayed_identity.effect_id)
                .await
                .expect("expired in-flight claim becomes Lost"),
            FunctionEffectObservation::Lost { .. }
        ));
        delayed_server.await.expect("delayed Function server");
        execution
            .await
            .expect("join delayed execution")
            .expect_err("late terminal is fenced");
        let late = sqlx::query(
            "SELECT runner_state,runner_late_evidence
             FROM workflow_executor_effects WHERE effect_id=$1",
        )
        .bind(&delayed_identity.effect_id)
        .fetch_one(&pool)
        .await
        .expect("load late receipt evidence");
        use sqlx::Row;
        assert_eq!(
            late.try_get::<String, _>("runner_state").expect("state"),
            "lost"
        );
        assert!(
            late.try_get::<Option<Value>, _>("runner_late_evidence")
                .expect("late evidence")
                .is_some()
        );
        assert_eq!(delayed_invocations.load(Ordering::SeqCst), 1);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind Workflow recovery Function server");
        let address = listener.local_addr().expect("recovery server address");
        let recovery_invocations = Arc::new(AtomicUsize::new(0));
        let recovery_recorded = recovery_invocations.clone();
        let recovery_server = tokio::spawn(async move {
            while let Ok(Ok((mut socket, _))) =
                tokio::time::timeout(Duration::from_millis(750), listener.accept()).await
            {
                recovery_recorded.fetch_add(1, Ordering::SeqCst);
                let mut request = [0_u8; 1024];
                let _ = socket.read(&mut request).await;
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await
                    .expect("write recovery Function response");
            }
        });
        let source_ref = OrchestrationSourceRef::Inline {
            source_digest: "sha256:workflow-recovery-function".to_owned(),
        };
        let plan = OrchestrationPlanSnapshot {
            plan_digest: "sha256:workflow-recovery-plan".to_owned(),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![PlanNode {
                node_id: "effect".to_owned(),
                node_path: "effect".to_owned(),
                parent_node_id: None,
                kind: PlanNodeKind::Function,
                label: Some("Recovery effect".to_owned()),
                executor: Some(ExecutorSpec::Function {
                    spec: FunctionActivityExecutorSpec::ApiRequest(ApiRequestExecutorSpec {
                        method: "GET".to_owned(),
                        url_template: format!("http://{address}/workflow-recovery"),
                        body_template: None,
                    }),
                }),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: None,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec!["effect".to_owned()],
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: chrono::Utc::now(),
        };
        let mut recovery_run = LifecycleRun::new_control(Uuid::new_v4());
        recovery_run.add_orchestration(activate_root_orchestration(source_ref, plan));
        run_repo
            .create(&recovery_run)
            .await
            .expect("create recoverable Workflow run");
        let repositories = WorkflowRepositorySet {
            lifecycle_run_repo: Arc::new(PostgresWorkflowRepository::new(pool.clone())),
            lifecycle_gate_repo: Arc::new(PostgresLifecycleGateRepository::new(pool.clone())),
            agent_procedure_repo: Arc::new(PostgresWorkflowRepository::new(pool.clone())),
        };
        let first_runner = Arc::new(
            DefaultFunctionRunner::new_with_fail_after_io(pool.clone())
                .with_test_lease(Duration::from_millis(150)),
        );
        let first_launcher = OrchestrationExecutorLauncher::new_durable(
            repositories.clone(),
            Arc::new(PostgresWorkflowExecutorEffectRepository::new(pool.clone())),
            first_runner,
        );
        first_launcher
            .drain_ready_nodes(recovery_run.id)
            .await
            .expect("receipt-loss Workflow remains inspect-only");
        tokio::time::sleep(Duration::from_millis(175)).await;
        let candidates = PostgresWorkflowRecoveryRepository::new(pool.clone())
            .list_recoverable_run_ids(64)
            .await
            .expect("scan recoverable Workflow runs");
        assert!(candidates.contains(&recovery_run.id));
        let recovery_launcher = OrchestrationExecutorLauncher::new_durable(
            repositories,
            Arc::new(PostgresWorkflowExecutorEffectRepository::new(pool.clone())),
            Arc::new(DefaultFunctionRunner::new(pool.clone())),
        );
        recovery_launcher
            .drain_ready_nodes(recovery_run.id)
            .await
            .expect("Lost Function converges to Blocked");
        let blocked = run_repo
            .get_by_id(recovery_run.id)
            .await
            .expect("load recovered Workflow")
            .expect("recovered Workflow exists");
        let node = &blocked.orchestrations[0].node_tree[0];
        assert_eq!(node.status, RuntimeNodeStatus::Blocked);
        assert_eq!(
            node.error.as_ref().map(|error| error.code.as_str()),
            Some("function_effect_outcome_lost")
        );
        recovery_server.await.expect("Workflow recovery server");
        assert_eq!(recovery_invocations.load(Ordering::SeqCst), 1);
    }
}
