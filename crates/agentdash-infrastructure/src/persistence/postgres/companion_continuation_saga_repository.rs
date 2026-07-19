use agentdash_application_agentrun::agent_run::{
    CompanionContinuationRepositoryError, CompanionContinuationSaga,
    CompanionContinuationSagaRepository,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresCompanionContinuationSagaRepository {
    pool: PgPool,
}

impl PostgresCompanionContinuationSagaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CompanionContinuationSagaRepository for PostgresCompanionContinuationSagaRepository {
    async fn create(
        &self,
        saga: CompanionContinuationSaga,
    ) -> Result<CompanionContinuationSaga, CompanionContinuationRepositoryError> {
        let saga = saga.advance_persisted_version(0).map_err(unavailable)?;
        let value = serde_json::to_value(&saga).map_err(unavailable)?;
        let phase = phase(&value)?.to_owned();
        let result = sqlx::query(
            "INSERT INTO companion_continuation_saga(
                request_id,dispatch_id,runtime_protocol_request_id,
                child_run_id,child_agent_id,runtime_thread_id,phase,version,saga
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(saga.request().request_id)
        .bind(&saga.request().dispatch_id)
        .bind(saga.request().runtime_protocol_request_id)
        .bind(saga.request().child_run_id)
        .bind(saga.request().child_agent_id)
        .bind(saga.request().child_runtime_thread_id.as_str())
        .bind(phase)
        .bind(i64_version(saga.version())?)
        .bind(value)
        .execute(&self.pool)
        .await;
        match result {
            Ok(_) => Ok(saga),
            Err(error) if is_unique(&error) => {
                Err(CompanionContinuationRepositoryError::AlreadyExists)
            }
            Err(error) => Err(unavailable(error)),
        }
    }

    async fn load(
        &self,
        request_id: Uuid,
    ) -> Result<Option<CompanionContinuationSaga>, CompanionContinuationRepositoryError> {
        sqlx::query_scalar::<_, Value>(
            "SELECT saga FROM companion_continuation_saga WHERE request_id=$1",
        )
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(unavailable)?
        .map(|value| serde_json::from_value(value).map_err(unavailable))
        .transpose()
    }

    async fn list_recoverable(
        &self,
        limit: usize,
    ) -> Result<Vec<Uuid>, CompanionContinuationRepositoryError> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT request_id
             FROM companion_continuation_saga
             WHERE phase <> 'succeeded'
               AND COALESCE(saga->'failure', 'null'::jsonb) = 'null'::jsonb
             ORDER BY updated_at,request_id
             LIMIT $1",
        )
        .bind(i64::try_from(limit).map_err(unavailable)?)
        .fetch_all(&self.pool)
        .await
        .map_err(unavailable)
    }

    async fn save(
        &self,
        expected_version: u64,
        saga: CompanionContinuationSaga,
    ) -> Result<CompanionContinuationSaga, CompanionContinuationRepositoryError> {
        let saga = saga
            .advance_persisted_version(expected_version)
            .map_err(unavailable)?;
        let value = serde_json::to_value(&saga).map_err(unavailable)?;
        let phase = phase(&value)?.to_owned();
        let result = sqlx::query(
            "UPDATE companion_continuation_saga
             SET phase=$3,version=$4,saga=$5,updated_at=NOW()
             WHERE request_id=$1 AND version=$2",
        )
        .bind(saga.request().request_id)
        .bind(i64_version(expected_version)?)
        .bind(phase)
        .bind(i64_version(saga.version())?)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(unavailable)?;
        if result.rows_affected() == 1 {
            return Ok(saga);
        }
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                SELECT 1 FROM companion_continuation_saga WHERE request_id=$1
             )",
        )
        .bind(saga.request().request_id)
        .fetch_one(&self.pool)
        .await
        .map_err(unavailable)?;
        if exists {
            Err(CompanionContinuationRepositoryError::Conflict)
        } else {
            Err(CompanionContinuationRepositoryError::NotFound)
        }
    }
}

fn phase(value: &Value) -> Result<&str, CompanionContinuationRepositoryError> {
    value
        .get("phase")
        .and_then(Value::as_str)
        .ok_or_else(|| unavailable("Companion continuation phase is missing"))
}

fn i64_version(value: u64) -> Result<i64, CompanionContinuationRepositoryError> {
    i64::try_from(value).map_err(unavailable)
}

fn unavailable(error: impl std::fmt::Display) -> CompanionContinuationRepositoryError {
    CompanionContinuationRepositoryError::Unavailable(error.to_string())
}

fn is_unique(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(database) if database.code().as_deref() == Some("23505")
    )
}
