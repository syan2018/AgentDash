use sqlx::{Row, SqlitePool};

use agentdash_domain::auth_session::{AuthSession, AuthSessionRepository};
use agentdash_domain::common::error::DomainError;

pub struct SqliteAuthSessionRepository {
    pool: SqlitePool,
}

impl SqliteAuthSessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS auth_sessions (
                token_hash TEXT PRIMARY KEY,
                identity_json TEXT NOT NULL,
                expires_at INTEGER NULL,
                revoked_at INTEGER NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires_at
             ON auth_sessions(expires_at)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl AuthSessionRepository for SqliteAuthSessionRepository {
    async fn upsert_session(&self, session: &AuthSession) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO auth_sessions (
                token_hash,
                identity_json,
                expires_at,
                revoked_at,
                created_at,
                updated_at
            ) VALUES (?, ?, ?, ?, ?, ?)
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
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn get_by_token_hash(&self, token_hash: &str) -> Result<Option<AuthSession>, DomainError> {
        let row_opt = sqlx::query(
            "SELECT token_hash, identity_json, expires_at, revoked_at, created_at, updated_at
             FROM auth_sessions WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let Some(row) = row_opt else {
            return Ok(None);
        };

        let revoked_at = row
            .try_get::<Option<i64>, _>("revoked_at")
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(Some(AuthSession {
            token_hash: row
                .try_get("token_hash")
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?,
            identity_json: row
                .try_get("identity_json")
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?,
            expires_at: row.try_get("expires_at").ok(),
            revoked_at,
            created_at: row
                .try_get("created_at")
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?,
        }))
    }

    async fn revoke_by_token_hash(
        &self,
        token_hash: &str,
        revoked_at: i64,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE auth_sessions
             SET revoked_at = ?, updated_at = ?
             WHERE token_hash = ?",
        )
        .bind(revoked_at)
        .bind(revoked_at)
        .bind(token_hash)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete_expired_before(&self, epoch_secs: i64) -> Result<u64, DomainError> {
        let result = sqlx::query(
            "DELETE FROM auth_sessions
             WHERE expires_at IS NOT NULL
               AND expires_at <= ?",
        )
        .bind(epoch_secs)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(result.rows_affected())
    }
}
