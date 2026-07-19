use agentdash_application_agentrun::agent_run::{
    WorkflowAgentCallBindingCommit, WorkflowAgentCallProductGraphRepository,
    WorkflowAgentCallProductPhase, WorkflowAgentCallProductPhaseIdentity,
    WorkflowAgentCallProductRepositoryError, WorkflowAgentCallProductSaga,
    WorkflowAgentCallProductSagaRepository, WorkflowAgentCallTargetMaterialization,
};
use agentdash_application_workflow::{
    WorkflowAgentCallMailboxState, WorkflowAgentCallRequest, WorkflowAgentCallTargetIntent,
};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Clone)]
pub struct PostgresWorkflowAgentCallRepository {
    pool: PgPool,
}

impl PostgresWorkflowAgentCallRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct SagaRow {
    request: Value,
    runtime_thread_id: String,
    receipts: Value,
    in_flight: Option<String>,
    source_binding: Option<Value>,
    mailbox_state: Option<String>,
    version: i64,
}

#[async_trait]
impl WorkflowAgentCallProductSagaRepository for PostgresWorkflowAgentCallRepository {
    async fn prepare(
        &self,
        saga: WorkflowAgentCallProductSaga,
    ) -> Result<WorkflowAgentCallProductSaga, WorkflowAgentCallProductRepositoryError> {
        if saga.version != 0 || !saga.receipts.is_empty() || saga.in_flight.is_some() {
            return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
        }
        let request = &saga.request;
        let identity = &request.identity;
        let target = saga.target();
        let request_json = encode(request)?;
        let result = sqlx::query(
            "INSERT INTO workflow_agent_call_product_sagas(
                 request_id,lifecycle_run_id,orchestration_id,node_path,attempt,payload_digest,
                 request,target_run_id,target_agent_id,runtime_thread_id,phase_plan,receipts,
                 in_flight,source_binding,mailbox_state,version
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NULL,$13,$14,0)
             ON CONFLICT DO NOTHING",
        )
        .bind(&identity.request_id)
        .bind(identity.lifecycle_run_id.to_string())
        .bind(identity.orchestration_id.to_string())
        .bind(&identity.node_path)
        .bind(i64::from(identity.attempt))
        .bind(&request.payload_digest)
        .bind(request_json)
        .bind(target.run_id.to_string())
        .bind(target.agent_id.to_string())
        .bind(saga.runtime_thread_id.as_str())
        .bind(phase_plan_json(request))
        .bind(encode(&saga.receipts)?)
        .bind(encode_optional(saga.source_binding.as_ref())?)
        .bind(mailbox_slug(saga.mailbox_state.as_ref()))
        .execute(&self.pool)
        .await
        .map_err(repository_db)?;
        let existing = load_saga(&self.pool, &identity.request_id).await?;
        let existing = match existing {
            Some(existing) => existing,
            None if result.rows_affected() == 0 => {
                return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
            }
            None => {
                return Err(repository_persistence(
                    "prepared AgentCall saga disappeared",
                ));
            }
        };
        if result.rows_affected() == 0
            && (existing.request != saga.request
                || existing.runtime_thread_id != saga.runtime_thread_id)
        {
            return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
        }
        Ok(existing)
    }

    async fn save(
        &self,
        expected_version: u64,
        mut saga: WorkflowAgentCallProductSaga,
    ) -> Result<WorkflowAgentCallProductSaga, WorkflowAgentCallProductRepositoryError> {
        let request_id = saga.request.identity.request_id.clone();
        let mut tx = self.pool.begin().await.map_err(repository_db)?;
        let existing = load_saga_for_update(&mut tx, &request_id)
            .await?
            .ok_or_else(|| repository_persistence("prepared AgentCall saga does not exist"))?;
        if existing.version != expected_version {
            return Err(WorkflowAgentCallProductRepositoryError::VersionConflict);
        }
        validate_saga_transition(&existing, &saga)?;
        let next_version = expected_version
            .checked_add(1)
            .ok_or_else(|| repository_persistence("AgentCall saga version overflow"))?;
        saga.version = next_version;
        synchronize_phase_effects(&mut tx, &saga).await?;
        let updated = sqlx::query(
            "UPDATE workflow_agent_call_product_sagas
             SET receipts=$1,in_flight=$2,source_binding=$3,mailbox_state=$4,
                 version=$5,updated_at=NOW()
             WHERE request_id=$6 AND version=$7",
        )
        .bind(encode(&saga.receipts)?)
        .bind(saga.in_flight.map(phase_slug))
        .bind(encode_optional(saga.source_binding.as_ref())?)
        .bind(mailbox_slug(saga.mailbox_state.as_ref()))
        .bind(to_i64(next_version)?)
        .bind(&request_id)
        .bind(to_i64(expected_version)?)
        .execute(&mut *tx)
        .await
        .map_err(repository_db)?;
        if updated.rows_affected() != 1 {
            return Err(WorkflowAgentCallProductRepositoryError::VersionConflict);
        }
        tx.commit().await.map_err(repository_db)?;
        Ok(saga)
    }
}

#[async_trait]
impl WorkflowAgentCallProductGraphRepository for PostgresWorkflowAgentCallRepository {
    async fn materialize_target_idempotent(
        &self,
        mutation: WorkflowAgentCallTargetMaterialization,
    ) -> Result<WorkflowAgentCallTargetMaterialization, String> {
        let mut tx = self.pool.begin().await.map_err(db_string)?;
        let request = load_bound_request(&mut tx, &mutation.request_id).await?;
        if request.payload_digest != mutation.payload_digest
            || request.target_intent.target() != &mutation.target
        {
            return Err("Workflow AgentCall target materialization binding conflict".to_owned());
        }
        if let Some(existing) = load_graph_effect(&mut tx, &mutation.effect_id).await? {
            if existing != target_materialization_json(&mutation) {
                return Err("Workflow AgentCall target materialization conflict".to_owned());
            }
            ensure_materialized_target(&mut tx, &request, &mutation.target).await?;
            tx.commit().await.map_err(db_string)?;
            return Ok(mutation);
        }
        let run_project = sqlx::query_scalar::<_, String>(
            "SELECT project_id FROM lifecycle_runs WHERE id=$1 FOR UPDATE",
        )
        .bind(mutation.target.run_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_string)?
        .ok_or_else(|| "Workflow AgentCall target LifecycleRun does not exist".to_owned())?;
        if run_project != request.project_id.to_string() {
            return Err("Workflow AgentCall target project drifted".to_owned());
        }
        let now = chrono::Utc::now();
        let agent_insert = sqlx::query(
            "INSERT INTO lifecycle_agents(
                 id,run_id,project_id,created_by_user_id,source,project_agent_id,status,
                 bootstrap_status,workspace_title,workspace_title_source,created_at,updated_at
             ) VALUES ($1,$2,$3,$4,'workflow_agent',NULL,'active','pending',NULL,NULL,$5,$5)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(mutation.target.agent_id.to_string())
        .bind(mutation.target.run_id.to_string())
        .bind(request.project_id.to_string())
        .bind(&request.created_by_user_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(db_string)?;
        if agent_insert.rows_affected() == 0 {
            let existing = sqlx::query(
                "SELECT run_id,project_id,created_by_user_id,source
                 FROM lifecycle_agents WHERE id=$1",
            )
            .bind(mutation.target.agent_id.to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(db_string)?;
            if existing.try_get::<String, _>("run_id").map_err(db_string)?
                != mutation.target.run_id.to_string()
                || existing
                    .try_get::<String, _>("project_id")
                    .map_err(db_string)?
                    != request.project_id.to_string()
                || existing
                    .try_get::<String, _>("created_by_user_id")
                    .map_err(db_string)?
                    != request.created_by_user_id
                || existing.try_get::<String, _>("source").map_err(db_string)? != "workflow_agent"
            {
                return Err("Workflow AgentCall target graph drifted".to_owned());
            }
        }
        insert_graph_effect(
            &mut tx,
            &mutation.effect_id,
            &mutation.request_id,
            &mutation.payload_digest,
            "materialize_target",
            &mutation.target,
            None,
            None,
            target_materialization_json(&mutation),
        )
        .await?;
        tx.commit().await.map_err(db_string)?;
        Ok(mutation)
    }

    async fn commit_runtime_binding_idempotent(
        &self,
        mutation: WorkflowAgentCallBindingCommit,
    ) -> Result<WorkflowAgentCallBindingCommit, String> {
        let mut tx = self.pool.begin().await.map_err(db_string)?;
        let request = load_bound_request(&mut tx, &mutation.request_id).await?;
        if request.payload_digest != mutation.payload_digest
            || request.target_intent.target() != &mutation.target
        {
            return Err("Workflow AgentCall binding request drifted".to_owned());
        }
        let binding_json = product_binding_json(&mutation);
        if let Some(existing) = load_graph_effect(&mut tx, &mutation.effect_id).await? {
            if existing != binding_commit_json(&mutation) {
                return Err("Workflow AgentCall binding commit conflict".to_owned());
            }
            let stored = sqlx::query_scalar::<_, Value>(
                "SELECT binding FROM agent_run_product_runtime_binding
                 WHERE target_run_id=$1 AND target_agent_id=$2",
            )
            .bind(mutation.target.run_id.to_string())
            .bind(mutation.target.agent_id.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_string)?;
            if stored.as_ref() != Some(&binding_json) {
                return Err("Workflow AgentCall Product binding drifted".to_owned());
            }
            tx.commit().await.map_err(db_string)?;
            return Ok(mutation);
        }
        let project_id = sqlx::query_scalar::<_, String>(
            "SELECT project_id FROM lifecycle_agents
             WHERE id=$1 AND run_id=$2 FOR UPDATE",
        )
        .bind(mutation.target.agent_id.to_string())
        .bind(mutation.target.run_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_string)?
        .ok_or_else(|| "Workflow AgentCall target agent does not exist".to_owned())?;
        let evidence = &mutation.binding;
        let binding_insert = sqlx::query(
            "INSERT INTO agent_run_product_runtime_binding(
                 target_run_id,target_agent_id,project_id,runtime_thread_id,source_ref,
                 source_committed_revision,source_applied_surface_revision,
                 source_activated_revision,binding
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (target_run_id,target_agent_id) DO NOTHING",
        )
        .bind(mutation.target.run_id.to_string())
        .bind(mutation.target.agent_id.to_string())
        .bind(project_id)
        .bind(mutation.runtime_thread_id.as_str())
        .bind(evidence.source_ref.as_str())
        .bind(to_i64_string(evidence.committed_at_revision.0)?)
        .bind(to_i64_string(evidence.applied_surface_revision.0)?)
        .bind(
            evidence
                .activated_at_revision
                .map(|revision| to_i64_string(revision.0))
                .transpose()?,
        )
        .bind(&binding_json)
        .execute(&mut *tx)
        .await
        .map_err(db_string)?;
        if binding_insert.rows_affected() == 0 {
            let existing = sqlx::query_scalar::<_, Value>(
                "SELECT binding FROM agent_run_product_runtime_binding
                 WHERE target_run_id=$1 AND target_agent_id=$2",
            )
            .bind(mutation.target.run_id.to_string())
            .bind(mutation.target.agent_id.to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(db_string)?;
            if existing != binding_json {
                return Err("Workflow AgentCall Product binding drifted".to_owned());
            }
        }
        insert_graph_effect(
            &mut tx,
            &mutation.effect_id,
            &mutation.request_id,
            &mutation.payload_digest,
            "commit_runtime_binding",
            &mutation.target,
            Some(mutation.runtime_thread_id.as_str()),
            Some(serde_json::to_value(&mutation.binding).map_err(|error| error.to_string())?),
            binding_commit_json(&mutation),
        )
        .await?;
        tx.commit().await.map_err(db_string)?;
        Ok(mutation)
    }
}

async fn ensure_materialized_target(
    tx: &mut Transaction<'_, Postgres>,
    request: &WorkflowAgentCallRequest,
    target: &agentdash_domain::agent_run_target::AgentRunTarget,
) -> Result<(), String> {
    let agent = sqlx::query(
        "SELECT run_id,project_id,created_by_user_id,source
         FROM lifecycle_agents WHERE id=$1",
    )
    .bind(target.agent_id.to_string())
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_string)?
    .ok_or_else(|| "Workflow AgentCall committed target disappeared".to_owned())?;
    if agent.try_get::<String, _>("run_id").map_err(db_string)? != target.run_id.to_string()
        || agent
            .try_get::<String, _>("project_id")
            .map_err(db_string)?
            != request.project_id.to_string()
        || agent
            .try_get::<String, _>("created_by_user_id")
            .map_err(db_string)?
            != request.created_by_user_id
        || agent.try_get::<String, _>("source").map_err(db_string)? != "workflow_agent"
    {
        return Err("Workflow AgentCall committed target graph drifted".to_owned());
    }
    Ok(())
}

async fn load_saga(
    pool: &PgPool,
    request_id: &str,
) -> Result<Option<WorkflowAgentCallProductSaga>, WorkflowAgentCallProductRepositoryError> {
    sqlx::query_as::<_, SagaRow>(
        "SELECT request,runtime_thread_id,receipts,in_flight,source_binding,mailbox_state,version
         FROM workflow_agent_call_product_sagas WHERE request_id=$1",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await
    .map_err(repository_db)?
    .map(map_saga)
    .transpose()
}

async fn load_saga_for_update(
    tx: &mut Transaction<'_, Postgres>,
    request_id: &str,
) -> Result<Option<WorkflowAgentCallProductSaga>, WorkflowAgentCallProductRepositoryError> {
    sqlx::query_as::<_, SagaRow>(
        "SELECT request,runtime_thread_id,receipts,in_flight,source_binding,mailbox_state,version
         FROM workflow_agent_call_product_sagas WHERE request_id=$1 FOR UPDATE",
    )
    .bind(request_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(repository_db)?
    .map(map_saga)
    .transpose()
}

fn map_saga(
    row: SagaRow,
) -> Result<WorkflowAgentCallProductSaga, WorkflowAgentCallProductRepositoryError> {
    Ok(WorkflowAgentCallProductSaga {
        request: decode_repository(row.request, "WorkflowAgentCallRequest")?,
        runtime_thread_id: agentdash_agent_runtime_contract::RuntimeThreadId::new(
            row.runtime_thread_id,
        )
        .map_err(|error| repository_persistence(error.to_string()))?,
        version: u64::try_from(row.version)
            .map_err(|error| repository_persistence(error.to_string()))?,
        receipts: decode_repository(row.receipts, "WorkflowAgentCall receipts")?,
        in_flight: row.in_flight.map(|phase| parse_phase(&phase)).transpose()?,
        source_binding: row
            .source_binding
            .map(|value| decode_repository(value, "ManagedRuntimeSourceBindingEvidence"))
            .transpose()?,
        mailbox_state: row
            .mailbox_state
            .map(|state| parse_mailbox(&state))
            .transpose()?,
    })
}

fn validate_saga_transition(
    existing: &WorkflowAgentCallProductSaga,
    proposed: &WorkflowAgentCallProductSaga,
) -> Result<(), WorkflowAgentCallProductRepositoryError> {
    if existing.request != proposed.request
        || existing.runtime_thread_id != proposed.runtime_thread_id
        || proposed.version != existing.version
        || proposed.receipts.len() < existing.receipts.len()
        || proposed.receipts[..existing.receipts.len()] != existing.receipts
        || proposed.receipts.len() > existing.receipts.len() + 1
    {
        return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
    }
    let mut phases = std::collections::BTreeSet::new();
    let mut operation_ids = std::collections::BTreeSet::new();
    let phase_plan = phase_plan(&proposed.request);
    for (index, receipt) in proposed.receipts.iter().enumerate() {
        let expected_identity =
            phase_identity(&proposed.request.identity.request_id, receipt.phase)?;
        if !phases.insert(receipt.phase)
            || phase_plan.get(index) != Some(&receipt.phase)
            || receipt.identity != expected_identity
            || receipt
                .identity
                .runtime_operation_id
                .as_ref()
                .is_some_and(|operation| !operation_ids.insert(operation.as_str()))
        {
            return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
        }
    }
    if proposed
        .in_flight
        .is_some_and(|phase| phase_plan.get(proposed.receipts.len()) != Some(&phase))
    {
        return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
    }
    if let Some(in_flight) = existing.in_flight {
        let applied_in_this_save = proposed.receipts.len() == existing.receipts.len() + 1
            && proposed
                .receipts
                .last()
                .is_some_and(|receipt| receipt.phase == in_flight);
        if proposed.in_flight != Some(in_flight) && !applied_in_this_save {
            return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
        }
    }
    if existing.mailbox_state.is_some() && existing.mailbox_state != proposed.mailbox_state {
        return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
    }
    if let Some(binding) = existing.source_binding.as_ref() {
        let Some(next) = proposed.source_binding.as_ref() else {
            return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
        };
        if binding.source_ref != next.source_ref
            || binding.committed_at_revision != next.committed_at_revision
            || binding.applied_surface_revision != next.applied_surface_revision
            || (binding.activated_at_revision.is_some()
                && binding.activated_at_revision != next.activated_at_revision)
        {
            return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
        }
    }
    Ok(())
}

async fn synchronize_phase_effects(
    tx: &mut Transaction<'_, Postgres>,
    saga: &WorkflowAgentCallProductSaga,
) -> Result<(), WorkflowAgentCallProductRepositoryError> {
    let request_id = &saga.request.identity.request_id;
    if let Some(phase) = saga.in_flight {
        let identity = phase_identity(request_id, phase)?;
        upsert_phase_effect(tx, saga, phase, &identity, "dispatched", None).await?;
    }
    for receipt in &saga.receipts {
        upsert_phase_effect(
            tx,
            saga,
            receipt.phase,
            &receipt.identity,
            "applied",
            Some(encode(receipt)?),
        )
        .await?;
    }
    Ok(())
}

async fn upsert_phase_effect(
    tx: &mut Transaction<'_, Postgres>,
    saga: &WorkflowAgentCallProductSaga,
    phase: WorkflowAgentCallProductPhase,
    identity: &WorkflowAgentCallProductPhaseIdentity,
    state: &str,
    evidence: Option<Value>,
) -> Result<(), WorkflowAgentCallProductRepositoryError> {
    let target = saga.target();
    let inserted = sqlx::query(
        "INSERT INTO workflow_agent_call_product_effects(
             effect_id,request_id,phase,runtime_operation_id,payload_digest,state,
             target_run_id,target_agent_id,runtime_thread_id,evidence
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (effect_id) DO NOTHING",
    )
    .bind(&identity.effect_id)
    .bind(&saga.request.identity.request_id)
    .bind(phase_slug(phase))
    .bind(
        identity
            .runtime_operation_id
            .as_ref()
            .map(|value| value.as_str()),
    )
    .bind(&saga.request.payload_digest)
    .bind(state)
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .bind(saga.runtime_thread_id.as_str())
    .bind(evidence.as_ref())
    .execute(&mut **tx)
    .await
    .map_err(repository_db)?;
    if inserted.rows_affected() == 0 {
        let existing = sqlx::query(
            "SELECT request_id,phase,runtime_operation_id,payload_digest,state,
                    target_run_id,target_agent_id,runtime_thread_id,evidence
             FROM workflow_agent_call_product_effects WHERE effect_id=$1 FOR UPDATE",
        )
        .bind(&identity.effect_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(repository_db)?;
        let identity_matches = existing
            .try_get::<String, _>("request_id")
            .map_err(repository_db)?
            == saga.request.identity.request_id
            && existing
                .try_get::<String, _>("phase")
                .map_err(repository_db)?
                == phase_slug(phase)
            && existing
                .try_get::<Option<String>, _>("runtime_operation_id")
                .map_err(repository_db)?
                == identity
                    .runtime_operation_id
                    .as_ref()
                    .map(|value| value.as_str().to_owned())
            && existing
                .try_get::<String, _>("payload_digest")
                .map_err(repository_db)?
                == saga.request.payload_digest
            && existing
                .try_get::<String, _>("target_run_id")
                .map_err(repository_db)?
                == target.run_id.to_string()
            && existing
                .try_get::<String, _>("target_agent_id")
                .map_err(repository_db)?
                == target.agent_id.to_string()
            && existing
                .try_get::<String, _>("runtime_thread_id")
                .map_err(repository_db)?
                == saga.runtime_thread_id.as_str();
        if !identity_matches {
            return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict);
        }
        let old_state = existing
            .try_get::<String, _>("state")
            .map_err(repository_db)?;
        let old_evidence = existing
            .try_get::<Option<Value>, _>("evidence")
            .map_err(repository_db)?;
        match (old_state.as_str(), state, old_evidence, evidence) {
            ("dispatched", "dispatched", None, None) => {}
            ("dispatched", "applied", None, Some(evidence)) => {
                sqlx::query(
                    "UPDATE workflow_agent_call_product_effects
                     SET state='applied',evidence=$1,updated_at=NOW() WHERE effect_id=$2",
                )
                .bind(evidence)
                .bind(&identity.effect_id)
                .execute(&mut **tx)
                .await
                .map_err(repository_db)?;
            }
            ("applied", "applied", Some(old), Some(new)) if old == new => {}
            _ => return Err(WorkflowAgentCallProductRepositoryError::PayloadConflict),
        }
    }
    Ok(())
}

async fn load_bound_request(
    tx: &mut Transaction<'_, Postgres>,
    request_id: &str,
) -> Result<WorkflowAgentCallRequest, String> {
    let request = sqlx::query_scalar::<_, Value>(
        "SELECT request FROM workflow_agent_call_product_sagas
         WHERE request_id=$1 FOR UPDATE",
    )
    .bind(request_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_string)?
    .ok_or_else(|| "Workflow AgentCall prepared saga does not exist".to_owned())?;
    decode(request, "WorkflowAgentCallRequest")
}

async fn load_graph_effect(
    tx: &mut Transaction<'_, Postgres>,
    effect_id: &str,
) -> Result<Option<Value>, String> {
    sqlx::query_scalar(
        "SELECT ledger_payload FROM workflow_agent_call_product_graph_effects
         WHERE effect_id=$1 FOR UPDATE",
    )
    .bind(effect_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_string)
}

#[allow(clippy::too_many_arguments)]
async fn insert_graph_effect(
    tx: &mut Transaction<'_, Postgres>,
    effect_id: &str,
    request_id: &str,
    payload_digest: &str,
    effect_kind: &str,
    target: &agentdash_domain::agent_run_target::AgentRunTarget,
    runtime_thread_id: Option<&str>,
    evidence_binding: Option<Value>,
    ledger_payload: Value,
) -> Result<(), String> {
    let result = sqlx::query(
        "INSERT INTO workflow_agent_call_product_graph_effects(
             effect_id,request_id,payload_digest,effect_kind,target_run_id,target_agent_id,
             runtime_thread_id,binding,ledger_payload
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         ON CONFLICT DO NOTHING",
    )
    .bind(effect_id)
    .bind(request_id)
    .bind(payload_digest)
    .bind(effect_kind)
    .bind(target.run_id.to_string())
    .bind(target.agent_id.to_string())
    .bind(runtime_thread_id)
    .bind(evidence_binding)
    .bind(ledger_payload)
    .execute(&mut **tx)
    .await
    .map_err(db_string)?;
    if result.rows_affected() != 1 {
        return Err("Workflow AgentCall graph effect identity conflict".to_owned());
    }
    Ok(())
}

fn phase_identity(
    request_id: &str,
    phase: WorkflowAgentCallProductPhase,
) -> Result<WorkflowAgentCallProductPhaseIdentity, WorkflowAgentCallProductRepositoryError> {
    let effect_id = format!("{request_id}:{}", phase_slug_hyphen(phase));
    let runtime_operation_id = match phase {
        WorkflowAgentCallProductPhase::CreateRuntime
        | WorkflowAgentCallProductPhase::ActivateRuntime
        | WorkflowAgentCallProductPhase::SubmitInput => Some(
            agentdash_agent_runtime_contract::RuntimeOperationId::new(effect_id.clone())
                .map_err(|error| repository_persistence(error.to_string()))?,
        ),
        WorkflowAgentCallProductPhase::MaterializeTarget
        | WorkflowAgentCallProductPhase::CommitBinding => None,
    };
    Ok(WorkflowAgentCallProductPhaseIdentity {
        effect_id,
        runtime_operation_id,
    })
}

fn phase_plan_json(request: &WorkflowAgentCallRequest) -> Value {
    Value::Array(
        phase_plan(request)
            .into_iter()
            .map(|phase| Value::String(phase_slug(phase).to_owned()))
            .collect(),
    )
}

fn phase_plan(request: &WorkflowAgentCallRequest) -> Vec<WorkflowAgentCallProductPhase> {
    match request.target_intent {
        WorkflowAgentCallTargetIntent::CreateNew { .. } => vec![
            WorkflowAgentCallProductPhase::MaterializeTarget,
            WorkflowAgentCallProductPhase::CreateRuntime,
            WorkflowAgentCallProductPhase::ActivateRuntime,
            WorkflowAgentCallProductPhase::CommitBinding,
            WorkflowAgentCallProductPhase::SubmitInput,
        ],
        WorkflowAgentCallTargetIntent::ContinueCurrent { .. } => {
            vec![WorkflowAgentCallProductPhase::SubmitInput]
        }
    }
}

fn target_materialization_json(mutation: &WorkflowAgentCallTargetMaterialization) -> Value {
    json!({
        "request_id": mutation.request_id,
        "payload_digest": mutation.payload_digest,
        "target": mutation.target,
        "effect_id": mutation.effect_id,
    })
}

fn binding_commit_json(mutation: &WorkflowAgentCallBindingCommit) -> Value {
    json!({
        "request_id": mutation.request_id,
        "payload_digest": mutation.payload_digest,
        "target": mutation.target,
        "runtime_thread_id": mutation.runtime_thread_id,
        "binding": mutation.binding,
        "effect_id": mutation.effect_id,
    })
}

fn product_binding_json(mutation: &WorkflowAgentCallBindingCommit) -> Value {
    json!({
        "target": {
            "run_id": mutation.target.run_id,
            "agent_id": mutation.target.agent_id,
        },
        "runtime_thread_id": mutation.runtime_thread_id,
        "source_binding": mutation.binding,
    })
}

fn phase_slug(phase: WorkflowAgentCallProductPhase) -> &'static str {
    match phase {
        WorkflowAgentCallProductPhase::MaterializeTarget => "materialize_target",
        WorkflowAgentCallProductPhase::CreateRuntime => "create_runtime",
        WorkflowAgentCallProductPhase::ActivateRuntime => "activate_runtime",
        WorkflowAgentCallProductPhase::CommitBinding => "commit_binding",
        WorkflowAgentCallProductPhase::SubmitInput => "submit_input",
    }
}

fn phase_slug_hyphen(phase: WorkflowAgentCallProductPhase) -> &'static str {
    match phase {
        WorkflowAgentCallProductPhase::MaterializeTarget => "materialize-target",
        WorkflowAgentCallProductPhase::CreateRuntime => "create-runtime",
        WorkflowAgentCallProductPhase::ActivateRuntime => "activate-runtime",
        WorkflowAgentCallProductPhase::CommitBinding => "commit-binding",
        WorkflowAgentCallProductPhase::SubmitInput => "submit-input",
    }
}

fn parse_phase(
    phase: &str,
) -> Result<WorkflowAgentCallProductPhase, WorkflowAgentCallProductRepositoryError> {
    match phase {
        "materialize_target" => Ok(WorkflowAgentCallProductPhase::MaterializeTarget),
        "create_runtime" => Ok(WorkflowAgentCallProductPhase::CreateRuntime),
        "activate_runtime" => Ok(WorkflowAgentCallProductPhase::ActivateRuntime),
        "commit_binding" => Ok(WorkflowAgentCallProductPhase::CommitBinding),
        "submit_input" => Ok(WorkflowAgentCallProductPhase::SubmitInput),
        _ => Err(repository_persistence(format!(
            "unknown Workflow AgentCall phase: {phase}"
        ))),
    }
}

fn mailbox_slug(state: Option<&WorkflowAgentCallMailboxState>) -> Option<&'static str> {
    state.map(|state| match state {
        WorkflowAgentCallMailboxState::Queued => "queued",
        WorkflowAgentCallMailboxState::Submitted => "submitted",
    })
}

fn parse_mailbox(
    state: &str,
) -> Result<WorkflowAgentCallMailboxState, WorkflowAgentCallProductRepositoryError> {
    match state {
        "queued" => Ok(WorkflowAgentCallMailboxState::Queued),
        "submitted" => Ok(WorkflowAgentCallMailboxState::Submitted),
        _ => Err(repository_persistence(format!(
            "unknown Workflow AgentCall mailbox state: {state}"
        ))),
    }
}

fn encode<T: serde::Serialize>(
    value: &T,
) -> Result<Value, WorkflowAgentCallProductRepositoryError> {
    serde_json::to_value(value).map_err(|error| repository_persistence(error.to_string()))
}

fn encode_optional<T: serde::Serialize>(
    value: Option<&T>,
) -> Result<Option<Value>, WorkflowAgentCallProductRepositoryError> {
    value.map(encode).transpose()
}

fn decode_repository<T: DeserializeOwned>(
    value: Value,
    context: &str,
) -> Result<T, WorkflowAgentCallProductRepositoryError> {
    serde_json::from_value(value)
        .map_err(|error| repository_persistence(format!("{context} decode failed: {error}")))
}

fn decode<T: DeserializeOwned>(value: Value, context: &str) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| format!("{context} decode failed: {error}"))
}

fn to_i64(value: u64) -> Result<i64, WorkflowAgentCallProductRepositoryError> {
    i64::try_from(value).map_err(|error| repository_persistence(error.to_string()))
}

fn to_i64_string(value: u64) -> Result<i64, String> {
    i64::try_from(value).map_err(|error| error.to_string())
}

fn repository_db(error: sqlx::Error) -> WorkflowAgentCallProductRepositoryError {
    repository_persistence(error.to_string())
}

fn repository_persistence(message: impl Into<String>) -> WorkflowAgentCallProductRepositoryError {
    WorkflowAgentCallProductRepositoryError::Persistence(message.into())
}

fn db_string(error: sqlx::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agentdash_application_agentrun::agent_run::{
        WorkflowAgentCallProductGraphRepository, WorkflowAgentCallProductPhase,
        WorkflowAgentCallProductSaga, WorkflowAgentCallProductSagaRepository,
        WorkflowAgentCallTargetMaterialization,
    };
    use agentdash_application_workflow::{
        WorkflowAgentCallContentBlock, WorkflowAgentCallIdentity, WorkflowAgentCallRequest,
        WorkflowAgentCallTargetIntent,
    };
    use agentdash_domain::agent_run_target::AgentRunTarget;
    use agentdash_domain::channel::{
        Channel, ChannelMedium, ChannelOwner, ChannelRecord, ChannelRegistryMutation,
        ChannelTopology,
    };
    use agentdash_domain::workflow::{
        LifecycleGate, LifecycleRun, LifecycleRunRepository, LifecycleRunWriteError,
        WorkflowExecutorEffectIdentity, WorkflowExecutorEffectRepository,
        WorkflowHumanGateOpenEffect, WorkflowHumanGateResolutionEffect,
    };
    use serde_json::json;
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::PostgresWorkflowAgentCallRepository;
    use crate::persistence::postgres::{
        PostgresWorkflowExecutorEffectRepository, PostgresWorkflowRepository,
    };

    async fn isolated_pool() -> (PgPool, crate::postgres_runtime::PostgresRuntime) {
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/workflow-w8-repository-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "workflow-w8-repository-tests",
            8,
            data_root,
        )
        .await
        .expect("start Workflow W8 repository PostgreSQL");
        let database_name = format!("workflow_w8_repo_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create isolated Workflow repository database");
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
            .expect("connect isolated Workflow repository database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("migrate isolated Workflow repository database");
        (pool, runtime)
    }

    fn agent_call_request(run: &LifecycleRun) -> WorkflowAgentCallRequest {
        WorkflowAgentCallRequest {
            identity: WorkflowAgentCallIdentity {
                request_id: format!("workflow-agent-call:{}", Uuid::new_v4()),
                lifecycle_run_id: run.id,
                orchestration_id: Uuid::new_v4(),
                node_path: "agent-call".to_owned(),
                attempt: 1,
            },
            payload_digest: String::new(),
            project_id: run.project_id,
            created_by_user_id: "workflow-test-user".to_owned(),
            target_intent: WorkflowAgentCallTargetIntent::CreateNew {
                target: AgentRunTarget {
                    run_id: run.id,
                    agent_id: Uuid::new_v4(),
                },
            },
            procedure_key: Some("review".to_owned()),
            procedure_contract: Default::default(),
            input: vec![WorkflowAgentCallContentBlock::Text {
                text: "review".to_owned(),
            }],
        }
        .with_calculated_payload_digest()
    }

    #[tokio::test]
    async fn workflow_w8_repositories_replay_and_conflict_on_real_postgres() {
        let (pool, _runtime) = isolated_pool().await;
        let runs = PostgresWorkflowRepository::new(pool.clone());
        let run = LifecycleRun::new_control_for_user(Uuid::new_v4(), "workflow-test-user");
        runs.create(&run).await.expect("create LifecycleRun");

        let channel = Channel::new(
            ChannelOwner::LifecycleRun { run_id: run.id },
            ChannelMedium::Runtime,
            ChannelTopology::Direct,
        );
        runs.mutate_channel_registry(
            run.id,
            ChannelRegistryMutation::UpsertChannel(ChannelRecord::new(channel.clone())),
        )
        .await
        .expect("mutate independent channel registry");
        let mut next = run.clone();
        next.revision = 1;
        runs.compare_and_swap(0, &next)
            .await
            .expect("LifecycleRun aggregate CAS");
        let stale = runs
            .compare_and_swap(0, &next)
            .await
            .expect_err("stale LifecycleRun CAS");
        assert!(matches!(
            stale,
            LifecycleRunWriteError::RevisionConflict {
                expected_revision: 0,
                actual_revision: 1,
                ..
            }
        ));
        assert_eq!(
            runs.load_channel_registry(run.id)
                .await
                .expect("load channel registry")
                .channels[0]
                .channel
                .id,
            channel.id
        );

        let effects = Arc::new(PostgresWorkflowExecutorEffectRepository::new(pool.clone()));
        let open_identity = WorkflowExecutorEffectIdentity {
            effect_id: format!("workflow-human-gate-open:{}", Uuid::new_v4()),
            lifecycle_run_id: run.id,
            orchestration_id: Uuid::new_v4(),
            node_path: "approval".to_owned(),
            attempt: 1,
        };
        let mut gate = LifecycleGate::open(
            run.id,
            None,
            None,
            "orchestration_human_gate",
            "workflow-test-gate",
            Some(json!({"prompt": "approve"})),
        );
        gate.id = Uuid::new_v4();
        let open = WorkflowHumanGateOpenEffect {
            identity: open_identity,
            payload_digest: "sha256:human-open".to_owned(),
            gate,
        };
        let (opened_left, opened_right) = tokio::join!(
            effects.open_human_gate(open.clone()),
            effects.open_human_gate(open.clone())
        );
        assert_eq!(
            opened_left.expect("open gate left"),
            opened_right.expect("open gate right")
        );
        let resolution = WorkflowHumanGateResolutionEffect {
            identity: WorkflowExecutorEffectIdentity {
                effect_id: format!("workflow-human-gate-resolve:{}", Uuid::new_v4()),
                lifecycle_run_id: run.id,
                orchestration_id: open.identity.orchestration_id,
                node_path: open.identity.node_path.clone(),
                attempt: 1,
            },
            payload_digest: "sha256:human-resolution".to_owned(),
            gate_id: open.gate.id,
            decision: json!({"approved": true}),
            resolved_by: "workflow-test-user".to_owned(),
            outputs: Vec::new(),
        };
        let (resolved_left, resolved_right) = tokio::join!(
            effects.resolve_human_gate(resolution.clone()),
            effects.resolve_human_gate(resolution)
        );
        assert_eq!(
            resolved_left.expect("resolve gate left"),
            resolved_right.expect("resolve gate right")
        );
        let gate_status =
            sqlx::query_scalar::<_, String>("SELECT status FROM lifecycle_gates WHERE id=$1")
                .bind(open.gate.id.to_string())
                .fetch_one(&pool)
                .await
                .expect("load gate");
        assert_eq!(gate_status, "resolved");

        let request = agent_call_request(&run);
        let prepared =
            WorkflowAgentCallProductSaga::prepare(request.clone()).expect("prepare saga value");
        let sagas = PostgresWorkflowAgentCallRepository::new(pool.clone());
        let stored = sagas
            .prepare(prepared.clone())
            .await
            .expect("prepare durable saga");
        let restarted = PostgresWorkflowAgentCallRepository::new(pool.clone());
        assert_eq!(
            restarted
                .prepare(prepared)
                .await
                .expect("replay prepared saga"),
            stored
        );
        let mut dispatched = stored;
        dispatched.in_flight = Some(WorkflowAgentCallProductPhase::MaterializeTarget);
        let dispatched = restarted
            .save(0, dispatched)
            .await
            .expect("persist dispatched phase");
        assert_eq!(dispatched.version, 1);
        let materialization = WorkflowAgentCallTargetMaterialization {
            request_id: request.identity.request_id.clone(),
            payload_digest: request.payload_digest.clone(),
            target: request.target_intent.target().clone(),
            effect_id: format!("workflow-agent-call-materialize:{}", Uuid::new_v4()),
        };
        let committed = restarted
            .materialize_target_idempotent(materialization.clone())
            .await
            .expect("materialize target");
        assert_eq!(
            PostgresWorkflowAgentCallRepository::new(pool.clone())
                .materialize_target_idempotent(materialization)
                .await
                .expect("replay materialization after restart"),
            committed
        );
        sqlx::query("UPDATE lifecycle_agents SET source='unknown' WHERE id=$1")
            .bind(committed.target.agent_id.to_string())
            .execute(&pool)
            .await
            .expect("inject Product graph drift");
        PostgresWorkflowAgentCallRepository::new(pool)
            .materialize_target_idempotent(committed)
            .await
            .expect_err("committed Product graph drift is rejected");
    }
}
