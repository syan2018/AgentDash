use agentdash_domain::workflow::{
    LifecycleGate, WorkflowExecutorEffectRepository, WorkflowExecutorEffectRepositoryError,
    WorkflowFunctionEffectRecord, WorkflowFunctionEffectRequest, WorkflowFunctionTerminalResult,
    WorkflowHumanGateOpenEffect, WorkflowHumanGateOpenReceipt, WorkflowHumanGateResolutionEffect,
    WorkflowHumanGateResolutionReceipt,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresWorkflowExecutorEffectRepository {
    pool: PgPool,
}

impl PostgresWorkflowExecutorEffectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct FunctionEffectRow {
    request: Value,
    receipt: Option<Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ReceiptRow {
    receipt: Value,
}

#[async_trait]
impl WorkflowExecutorEffectRepository for PostgresWorkflowExecutorEffectRepository {
    async fn prepare_function(
        &self,
        request: WorkflowFunctionEffectRequest,
    ) -> Result<WorkflowFunctionEffectRecord, WorkflowExecutorEffectRepositoryError> {
        let request_json = json(&request)?;
        let identity = &request.identity;
        sqlx::query(
            "INSERT INTO workflow_executor_effects(
                 effect_id,effect_kind,lifecycle_run_id,orchestration_id,node_path,attempt,
                 payload_digest,request,state
             ) VALUES ($1,'function',$2,$3,$4,$5,$6,$7,'prepared')
             ON CONFLICT DO NOTHING",
        )
        .bind(&identity.effect_id)
        .bind(identity.lifecycle_run_id.to_string())
        .bind(identity.orchestration_id.to_string())
        .bind(&identity.node_path)
        .bind(i64::from(identity.attempt))
        .bind(&request.payload_digest)
        .bind(request_json)
        .execute(&self.pool)
        .await
        .map_err(persistence)?;

        let record = load_function(&self.pool, &identity.effect_id)
            .await?
            .ok_or_else(|| payload_conflict(&identity.effect_id))?;
        ensure_function_request(&request, &record)?;
        Ok(record)
    }

    async fn commit_function_terminal(
        &self,
        request: WorkflowFunctionEffectRequest,
        terminal: WorkflowFunctionTerminalResult,
    ) -> Result<WorkflowFunctionEffectRecord, WorkflowExecutorEffectRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        let row = load_function_for_update(&mut tx, &request.identity.effect_id)
            .await?
            .ok_or_else(|| persistence_message("prepared Function effect does not exist"))?;
        ensure_function_request(&request, &row)?;
        if let Some(existing) = row.terminal.as_ref() {
            if existing != &terminal {
                return Err(payload_conflict(&request.identity.effect_id));
            }
            tx.commit().await.map_err(persistence)?;
            return Ok(row);
        }
        sqlx::query(
            "UPDATE workflow_executor_effects
             SET state='terminal',receipt=$1,updated_at=NOW()
             WHERE effect_id=$2 AND effect_kind='function'",
        )
        .bind(json(&terminal)?)
        .bind(&request.identity.effect_id)
        .execute(&mut *tx)
        .await
        .map_err(persistence)?;
        let committed = load_function_for_update(&mut tx, &request.identity.effect_id)
            .await?
            .ok_or_else(|| persistence_message("committed Function effect disappeared"))?;
        tx.commit().await.map_err(persistence)?;
        Ok(committed)
    }

    async fn get_function(
        &self,
        effect_id: &str,
    ) -> Result<Option<WorkflowFunctionEffectRecord>, WorkflowExecutorEffectRepositoryError> {
        load_function(&self.pool, effect_id).await
    }

    async fn open_human_gate(
        &self,
        effect: WorkflowHumanGateOpenEffect,
    ) -> Result<WorkflowHumanGateOpenReceipt, WorkflowExecutorEffectRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        if let Some(existing) = load_receipt_by_effect::<WorkflowHumanGateOpenReceipt>(
            &mut tx,
            &effect.identity.effect_id,
            "human_gate_open",
        )
        .await?
        {
            ensure_open_effect(&effect, &existing.effect)?;
            tx.commit().await.map_err(persistence)?;
            return Ok(existing);
        }

        insert_gate(&mut tx, &effect.gate).await?;
        let receipt = WorkflowHumanGateOpenReceipt {
            effect: effect.clone(),
            committed_at: Utc::now(),
        };
        let identity = &effect.identity;
        let result = sqlx::query(
            "INSERT INTO workflow_executor_effects(
                 effect_id,effect_kind,lifecycle_run_id,orchestration_id,node_path,attempt,
                 payload_digest,state,gate_id,receipt,created_at,updated_at
             ) VALUES ($1,'human_gate_open',$2,$3,$4,$5,$6,'terminal',$7,$8,$9,$9)
             ON CONFLICT DO NOTHING",
        )
        .bind(&identity.effect_id)
        .bind(identity.lifecycle_run_id.to_string())
        .bind(identity.orchestration_id.to_string())
        .bind(&identity.node_path)
        .bind(i64::from(identity.attempt))
        .bind(&effect.payload_digest)
        .bind(effect.gate.id.to_string())
        .bind(json(&receipt)?)
        .bind(receipt.committed_at)
        .execute(&mut *tx)
        .await
        .map_err(persistence)?;
        if result.rows_affected() == 0 {
            let existing = load_receipt_by_effect::<WorkflowHumanGateOpenReceipt>(
                &mut tx,
                &identity.effect_id,
                "human_gate_open",
            )
            .await?
            .ok_or_else(|| payload_conflict(&identity.effect_id))?;
            ensure_open_effect(&effect, &existing.effect)?;
            tx.commit().await.map_err(persistence)?;
            return Ok(existing);
        }
        tx.commit().await.map_err(persistence)?;
        Ok(receipt)
    }

    async fn get_human_gate_open(
        &self,
        effect_id: &str,
    ) -> Result<Option<WorkflowHumanGateOpenReceipt>, WorkflowExecutorEffectRepositoryError> {
        let row = sqlx::query_as::<_, ReceiptRow>(
            "SELECT receipt FROM workflow_executor_effects
             WHERE effect_id=$1 AND effect_kind='human_gate_open'",
        )
        .bind(effect_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(persistence)?;
        row.map(|row| parse(row.receipt, "WorkflowHumanGateOpenReceipt"))
            .transpose()
    }

    async fn resolve_human_gate(
        &self,
        effect: WorkflowHumanGateResolutionEffect,
    ) -> Result<WorkflowHumanGateResolutionReceipt, WorkflowExecutorEffectRepositoryError> {
        let mut tx = self.pool.begin().await.map_err(persistence)?;
        if let Some(existing) = load_resolution_by_gate(&mut tx, effect.gate_id).await? {
            ensure_resolution_effect(&effect, &existing.effect)?;
            tx.commit().await.map_err(persistence)?;
            return Ok(existing);
        }
        let gate_status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM lifecycle_gates WHERE id=$1 FOR UPDATE",
        )
        .bind(effect.gate_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(persistence)?
        .ok_or_else(|| persistence_message("HumanGate does not exist"))?;
        if gate_status != "open" {
            let existing = load_resolution_by_gate(&mut tx, effect.gate_id)
                .await?
                .ok_or_else(|| payload_conflict(&effect.identity.effect_id))?;
            ensure_resolution_effect(&effect, &existing.effect)?;
            tx.commit().await.map_err(persistence)?;
            return Ok(existing);
        }
        let committed_at = Utc::now();
        sqlx::query(
            "UPDATE lifecycle_gates
             SET status='resolved',resolved_by=$1,resolved_at=$2
             WHERE id=$3 AND status='open'",
        )
        .bind(&effect.resolved_by)
        .bind(committed_at)
        .bind(effect.gate_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(persistence)?;
        let receipt = WorkflowHumanGateResolutionReceipt {
            effect: effect.clone(),
            committed_at,
        };
        let identity = &effect.identity;
        let result = sqlx::query(
            "INSERT INTO workflow_executor_effects(
                 effect_id,effect_kind,lifecycle_run_id,orchestration_id,node_path,attempt,
                 payload_digest,state,gate_id,receipt,created_at,updated_at
             ) VALUES ($1,'human_gate_resolution',$2,$3,$4,$5,$6,'terminal',$7,$8,$9,$9)
             ON CONFLICT DO NOTHING",
        )
        .bind(&identity.effect_id)
        .bind(identity.lifecycle_run_id.to_string())
        .bind(identity.orchestration_id.to_string())
        .bind(&identity.node_path)
        .bind(i64::from(identity.attempt))
        .bind(&effect.payload_digest)
        .bind(effect.gate_id.to_string())
        .bind(json(&receipt)?)
        .bind(committed_at)
        .execute(&mut *tx)
        .await
        .map_err(persistence)?;
        if result.rows_affected() == 0 {
            return Err(payload_conflict(&identity.effect_id));
        }
        tx.commit().await.map_err(persistence)?;
        Ok(receipt)
    }

    async fn get_human_gate_resolution(
        &self,
        gate_id: Uuid,
    ) -> Result<Option<WorkflowHumanGateResolutionReceipt>, WorkflowExecutorEffectRepositoryError>
    {
        let row = sqlx::query_as::<_, ReceiptRow>(
            "SELECT receipt FROM workflow_executor_effects
             WHERE gate_id=$1 AND effect_kind='human_gate_resolution'",
        )
        .bind(gate_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(persistence)?;
        row.map(|row| parse(row.receipt, "WorkflowHumanGateResolutionReceipt"))
            .transpose()
    }
}

async fn load_function(
    pool: &PgPool,
    effect_id: &str,
) -> Result<Option<WorkflowFunctionEffectRecord>, WorkflowExecutorEffectRepositoryError> {
    let row = sqlx::query_as::<_, FunctionEffectRow>(
        "SELECT request,receipt,created_at,updated_at
         FROM workflow_executor_effects
         WHERE effect_id=$1 AND effect_kind='function'",
    )
    .bind(effect_id)
    .fetch_optional(pool)
    .await
    .map_err(persistence)?;
    row.map(function_record).transpose()
}

async fn load_function_for_update(
    tx: &mut Transaction<'_, Postgres>,
    effect_id: &str,
) -> Result<Option<WorkflowFunctionEffectRecord>, WorkflowExecutorEffectRepositoryError> {
    let row = sqlx::query_as::<_, FunctionEffectRow>(
        "SELECT request,receipt,created_at,updated_at
         FROM workflow_executor_effects
         WHERE effect_id=$1 AND effect_kind='function'
         FOR UPDATE",
    )
    .bind(effect_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(persistence)?;
    row.map(function_record).transpose()
}

fn function_record(
    row: FunctionEffectRow,
) -> Result<WorkflowFunctionEffectRecord, WorkflowExecutorEffectRepositoryError> {
    Ok(WorkflowFunctionEffectRecord {
        request: parse(row.request, "WorkflowFunctionEffectRequest")?,
        terminal: row
            .receipt
            .map(|value| parse(value, "WorkflowFunctionTerminalResult"))
            .transpose()?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

async fn insert_gate(
    tx: &mut Transaction<'_, Postgres>,
    gate: &LifecycleGate,
) -> Result<(), WorkflowExecutorEffectRepositoryError> {
    let result = sqlx::query(
        "INSERT INTO lifecycle_gates(
             id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,
             resolved_by,created_at,resolved_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(gate.id.to_string())
    .bind(gate.run_id.to_string())
    .bind(gate.agent_id.map(|id| id.to_string()))
    .bind(gate.frame_id.map(|id| id.to_string()))
    .bind(&gate.gate_kind)
    .bind(&gate.correlation_id)
    .bind(&gate.status)
    .bind(&gate.payload_json)
    .bind(&gate.resolved_by)
    .bind(gate.created_at)
    .bind(gate.resolved_at)
    .execute(&mut **tx)
    .await
    .map_err(persistence)?;
    if result.rows_affected() == 0 {
        let existing = sqlx::query_scalar::<_, Value>(
            "SELECT jsonb_build_object(
                 'id',id,'run_id',run_id,'agent_id',agent_id,'frame_id',frame_id,
                 'gate_kind',gate_kind,'correlation_id',correlation_id,'status',status,
                 'payload_json',payload_json,'resolved_by',resolved_by,
                 'created_at',created_at,'resolved_at',resolved_at
             ) FROM lifecycle_gates WHERE id=$1",
        )
        .bind(gate.id.to_string())
        .fetch_one(&mut **tx)
        .await
        .map_err(persistence)?;
        let existing: LifecycleGate = parse(existing, "LifecycleGate")?;
        if !same_open_gate(&existing, gate) {
            return Err(payload_conflict(&gate.id.to_string()));
        }
    }
    Ok(())
}

async fn load_receipt_by_effect<T: DeserializeOwned>(
    tx: &mut Transaction<'_, Postgres>,
    effect_id: &str,
    kind: &str,
) -> Result<Option<T>, WorkflowExecutorEffectRepositoryError> {
    let row = sqlx::query_as::<_, ReceiptRow>(
        "SELECT receipt FROM workflow_executor_effects
         WHERE effect_id=$1 AND effect_kind=$2 FOR UPDATE",
    )
    .bind(effect_id)
    .bind(kind)
    .fetch_optional(&mut **tx)
    .await
    .map_err(persistence)?;
    row.map(|row| parse(row.receipt, "workflow executor receipt"))
        .transpose()
}

async fn load_resolution_by_gate(
    tx: &mut Transaction<'_, Postgres>,
    gate_id: Uuid,
) -> Result<Option<WorkflowHumanGateResolutionReceipt>, WorkflowExecutorEffectRepositoryError> {
    let row = sqlx::query_as::<_, ReceiptRow>(
        "SELECT receipt FROM workflow_executor_effects
         WHERE gate_id=$1 AND effect_kind='human_gate_resolution' FOR UPDATE",
    )
    .bind(gate_id.to_string())
    .fetch_optional(&mut **tx)
    .await
    .map_err(persistence)?;
    row.map(|row| parse(row.receipt, "WorkflowHumanGateResolutionReceipt"))
        .transpose()
}

fn ensure_function_request(
    expected: &WorkflowFunctionEffectRequest,
    actual: &WorkflowFunctionEffectRecord,
) -> Result<(), WorkflowExecutorEffectRepositoryError> {
    if &actual.request != expected {
        return Err(payload_conflict(&expected.identity.effect_id));
    }
    Ok(())
}

fn ensure_open_effect(
    expected: &WorkflowHumanGateOpenEffect,
    actual: &WorkflowHumanGateOpenEffect,
) -> Result<(), WorkflowExecutorEffectRepositoryError> {
    if expected.identity != actual.identity
        || expected.payload_digest != actual.payload_digest
        || !same_open_gate(&expected.gate, &actual.gate)
    {
        return Err(payload_conflict(&expected.identity.effect_id));
    }
    Ok(())
}

fn same_open_gate(left: &LifecycleGate, right: &LifecycleGate) -> bool {
    left.id == right.id
        && left.run_id == right.run_id
        && left.agent_id == right.agent_id
        && left.frame_id == right.frame_id
        && left.gate_kind == right.gate_kind
        && left.correlation_id == right.correlation_id
        && left.status == right.status
        && left.payload_json == right.payload_json
        && left.resolved_by == right.resolved_by
        && left.resolved_at == right.resolved_at
}

fn ensure_resolution_effect(
    expected: &WorkflowHumanGateResolutionEffect,
    actual: &WorkflowHumanGateResolutionEffect,
) -> Result<(), WorkflowExecutorEffectRepositoryError> {
    if expected != actual {
        return Err(payload_conflict(&expected.identity.effect_id));
    }
    Ok(())
}

fn json<T: serde::Serialize>(value: &T) -> Result<Value, WorkflowExecutorEffectRepositoryError> {
    serde_json::to_value(value).map_err(|error| persistence_message(error.to_string()))
}

fn parse<T: DeserializeOwned>(
    value: Value,
    context: &str,
) -> Result<T, WorkflowExecutorEffectRepositoryError> {
    serde_json::from_value(value)
        .map_err(|error| persistence_message(format!("{context} decode failed: {error}")))
}

fn payload_conflict(effect_id: &str) -> WorkflowExecutorEffectRepositoryError {
    WorkflowExecutorEffectRepositoryError::PayloadConflict {
        effect_id: effect_id.to_owned(),
    }
}

fn persistence(error: sqlx::Error) -> WorkflowExecutorEffectRepositoryError {
    persistence_message(error.to_string())
}

fn persistence_message(message: impl Into<String>) -> WorkflowExecutorEffectRepositoryError {
    WorkflowExecutorEffectRepositoryError::Persistence(message.into())
}
