use agentdash_application_agentrun::agent_run::product_protocol::{
    AgentRunForkRequestId, AgentRunForkSaga, AgentRunForkSagaRepository,
    AgentRunForkSagaRepositoryError, CompanionFreshRepositoryError, CompanionFreshRequestId,
    CompanionFreshSaga, CompanionFreshSagaRepository, PreparedAgentRunForkGraph,
};
use agentdash_application_agentrun::agent_run::{
    AgentRunProductRuntimeRecoveryId, AgentRunProductRuntimeRecoveryRepositoryError,
    AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoverySagaRepository,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;

use super::agent_run_fork_graph_store::insert_agent_run_fork_graph;

#[derive(Clone)]
pub struct PostgresAgentRunForkSagaRepository {
    pool: PgPool,
}

impl PostgresAgentRunForkSagaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentRunForkSagaRepository for PostgresAgentRunForkSagaRepository {
    async fn create(
        &self,
        saga: AgentRunForkSaga,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError> {
        let saga = saga.advance_persisted_version(0)?;
        let value = serde_json::to_value(&saga).map_err(fork_unavailable)?;
        let phase = string_field(&value, "phase").map_err(fork_unavailable)?;
        let durable = value.get("durable_runtime_dispatch").cloned();
        let result = sqlx::query(
            "INSERT INTO agent_run_fork_saga(
                request_id,version,phase,durable_runtime_dispatch,runtime_thread_id,saga
             ) VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(saga.request_id().0)
        .bind(i64_version(saga.version()).map_err(fork_unavailable)?)
        .bind(phase)
        .bind(nullable_json(durable))
        .bind(saga.child().runtime_thread_id.as_str())
        .bind(value)
        .execute(&self.pool)
        .await;
        match result {
            Ok(_) => Ok(saga),
            Err(error) if is_unique(&error) => Err(AgentRunForkSagaRepositoryError::AlreadyExists),
            Err(error) => Err(fork_unavailable(error)),
        }
    }

    async fn load(
        &self,
        request_id: &AgentRunForkRequestId,
    ) -> Result<Option<AgentRunForkSaga>, AgentRunForkSagaRepositoryError> {
        sqlx::query_scalar::<_, Value>("SELECT saga FROM agent_run_fork_saga WHERE request_id=$1")
            .bind(request_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(fork_unavailable)?
            .map(|value| serde_json::from_value(value).map_err(fork_unavailable))
            .transpose()
    }

    async fn list_recoverable(
        &self,
        limit: usize,
    ) -> Result<Vec<AgentRunForkRequestId>, AgentRunForkSagaRepositoryError> {
        let limit = i64::try_from(limit).map_err(fork_unavailable)?;
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT request_id
             FROM agent_run_fork_saga
             WHERE phase <> 'succeeded'
               AND COALESCE(saga->'failed', 'null'::jsonb) = 'null'::jsonb
               AND COALESCE(saga->'lost', 'null'::jsonb) = 'null'::jsonb
             ORDER BY updated_at, request_id
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map(|request_ids| request_ids.into_iter().map(AgentRunForkRequestId).collect())
        .map_err(fork_unavailable)
    }

    async fn save(
        &self,
        expected_version: u64,
        saga: AgentRunForkSaga,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError> {
        let saga = saga.advance_persisted_version(expected_version)?;
        let value = serde_json::to_value(&saga).map_err(fork_unavailable)?;
        let phase = string_field(&value, "phase").map_err(fork_unavailable)?;
        let durable = nullable_json(value.get("durable_runtime_dispatch").cloned());
        let graph_revision = value
            .pointer("/graph_commit/commit_revision")
            .and_then(Value::as_u64)
            .map(i64_version)
            .transpose()
            .map_err(fork_unavailable)?;
        let result = sqlx::query(
            "UPDATE agent_run_fork_saga
             SET version=$3,phase=$4,durable_runtime_dispatch=$5,
                 graph_commit_revision=COALESCE($6,graph_commit_revision),
                 saga=$7,updated_at=NOW()
             WHERE request_id=$1 AND version=$2",
        )
        .bind(saga.request_id().0)
        .bind(i64_version(expected_version).map_err(fork_unavailable)?)
        .bind(i64_version(saga.version()).map_err(fork_unavailable)?)
        .bind(phase)
        .bind(durable)
        .bind(graph_revision)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(fork_unavailable)?;
        if result.rows_affected() == 1 {
            return Ok(saga);
        }
        fork_save_conflict(&self.pool, saga.request_id(), expected_version).await
    }

    async fn commit_product_graph(
        &self,
        expected_version: u64,
        saga: AgentRunForkSaga,
        graph: PreparedAgentRunForkGraph,
    ) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError> {
        graph.validate_for_saga_transition(&saga).map_err(|error| {
            AgentRunForkSagaRepositoryError::InvalidGraphPayload(error.to_string())
        })?;
        let mut tx = self.pool.begin().await.map_err(fork_unavailable)?;
        if let Some(existing_digest) = sqlx::query_scalar::<_, String>(
            "SELECT payload_digest FROM agent_run_fork_graph WHERE request_id=$1",
        )
        .bind(graph.request_id().0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(fork_unavailable)?
        {
            if existing_digest != graph.payload_digest() {
                return Err(AgentRunForkSagaRepositoryError::GraphPayloadConflict);
            }
            tx.rollback().await.map_err(fork_unavailable)?;
            return self
                .load(graph.request_id())
                .await?
                .ok_or(AgentRunForkSagaRepositoryError::NotFound);
        }
        let saga = saga.advance_persisted_version(expected_version)?;
        let graph_revision = saga.version();
        let value = serde_json::to_value(&saga).map_err(fork_unavailable)?;
        let phase = string_field(&value, "phase").map_err(fork_unavailable)?;
        let durable = nullable_json(value.get("durable_runtime_dispatch").cloned());
        let updated = sqlx::query(
            "UPDATE agent_run_fork_saga
             SET version=$3,phase=$4,durable_runtime_dispatch=$5,
                 graph_commit_revision=$3,saga=$6,updated_at=NOW()
             WHERE request_id=$1 AND version=$2",
        )
        .bind(saga.request_id().0)
        .bind(i64_version(expected_version).map_err(fork_unavailable)?)
        .bind(i64_version(graph_revision).map_err(fork_unavailable)?)
        .bind(phase)
        .bind(durable)
        .bind(value)
        .execute(&mut *tx)
        .await
        .map_err(fork_unavailable)?;
        if updated.rows_affected() != 1 {
            tx.rollback().await.map_err(fork_unavailable)?;
            return fork_save_conflict(&self.pool, saga.request_id(), expected_version).await;
        }
        sqlx::query(
            "INSERT INTO agent_run_fork_graph(
                request_id,graph_commit_revision,payload_digest,graph
             ) VALUES ($1,$2,$3,$4)",
        )
        .bind(graph.request_id().0)
        .bind(i64_version(graph_revision).map_err(fork_unavailable)?)
        .bind(graph.payload_digest())
        .bind(serde_json::to_value(&graph).map_err(fork_unavailable)?)
        .execute(&mut *tx)
        .await
        .map_err(fork_unavailable)?;
        let product_graph = graph.graph();
        let persistence_graph = agentdash_application_ports::agent_run_fork::AgentRunForkGraph {
            child_run: product_graph.child_run,
            child_agent: product_graph.child_agent,
            child_frame: product_graph.child_frame,
            lineage: product_graph.lineage,
        };
        insert_agent_run_fork_graph(&mut tx, &persistence_graph)
            .await
            .map_err(fork_unavailable)?;
        tx.commit().await.map_err(fork_unavailable)?;
        Ok(saga)
    }
}

#[derive(Clone)]
pub struct PostgresCompanionFreshSagaRepository {
    pool: PgPool,
}

impl PostgresCompanionFreshSagaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CompanionFreshSagaRepository for PostgresCompanionFreshSagaRepository {
    async fn create(
        &self,
        saga: CompanionFreshSaga,
    ) -> Result<CompanionFreshSaga, CompanionFreshRepositoryError> {
        let saga = saga
            .advance_persisted_version(0)
            .map_err(|error| CompanionFreshRepositoryError::Unavailable(error.to_string()))?;
        let value = serde_json::to_value(&saga).map_err(fresh_unavailable)?;
        let identities = saga.identities();
        let result = sqlx::query(
            "INSERT INTO companion_fresh_saga(
                request_id,version,phase,runtime_thread_id,create_effect_id,
                activation_effect_id,first_input_effect_id,durable_dispatch,
                context_application_evidence,first_input_receipt,saga
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(saga.request_id().0)
        .bind(i64_version(saga.version()).map_err(fresh_unavailable)?)
        .bind(string_field(&value, "phase").map_err(fresh_unavailable)?)
        .bind(saga.runtime_thread_id().as_str())
        .bind(identities.create_effect_id)
        .bind(identities.activation_effect_id)
        .bind(identities.first_input_effect_id)
        .bind(nullable_json(value.get("durable_dispatch").cloned()))
        .bind(nullable_json(value.get("context_evidence").cloned()))
        .bind(nullable_json(
            value.pointer("/receipts/first_input").cloned(),
        ))
        .bind(value)
        .execute(&self.pool)
        .await;
        match result {
            Ok(_) => Ok(saga),
            Err(error) if is_unique(&error) => Err(CompanionFreshRepositoryError::AlreadyExists),
            Err(error) => Err(fresh_unavailable(error)),
        }
    }

    async fn load(
        &self,
        request_id: &CompanionFreshRequestId,
    ) -> Result<Option<CompanionFreshSaga>, CompanionFreshRepositoryError> {
        sqlx::query_scalar::<_, Value>("SELECT saga FROM companion_fresh_saga WHERE request_id=$1")
            .bind(request_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(fresh_unavailable)?
            .map(|value| serde_json::from_value(value).map_err(fresh_unavailable))
            .transpose()
    }

    async fn list_recoverable(
        &self,
        limit: usize,
    ) -> Result<Vec<CompanionFreshRequestId>, CompanionFreshRepositoryError> {
        let limit = i64::try_from(limit).map_err(fresh_unavailable)?;
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT request_id
             FROM companion_fresh_saga
             WHERE phase <> 'succeeded'
               AND COALESCE(saga->'failed', 'null'::jsonb) = 'null'::jsonb
               AND COALESCE(saga->'lost', 'null'::jsonb) = 'null'::jsonb
             ORDER BY updated_at, request_id
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map(|request_ids| {
            request_ids
                .into_iter()
                .map(CompanionFreshRequestId)
                .collect()
        })
        .map_err(fresh_unavailable)
    }

    async fn save(
        &self,
        expected_version: u64,
        saga: CompanionFreshSaga,
    ) -> Result<CompanionFreshSaga, CompanionFreshRepositoryError> {
        let saga = saga
            .advance_persisted_version(expected_version)
            .map_err(|error| CompanionFreshRepositoryError::Unavailable(error.to_string()))?;
        let value = serde_json::to_value(&saga).map_err(fresh_unavailable)?;
        let result = sqlx::query(
            "UPDATE companion_fresh_saga
             SET version=$3,phase=$4,durable_dispatch=$5,
                 context_application_evidence=$6,first_input_receipt=$7,
                 saga=$8,updated_at=NOW()
             WHERE request_id=$1 AND version=$2",
        )
        .bind(saga.request_id().0)
        .bind(i64_version(expected_version).map_err(fresh_unavailable)?)
        .bind(i64_version(saga.version()).map_err(fresh_unavailable)?)
        .bind(string_field(&value, "phase").map_err(fresh_unavailable)?)
        .bind(nullable_json(value.get("durable_dispatch").cloned()))
        .bind(nullable_json(value.get("context_evidence").cloned()))
        .bind(nullable_json(
            value.pointer("/receipts/first_input").cloned(),
        ))
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(fresh_unavailable)?;
        if result.rows_affected() == 1 {
            return Ok(saga);
        }
        let actual = sqlx::query_scalar::<_, i64>(
            "SELECT version FROM companion_fresh_saga WHERE request_id=$1",
        )
        .bind(saga.request_id().0)
        .fetch_optional(&self.pool)
        .await
        .map_err(fresh_unavailable)?;
        match actual {
            None => Err(CompanionFreshRepositoryError::NotFound),
            Some(_) => Err(CompanionFreshRepositoryError::Conflict),
        }
    }
}

#[derive(Clone)]
pub struct PostgresAgentRunProductRuntimeRecoverySagaRepository {
    pool: PgPool,
}

impl PostgresAgentRunProductRuntimeRecoverySagaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentRunProductRuntimeRecoverySagaRepository
    for PostgresAgentRunProductRuntimeRecoverySagaRepository
{
    async fn create(
        &self,
        saga: AgentRunProductRuntimeRecoverySaga,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryRepositoryError>
    {
        let saga = saga
            .advance_persisted_version(0)
            .map_err(runtime_recovery_unavailable)?;
        let value = serde_json::to_value(&saga).map_err(runtime_recovery_unavailable)?;
        let result = sqlx::query(
            "INSERT INTO agent_run_product_runtime_recovery_saga(
                recovery_id,target_run_id,target_agent_id,client_command_id,
                runtime_thread_id,phase,version,saga
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(saga.recovery_id().as_str())
        .bind(saga.target().run_id.to_string())
        .bind(saga.target().agent_id.to_string())
        .bind(saga.client_command_id())
        .bind(saga.runtime_thread_id().as_str())
        .bind(string_field(&value, "phase").map_err(runtime_recovery_unavailable)?)
        .bind(i64_version(saga.version()).map_err(runtime_recovery_unavailable)?)
        .bind(value)
        .execute(&self.pool)
        .await;
        match result {
            Ok(_) => Ok(saga),
            Err(error) if is_unique(&error) => {
                Err(AgentRunProductRuntimeRecoveryRepositoryError::AlreadyExists)
            }
            Err(error) => Err(runtime_recovery_unavailable(error)),
        }
    }

    async fn load(
        &self,
        recovery_id: &AgentRunProductRuntimeRecoveryId,
    ) -> Result<
        Option<AgentRunProductRuntimeRecoverySaga>,
        AgentRunProductRuntimeRecoveryRepositoryError,
    > {
        sqlx::query_scalar::<_, Value>(
            "SELECT saga
             FROM agent_run_product_runtime_recovery_saga
             WHERE recovery_id=$1",
        )
        .bind(recovery_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(runtime_recovery_unavailable)?
        .map(|value| serde_json::from_value(value).map_err(runtime_recovery_unavailable))
        .transpose()
    }

    async fn list_recoverable(
        &self,
        limit: usize,
    ) -> Result<Vec<AgentRunProductRuntimeRecoveryId>, AgentRunProductRuntimeRecoveryRepositoryError>
    {
        let limit = i64::try_from(limit).map_err(runtime_recovery_unavailable)?;
        let values = sqlx::query_scalar::<_, String>(
            "SELECT recovery_id
             FROM agent_run_product_runtime_recovery_saga
             WHERE phase <> 'succeeded'
             ORDER BY updated_at,recovery_id
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(runtime_recovery_unavailable)?;
        values
            .into_iter()
            .map(AgentRunProductRuntimeRecoveryId::from_persisted)
            .collect::<Result<Vec<_>, _>>()
            .map_err(runtime_recovery_unavailable)
    }

    async fn save(
        &self,
        expected_version: u64,
        saga: AgentRunProductRuntimeRecoverySaga,
    ) -> Result<AgentRunProductRuntimeRecoverySaga, AgentRunProductRuntimeRecoveryRepositoryError>
    {
        let saga = saga
            .advance_persisted_version(expected_version)
            .map_err(runtime_recovery_unavailable)?;
        let value = serde_json::to_value(&saga).map_err(runtime_recovery_unavailable)?;
        let result = sqlx::query(
            "UPDATE agent_run_product_runtime_recovery_saga
             SET phase=$3,version=$4,saga=$5,updated_at=NOW()
             WHERE recovery_id=$1 AND version=$2",
        )
        .bind(saga.recovery_id().as_str())
        .bind(i64_version(expected_version).map_err(runtime_recovery_unavailable)?)
        .bind(string_field(&value, "phase").map_err(runtime_recovery_unavailable)?)
        .bind(i64_version(saga.version()).map_err(runtime_recovery_unavailable)?)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(runtime_recovery_unavailable)?;
        if result.rows_affected() == 1 {
            return Ok(saga);
        }
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                SELECT 1 FROM agent_run_product_runtime_recovery_saga WHERE recovery_id=$1
             )",
        )
        .bind(saga.recovery_id().as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(runtime_recovery_unavailable)?;
        if exists {
            Err(AgentRunProductRuntimeRecoveryRepositoryError::Conflict)
        } else {
            Err(AgentRunProductRuntimeRecoveryRepositoryError::NotFound)
        }
    }
}

async fn fork_save_conflict(
    pool: &PgPool,
    request_id: &AgentRunForkRequestId,
    expected: u64,
) -> Result<AgentRunForkSaga, AgentRunForkSagaRepositoryError> {
    let actual =
        sqlx::query_scalar::<_, i64>("SELECT version FROM agent_run_fork_saga WHERE request_id=$1")
            .bind(request_id.0)
            .fetch_optional(pool)
            .await
            .map_err(fork_unavailable)?;
    match actual {
        None => Err(AgentRunForkSagaRepositoryError::NotFound),
        Some(actual) => Err(AgentRunForkSagaRepositoryError::Conflict {
            expected,
            actual: u64::try_from(actual).unwrap_or_default(),
        }),
    }
}

fn nullable_json(value: Option<Value>) -> Option<Value> {
    value.filter(|value| !value.is_null())
}

fn runtime_recovery_unavailable(
    error: impl std::fmt::Display,
) -> AgentRunProductRuntimeRecoveryRepositoryError {
    AgentRunProductRuntimeRecoveryRepositoryError::Unavailable(error.to_string())
}

fn string_field(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("serialized saga omitted {field}"))
}

fn i64_version(value: u64) -> Result<i64, String> {
    i64::try_from(value).map_err(|error| error.to_string())
}

fn is_unique(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code == "23505")
}

fn fork_unavailable(error: impl ToString) -> AgentRunForkSagaRepositoryError {
    AgentRunForkSagaRepositoryError::Unavailable(error.to_string())
}

fn fresh_unavailable(error: impl ToString) -> CompanionFreshRepositoryError {
    CompanionFreshRepositoryError::Unavailable(error.to_string())
}
