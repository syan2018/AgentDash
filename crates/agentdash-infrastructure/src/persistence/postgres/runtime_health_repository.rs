use chrono::{DateTime, Utc};
use sqlx::PgPool;

use agentdash_domain::backend::{
    RuntimeHealth, RuntimeHealthOnlineUpdate, RuntimeHealthRepository, RuntimeHealthStatus,
};
use agentdash_domain::common::error::DomainError;

pub struct PostgresRuntimeHealthRepository {
    pool: PgPool,
}

impl PostgresRuntimeHealthRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &["runtime_health"]).await
    }
}

#[async_trait::async_trait]
impl RuntimeHealthRepository for PostgresRuntimeHealthRepository {
    async fn upsert_online(&self, update: &RuntimeHealthOnlineUpdate) -> Result<(), DomainError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO runtime_health (
                backend_id,
                profile_id,
                name,
                status,
                version,
                capabilities,
                device,
                connected_at,
                last_seen_at,
                disconnected_at,
                disconnect_reason,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, 'online', $4, $5, $6, $7, $7, NULL, NULL, $8, $8)
            ON CONFLICT (backend_id) DO UPDATE SET
                profile_id = EXCLUDED.profile_id,
                name = EXCLUDED.name,
                status = 'online',
                version = EXCLUDED.version,
                capabilities = EXCLUDED.capabilities,
                device = EXCLUDED.device,
                connected_at = EXCLUDED.connected_at,
                last_seen_at = EXCLUDED.last_seen_at,
                disconnected_at = NULL,
                disconnect_reason = NULL,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(&update.backend_id)
        .bind(&update.profile_id)
        .bind(&update.name)
        .bind(&update.version)
        .bind(sqlx::types::Json(&update.capabilities))
        .bind(sqlx::types::Json(&update.device))
        .bind(update.connected_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;

        Ok(())
    }

    async fn update_capabilities(
        &self,
        backend_id: &str,
        capabilities: serde_json::Value,
    ) -> Result<(), DomainError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE runtime_health
            SET capabilities = $2,
                last_seen_at = $3,
                updated_at = $3
            WHERE backend_id = $1
            "#,
        )
        .bind(backend_id)
        .bind(sqlx::types::Json(capabilities))
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn mark_seen(&self, backend_id: &str, seen_at: DateTime<Utc>) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE runtime_health
            SET last_seen_at = $2,
                updated_at = $2
            WHERE backend_id = $1
            "#,
        )
        .bind(backend_id)
        .bind(seen_at)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn mark_offline(
        &self,
        backend_id: &str,
        disconnected_at: DateTime<Utc>,
        reason: Option<String>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE runtime_health
            SET status = 'offline',
                disconnected_at = $2,
                disconnect_reason = $3,
                updated_at = $2
            WHERE backend_id = $1
            "#,
        )
        .bind(backend_id)
        .bind(disconnected_at)
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn get_runtime_health(
        &self,
        backend_id: &str,
    ) -> Result<Option<RuntimeHealth>, DomainError> {
        let row = sqlx::query_as::<_, RuntimeHealthRow>(
            r#"
            SELECT
                backend_id,
                profile_id,
                name,
                status,
                version,
                capabilities,
                device,
                connected_at,
                last_seen_at,
                disconnected_at,
                disconnect_reason,
                created_at,
                updated_at
            FROM runtime_health
            WHERE backend_id = $1
            "#,
        )
        .bind(backend_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_runtime_health(&self) -> Result<Vec<RuntimeHealth>, DomainError> {
        let rows = sqlx::query_as::<_, RuntimeHealthRow>(
            r#"
            SELECT
                backend_id,
                profile_id,
                name,
                status,
                version,
                capabilities,
                device,
                connected_at,
                last_seen_at,
                disconnected_at,
                disconnect_reason,
                created_at,
                updated_at
            FROM runtime_health
            ORDER BY updated_at DESC, backend_id ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }
}

#[derive(sqlx::FromRow)]
struct RuntimeHealthRow {
    backend_id: String,
    profile_id: Option<String>,
    name: String,
    status: String,
    version: Option<String>,
    capabilities: sqlx::types::Json<serde_json::Value>,
    device: sqlx::types::Json<serde_json::Value>,
    connected_at: Option<chrono::DateTime<chrono::Utc>>,
    last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    disconnected_at: Option<chrono::DateTime<chrono::Utc>>,
    disconnect_reason: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<RuntimeHealthRow> for RuntimeHealth {
    type Error = DomainError;

    fn try_from(row: RuntimeHealthRow) -> Result<Self, Self::Error> {
        Ok(Self {
            backend_id: row.backend_id,
            profile_id: row.profile_id,
            name: row.name,
            status: parse_runtime_health_status(&row.status)?,
            version: row.version,
            capabilities: row.capabilities.0,
            device: row.device.0,
            connected_at: row.connected_at,
            last_seen_at: row.last_seen_at,
            disconnected_at: row.disconnected_at,
            disconnect_reason: row.disconnect_reason,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn parse_runtime_health_status(raw: &str) -> Result<RuntimeHealthStatus, DomainError> {
    match raw {
        "online" => Ok(RuntimeHealthStatus::Online),
        "offline" => Ok(RuntimeHealthStatus::Offline),
        "starting" => Ok(RuntimeHealthStatus::Starting),
        "degraded" => Ok(RuntimeHealthStatus::Degraded),
        "stopping" => Ok(RuntimeHealthStatus::Stopping),
        "error" => Ok(RuntimeHealthStatus::Error),
        _ => Err(DomainError::InvalidConfig(format!(
            "runtime_health.status: 未知值 `{raw}`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_health_round_trips_lifecycle() {
        let Some(pool) = crate::persistence::postgres::test_pg_pool("runtime_health").await else {
            return;
        };
        let repo = PostgresRuntimeHealthRepository::new(pool.clone());
        repo.initialize().await.expect("initialize runtime_health");

        let backend_id = format!("runtime-health-{}", uuid::Uuid::new_v4());
        sqlx::query(
            r#"
            INSERT INTO backends (id, name, endpoint, enabled, backend_type)
            VALUES ($1, 'Runtime Health Test', '', TRUE, 'local')
            "#,
        )
        .bind(&backend_id)
        .execute(&pool)
        .await
        .expect("insert backend");

        let connected_at = Utc::now();
        repo.upsert_online(&RuntimeHealthOnlineUpdate {
            backend_id: backend_id.clone(),
            profile_id: Some("desktop".to_string()),
            name: "Desktop Runtime".to_string(),
            version: "0.1.0".to_string(),
            capabilities: serde_json::json!({ "supports_cancel": true }),
            device: serde_json::json!({ "os": "windows" }),
            connected_at,
        })
        .await
        .expect("upsert online");

        let online = repo
            .get_runtime_health(&backend_id)
            .await
            .expect("get health")
            .expect("health exists");
        assert_eq!(online.status, RuntimeHealthStatus::Online);
        assert_eq!(online.profile_id.as_deref(), Some("desktop"));

        repo.update_capabilities(&backend_id, serde_json::json!({ "mcp_servers": [] }))
            .await
            .expect("update capabilities");
        repo.mark_seen(&backend_id, Utc::now())
            .await
            .expect("mark seen");
        repo.mark_offline(&backend_id, Utc::now(), Some("test disconnect".to_string()))
            .await
            .expect("mark offline");

        let offline = repo
            .get_runtime_health(&backend_id)
            .await
            .expect("get offline health")
            .expect("offline health exists");
        assert_eq!(offline.status, RuntimeHealthStatus::Offline);
        assert_eq!(
            offline.disconnect_reason.as_deref(),
            Some("test disconnect")
        );
        assert_eq!(
            offline.capabilities,
            serde_json::json!({ "mcp_servers": [] })
        );

        sqlx::query("DELETE FROM backends WHERE id = $1")
            .bind(&backend_id)
            .execute(&pool)
            .await
            .expect("cleanup backend");
    }
}
