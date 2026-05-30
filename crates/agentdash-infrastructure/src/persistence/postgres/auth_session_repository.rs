use sqlx::{PgPool, Row};

use agentdash_domain::auth_session::{AuthSession, AuthSessionRepository};
use agentdash_domain::common::error::DomainError;

pub struct PostgresAuthSessionRepository {
    pool: PgPool,
}

impl PostgresAuthSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &["auth_sessions"]).await
    }
}

#[async_trait::async_trait]
impl AuthSessionRepository for PostgresAuthSessionRepository {
    async fn upsert_session(&self, session: &AuthSession) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO auth_sessions (
                token_hash,
                identity_json,
                expires_at,
                revoked_at,
                created_at,
                updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT(token_hash) DO UPDATE SET
                identity_json = excluded.identity_json,
                expires_at = excluded.expires_at,
                revoked_at = excluded.revoked_at,
                updated_at = excluded.updated_at",
        )
        .bind(&session.token_hash)
        .bind(&session.identity_json)
        .bind(session.expires_at)
        .bind(session.revoked_at)
        .bind(session.created_at)
        .bind(session.updated_at)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn get_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthSession>, DomainError> {
        let row_opt = sqlx::query(
            "SELECT token_hash, identity_json, expires_at, revoked_at, created_at, updated_at
             FROM auth_sessions WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;

        let Some(row) = row_opt else {
            return Ok(None);
        };

        let revoked_at = row
            .try_get::<Option<i64>, _>("revoked_at")
            .map_err(super::db_err)?;

        Ok(Some(AuthSession {
            token_hash: row.try_get("token_hash").map_err(super::db_err)?,
            identity_json: row.try_get("identity_json").map_err(super::db_err)?,
            expires_at: row.try_get("expires_at").map_err(super::db_err)?,
            revoked_at,
            created_at: row.try_get("created_at").map_err(super::db_err)?,
            updated_at: row.try_get("updated_at").map_err(super::db_err)?,
        }))
    }

    async fn revoke_by_token_hash(
        &self,
        token_hash: &str,
        revoked_at: i64,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE auth_sessions
             SET revoked_at = $1, updated_at = $2
             WHERE token_hash = $3",
        )
        .bind(revoked_at)
        .bind(revoked_at)
        .bind(token_hash)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete_expired_before(&self, epoch_secs: i64) -> Result<u64, DomainError> {
        let result = sqlx::query(
            "DELETE FROM auth_sessions
             WHERE expires_at IS NOT NULL
               AND expires_at <= $1",
        )
        .bind(epoch_secs)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(result.rows_affected())
    }
}
