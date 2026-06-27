use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use agentdash_domain::backend::{RunnerRegistrationToken, RunnerRegistrationTokenRepository};
use agentdash_domain::common::error::DomainError;

pub struct PostgresRunnerRegistrationTokenRepository {
    pool: PgPool,
}

impl PostgresRunnerRegistrationTokenRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &["runner_registration_tokens"])
            .await
    }
}

#[async_trait::async_trait]
impl RunnerRegistrationTokenRepository for PostgresRunnerRegistrationTokenRepository {
    async fn create(&self, token: &RunnerRegistrationToken) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO runner_registration_tokens
             (id, project_id, name, token_secret_hash, token_prefix, created_by_user_id,
              expires_at, revoked_at, last_used_at, last_claimed_backend_id,
              default_capability_slot, machine_policy, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(token.id.trim())
        .bind(token.project_id.to_string())
        .bind(token.name.trim())
        .bind(token.token_secret_hash.trim())
        .bind(token.token_prefix.trim())
        .bind(token.created_by_user_id.trim())
        .bind(token.expires_at)
        .bind(token.revoked_at)
        .bind(token.last_used_at)
        .bind(token.last_claimed_backend_id.as_deref())
        .bind(token.default_capability_slot.trim())
        .bind(&token.machine_policy)
        .bind(token.created_at)
        .bind(token.updated_at)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn update(&self, token: &RunnerRegistrationToken) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE runner_registration_tokens
             SET name = $2,
                 token_secret_hash = $3,
                 token_prefix = $4,
                 expires_at = $5,
                 revoked_at = $6,
                 last_used_at = $7,
                 last_claimed_backend_id = $8,
                 default_capability_slot = $9,
                 machine_policy = $10,
                 updated_at = $11
             WHERE id = $1",
        )
        .bind(token.id.trim())
        .bind(token.name.trim())
        .bind(token.token_secret_hash.trim())
        .bind(token.token_prefix.trim())
        .bind(token.expires_at)
        .bind(token.revoked_at)
        .bind(token.last_used_at)
        .bind(token.last_claimed_backend_id.as_deref())
        .bind(token.default_capability_slot.trim())
        .bind(&token.machine_policy)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "runner_registration_token",
                id: token.id.clone(),
            });
        }
        Ok(())
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<RunnerRegistrationToken>, DomainError> {
        let sql = select_token_sql("WHERE id = $1");
        let row = sqlx::query(&sql)
            .bind(id.trim())
            .fetch_optional(&self.pool)
            .await
            .map_err(super::db_err)?;
        row.map(|row| token_from_row(&row)).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<RunnerRegistrationToken>, DomainError> {
        let sql = select_token_sql("WHERE project_id = $1 ORDER BY created_at DESC, id ASC");
        let row = sqlx::query(&sql)
            .bind(project_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(super::db_err)?;
        row.into_iter().map(|row| token_from_row(&row)).collect()
    }

    async fn revoke(&self, id: &str, revoked_at: DateTime<Utc>) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE runner_registration_tokens
             SET revoked_at = COALESCE(revoked_at, $2), updated_at = $2
             WHERE id = $1",
        )
        .bind(id.trim())
        .bind(revoked_at)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "runner_registration_token",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn record_usage(
        &self,
        id: &str,
        backend_id: &str,
        used_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE runner_registration_tokens
             SET last_used_at = $2,
                 last_claimed_backend_id = $3,
                 updated_at = $2
             WHERE id = $1",
        )
        .bind(id.trim())
        .bind(used_at)
        .bind(backend_id.trim())
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "runner_registration_token",
                id: id.to_string(),
            });
        }
        Ok(())
    }
}

fn select_token_sql(suffix: &str) -> String {
    format!(
        "SELECT id, project_id, name, token_secret_hash, token_prefix, created_by_user_id,
                expires_at, revoked_at, last_used_at, last_claimed_backend_id,
                default_capability_slot, COALESCE(machine_policy, '{{}}'::jsonb) AS machine_policy,
                created_at, updated_at
         FROM runner_registration_tokens {suffix}"
    )
}

fn token_from_row(row: &sqlx::postgres::PgRow) -> Result<RunnerRegistrationToken, DomainError> {
    let project_id_raw = string_col(row, "project_id", "runner_registration_tokens.project_id")?;
    Ok(RunnerRegistrationToken {
        id: string_col(row, "id", "runner_registration_tokens.id")?,
        project_id: Uuid::parse_str(&project_id_raw).map_err(|error| {
            DomainError::InvalidConfig(format!("runner_registration_tokens.project_id: {error}"))
        })?,
        name: string_col(row, "name", "runner_registration_tokens.name")?,
        token_secret_hash: string_col(
            row,
            "token_secret_hash",
            "runner_registration_tokens.token_secret_hash",
        )?,
        token_prefix: string_col(
            row,
            "token_prefix",
            "runner_registration_tokens.token_prefix",
        )?,
        created_by_user_id: string_col(
            row,
            "created_by_user_id",
            "runner_registration_tokens.created_by_user_id",
        )?,
        expires_at: datetime_col(row, "expires_at", "runner_registration_tokens.expires_at")?,
        revoked_at: optional_datetime_col(
            row,
            "revoked_at",
            "runner_registration_tokens.revoked_at",
        )?,
        last_used_at: optional_datetime_col(
            row,
            "last_used_at",
            "runner_registration_tokens.last_used_at",
        )?,
        last_claimed_backend_id: optional_string_col(
            row,
            "last_claimed_backend_id",
            "runner_registration_tokens.last_claimed_backend_id",
        )?,
        default_capability_slot: string_col(
            row,
            "default_capability_slot",
            "runner_registration_tokens.default_capability_slot",
        )?,
        machine_policy: json_col(
            row,
            "machine_policy",
            "runner_registration_tokens.machine_policy",
        )?,
        created_at: datetime_col(row, "created_at", "runner_registration_tokens.created_at")?,
        updated_at: datetime_col(row, "updated_at", "runner_registration_tokens.updated_at")?,
    })
}

fn string_col(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> Result<String, DomainError> {
    row.try_get::<String, _>(column)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn optional_string_col(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> Result<Option<String>, DomainError> {
    row.try_get::<Option<String>, _>(column)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn datetime_col(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> Result<DateTime<Utc>, DomainError> {
    row.try_get::<DateTime<Utc>, _>(column)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn optional_datetime_col(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> Result<Option<DateTime<Utc>>, DomainError> {
    row.try_get::<Option<DateTime<Utc>>, _>(column)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn json_col(row: &sqlx::postgres::PgRow, column: &str, field: &str) -> Result<Value, DomainError> {
    row.try_get::<Value, _>(column)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}
