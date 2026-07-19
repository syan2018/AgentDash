use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresWorkflowRecoveryRepository {
    pool: PgPool,
}

impl PostgresWorkflowRecoveryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_recoverable_run_ids(&self, limit: usize) -> Result<Vec<Uuid>, String> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM lifecycle_runs
             WHERE status IN ('ready','running')
             ORDER BY last_activity_at,id
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        ids.into_iter()
            .map(|id| Uuid::parse_str(&id).map_err(|error| error.to_string()))
            .collect()
    }
}
