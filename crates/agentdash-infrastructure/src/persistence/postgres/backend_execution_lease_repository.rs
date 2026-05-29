use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use agentdash_domain::backend::{
    BackendExecutionLease, BackendExecutionLeaseRepository, BackendExecutionLeaseState,
    BackendExecutionSelectionMode, BackendExecutionTerminalKind,
};
use agentdash_domain::common::error::DomainError;

pub struct PostgresBackendExecutionLeaseRepository {
    pool: PgPool,
}

impl PostgresBackendExecutionLeaseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &["backend_execution_leases"])
            .await
    }
}

#[async_trait::async_trait]
impl BackendExecutionLeaseRepository for PostgresBackendExecutionLeaseRepository {
    async fn claim(&self, lease: &BackendExecutionLease) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO backend_execution_leases
             (id, backend_id, session_id, turn_id, executor_id, workspace_id, root_ref, selection_mode, state, claim_reason, terminal_kind, release_reason, claimed_at, activated_at, released_at, last_seen_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'claimed', $9, NULL, NULL, $10, NULL, NULL, $11, $12, $13)",
        )
        .bind(lease.id.to_string())
        .bind(lease.backend_id.trim())
        .bind(lease.session_id.trim())
        .bind(lease.turn_id.trim())
        .bind(lease.executor_id.trim())
        .bind(lease.workspace_id.map(|id| id.to_string()))
        .bind(lease.root_ref.as_deref().map(str::trim))
        .bind(selection_mode_to_str(lease.selection_mode))
        .bind(lease.claim_reason.as_deref())
        .bind(lease.claimed_at.to_rfc3339())
        .bind(lease.last_seen_at.to_rfc3339())
        .bind(lease.created_at.to_rfc3339())
        .bind(lease.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn activate(
        &self,
        lease_id: Uuid,
        activated_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        update_state(
            &self.pool,
            lease_id,
            BackendExecutionLeaseState::Running,
            Some(activated_at),
            None,
            None,
            None,
            activated_at,
        )
        .await
    }

    async fn release(
        &self,
        lease_id: Uuid,
        terminal_kind: Option<BackendExecutionTerminalKind>,
        reason: Option<String>,
        released_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        update_state(
            &self.pool,
            lease_id,
            BackendExecutionLeaseState::Released,
            None,
            Some(released_at),
            terminal_kind,
            reason,
            released_at,
        )
        .await
    }

    async fn fail(
        &self,
        lease_id: Uuid,
        reason: Option<String>,
        failed_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        update_state(
            &self.pool,
            lease_id,
            BackendExecutionLeaseState::Failed,
            None,
            Some(failed_at),
            Some(BackendExecutionTerminalKind::Failed),
            reason,
            failed_at,
        )
        .await
    }

    async fn mark_lost_by_backend(
        &self,
        backend_id: &str,
        reason: Option<String>,
        lost_at: DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        let result = sqlx::query(
            "UPDATE backend_execution_leases
             SET state = 'lost',
                 release_reason = $2,
                 released_at = $3,
                 last_seen_at = $3,
                 updated_at = $3
             WHERE backend_id = $1 AND state IN ('claimed', 'running')",
        )
        .bind(backend_id.trim())
        .bind(reason)
        .bind(lost_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(result.rows_affected())
    }

    async fn get_by_id(
        &self,
        lease_id: Uuid,
    ) -> Result<Option<BackendExecutionLease>, DomainError> {
        let sql = format!("{LEASE_COLUMNS_SQL} WHERE id = $1");
        let row = sqlx::query(&sql)
            .bind(lease_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(super::db_err)?;
        row.map(|row| lease_from_row(&row)).transpose()
    }

    async fn list_active(&self) -> Result<Vec<BackendExecutionLease>, DomainError> {
        let sql = format!(
            "{LEASE_COLUMNS_SQL} WHERE state IN ('claimed', 'running') ORDER BY claimed_at ASC, id ASC"
        );
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(super::db_err)?;
        rows.into_iter().map(|row| lease_from_row(&row)).collect()
    }

    async fn count_active_by_backend(
        &self,
        backend_ids: &[String],
    ) -> Result<HashMap<String, i64>, DomainError> {
        if backend_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query(
            "SELECT backend_id, COUNT(*)::BIGINT AS active_count
             FROM backend_execution_leases
             WHERE backend_id = ANY($1) AND state IN ('claimed', 'running')
             GROUP BY backend_id",
        )
        .bind(backend_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        let mut counts = backend_ids
            .iter()
            .map(|backend_id| (backend_id.clone(), 0_i64))
            .collect::<HashMap<_, _>>();
        for row in rows {
            let backend_id = string_col(&row, "backend_id", "backend_execution_leases.backend_id")?;
            let active_count = row.try_get::<i64, _>("active_count").map_err(|error| {
                DomainError::InvalidConfig(format!(
                    "backend_execution_leases.active_count: {error}"
                ))
            })?;
            counts.insert(backend_id, active_count);
        }
        Ok(counts)
    }
}

const LEASE_COLUMNS_SQL: &str = "SELECT id, backend_id, session_id, turn_id, executor_id, workspace_id, root_ref, selection_mode, state, claim_reason, terminal_kind, release_reason, claimed_at, activated_at, released_at, last_seen_at, created_at, updated_at FROM backend_execution_leases";

async fn update_state(
    pool: &PgPool,
    lease_id: Uuid,
    state: BackendExecutionLeaseState,
    activated_at: Option<DateTime<Utc>>,
    released_at: Option<DateTime<Utc>>,
    terminal_kind: Option<BackendExecutionTerminalKind>,
    release_reason: Option<String>,
    updated_at: DateTime<Utc>,
) -> Result<(), DomainError> {
    let result = sqlx::query(
        "UPDATE backend_execution_leases
         SET state = $1,
             activated_at = COALESCE($2, activated_at),
             released_at = COALESCE($3, released_at),
             terminal_kind = $4,
             release_reason = $5,
             last_seen_at = $6,
             updated_at = $6
         WHERE id = $7",
    )
    .bind(state_to_str(state))
    .bind(activated_at.map(|value| value.to_rfc3339()))
    .bind(released_at.map(|value| value.to_rfc3339()))
    .bind(terminal_kind.map(terminal_kind_to_str))
    .bind(release_reason)
    .bind(updated_at.to_rfc3339())
    .bind(lease_id.to_string())
    .execute(pool)
    .await
    .map_err(super::db_err)?;
    if result.rows_affected() == 0 {
        return Err(DomainError::NotFound {
            entity: "backend_execution_lease",
            id: lease_id.to_string(),
        });
    }
    Ok(())
}

fn lease_from_row(row: &sqlx::postgres::PgRow) -> Result<BackendExecutionLease, DomainError> {
    Ok(BackendExecutionLease {
        id: parse_uuid(row, "id", "backend_execution_leases.id")?,
        backend_id: string_col(row, "backend_id", "backend_execution_leases.backend_id")?,
        session_id: string_col(row, "session_id", "backend_execution_leases.session_id")?,
        turn_id: string_col(row, "turn_id", "backend_execution_leases.turn_id")?,
        executor_id: string_col(row, "executor_id", "backend_execution_leases.executor_id")?,
        workspace_id: optional_uuid_col(
            row,
            "workspace_id",
            "backend_execution_leases.workspace_id",
        )?,
        root_ref: optional_string_col(row, "root_ref", "backend_execution_leases.root_ref")?,
        selection_mode: str_to_selection_mode(&string_col(
            row,
            "selection_mode",
            "backend_execution_leases.selection_mode",
        )?)?,
        state: str_to_state(&string_col(row, "state", "backend_execution_leases.state")?)?,
        claim_reason: optional_string_col(
            row,
            "claim_reason",
            "backend_execution_leases.claim_reason",
        )?,
        terminal_kind: optional_terminal_kind(row)?,
        release_reason: optional_string_col(
            row,
            "release_reason",
            "backend_execution_leases.release_reason",
        )?,
        claimed_at: datetime_col(row, "claimed_at", "backend_execution_leases.claimed_at")?,
        activated_at: optional_datetime_col(
            row,
            "activated_at",
            "backend_execution_leases.activated_at",
        )?,
        released_at: optional_datetime_col(
            row,
            "released_at",
            "backend_execution_leases.released_at",
        )?,
        last_seen_at: datetime_col(row, "last_seen_at", "backend_execution_leases.last_seen_at")?,
        created_at: datetime_col(row, "created_at", "backend_execution_leases.created_at")?,
        updated_at: datetime_col(row, "updated_at", "backend_execution_leases.updated_at")?,
    })
}

fn optional_terminal_kind(
    row: &sqlx::postgres::PgRow,
) -> Result<Option<BackendExecutionTerminalKind>, DomainError> {
    optional_string_col(
        row,
        "terminal_kind",
        "backend_execution_leases.terminal_kind",
    )?
    .as_deref()
    .map(str_to_terminal_kind)
    .transpose()
}

fn selection_mode_to_str(value: BackendExecutionSelectionMode) -> &'static str {
    value.as_str()
}

fn str_to_selection_mode(value: &str) -> Result<BackendExecutionSelectionMode, DomainError> {
    match value {
        "explicit" => Ok(BackendExecutionSelectionMode::Explicit),
        "auto_idle" => Ok(BackendExecutionSelectionMode::AutoIdle),
        "workspace_binding" => Ok(BackendExecutionSelectionMode::WorkspaceBinding),
        _ => Err(DomainError::InvalidConfig(format!(
            "backend_execution_leases.selection_mode: 未知值 `{value}`"
        ))),
    }
}

fn state_to_str(value: BackendExecutionLeaseState) -> &'static str {
    value.as_str()
}

fn str_to_state(value: &str) -> Result<BackendExecutionLeaseState, DomainError> {
    match value {
        "claimed" => Ok(BackendExecutionLeaseState::Claimed),
        "running" => Ok(BackendExecutionLeaseState::Running),
        "released" => Ok(BackendExecutionLeaseState::Released),
        "lost" => Ok(BackendExecutionLeaseState::Lost),
        "failed" => Ok(BackendExecutionLeaseState::Failed),
        _ => Err(DomainError::InvalidConfig(format!(
            "backend_execution_leases.state: 未知值 `{value}`"
        ))),
    }
}

fn terminal_kind_to_str(value: BackendExecutionTerminalKind) -> &'static str {
    value.as_str()
}

fn str_to_terminal_kind(value: &str) -> Result<BackendExecutionTerminalKind, DomainError> {
    match value {
        "completed" => Ok(BackendExecutionTerminalKind::Completed),
        "failed" => Ok(BackendExecutionTerminalKind::Failed),
        "interrupted" => Ok(BackendExecutionTerminalKind::Interrupted),
        _ => Err(DomainError::InvalidConfig(format!(
            "backend_execution_leases.terminal_kind: 未知值 `{value}`"
        ))),
    }
}

fn parse_uuid(row: &sqlx::postgres::PgRow, column: &str, field: &str) -> Result<Uuid, DomainError> {
    let raw = string_col(row, column, field)?;
    Uuid::parse_str(&raw).map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn optional_uuid_col(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> Result<Option<Uuid>, DomainError> {
    optional_string_col(row, column, field)?
        .as_deref()
        .map(|value| {
            Uuid::parse_str(value)
                .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
        })
        .transpose()
}

fn datetime_col(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> Result<DateTime<Utc>, DomainError> {
    let raw = string_col(row, column, field)?;
    super::parse_pg_timestamp_checked(&raw, field)
}

fn optional_datetime_col(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> Result<Option<DateTime<Utc>>, DomainError> {
    optional_string_col(row, column, field)?
        .as_deref()
        .map(|value| super::parse_pg_timestamp_checked(value, field))
        .transpose()
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

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::backend::{BackendType, BackendVisibility};

    #[tokio::test]
    async fn backend_execution_lease_round_trips_lifecycle() {
        let Some(pool) =
            crate::persistence::postgres::test_pg_pool("backend_execution_lease").await
        else {
            return;
        };
        let repo = PostgresBackendExecutionLeaseRepository::new(pool.clone());
        repo.initialize()
            .await
            .expect("initialize backend_execution_leases");

        let backend_id = format!("lease-backend-{}", Uuid::new_v4());
        insert_backend(&pool, &backend_id).await;

        let lease = BackendExecutionLease::claimed(
            backend_id.clone(),
            format!("session-{}", Uuid::new_v4()),
            format!("turn-{}", Uuid::new_v4()),
            "CODEX".to_string(),
            BackendExecutionSelectionMode::AutoIdle,
            Some("auto idle selected".to_string()),
        );
        repo.claim(&lease).await.expect("claim lease");

        let counts = repo
            .count_active_by_backend(std::slice::from_ref(&backend_id))
            .await
            .expect("count active leases");
        assert_eq!(counts.get(&backend_id), Some(&1));

        repo.activate(lease.id, Utc::now())
            .await
            .expect("activate lease");
        let running = repo
            .get_by_id(lease.id)
            .await
            .expect("get running lease")
            .expect("lease exists");
        assert_eq!(running.state, BackendExecutionLeaseState::Running);
        assert!(running.activated_at.is_some());

        repo.release(
            lease.id,
            Some(BackendExecutionTerminalKind::Completed),
            Some("completed".to_string()),
            Utc::now(),
        )
        .await
        .expect("release lease");
        let released = repo
            .get_by_id(lease.id)
            .await
            .expect("get released lease")
            .expect("lease exists");
        assert_eq!(released.state, BackendExecutionLeaseState::Released);
        assert_eq!(
            released.terminal_kind,
            Some(BackendExecutionTerminalKind::Completed)
        );

        let counts = repo
            .count_active_by_backend(std::slice::from_ref(&backend_id))
            .await
            .expect("count released leases");
        assert_eq!(counts.get(&backend_id), Some(&0));
        cleanup_backend(&pool, &backend_id).await;
    }

    #[tokio::test]
    async fn mark_lost_by_backend_only_updates_active_leases() {
        let Some(pool) = crate::persistence::postgres::test_pg_pool("backend_execution_lost").await
        else {
            return;
        };
        let repo = PostgresBackendExecutionLeaseRepository::new(pool.clone());
        let backend_id = format!("lost-backend-{}", Uuid::new_v4());
        insert_backend(&pool, &backend_id).await;

        let active = BackendExecutionLease::claimed(
            backend_id.clone(),
            format!("session-active-{}", Uuid::new_v4()),
            format!("turn-active-{}", Uuid::new_v4()),
            "CODEX".to_string(),
            BackendExecutionSelectionMode::Explicit,
            None,
        );
        let released = BackendExecutionLease::claimed(
            backend_id.clone(),
            format!("session-released-{}", Uuid::new_v4()),
            format!("turn-released-{}", Uuid::new_v4()),
            "CODEX".to_string(),
            BackendExecutionSelectionMode::Explicit,
            None,
        );
        repo.claim(&active).await.expect("claim active");
        repo.claim(&released).await.expect("claim released");
        repo.release(released.id, None, Some("done".to_string()), Utc::now())
            .await
            .expect("release second");

        let affected = repo
            .mark_lost_by_backend(&backend_id, Some("disconnect".to_string()), Utc::now())
            .await
            .expect("mark lost");
        assert_eq!(affected, 1);
        assert_eq!(
            repo.get_by_id(active.id)
                .await
                .expect("get active")
                .expect("active exists")
                .state,
            BackendExecutionLeaseState::Lost
        );
        assert_eq!(
            repo.get_by_id(released.id)
                .await
                .expect("get released")
                .expect("released exists")
                .state,
            BackendExecutionLeaseState::Released
        );
        cleanup_backend(&pool, &backend_id).await;
    }

    async fn insert_backend(pool: &PgPool, backend_id: &str) {
        sqlx::query(
            "INSERT INTO backends
             (id, name, endpoint, enabled, backend_type, visibility, share_scope_kind, capability_slot)
             VALUES ($1, $2, '', TRUE, $3, $4, 'user', 'default')
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(backend_id)
        .bind(format!("Backend {backend_id}"))
        .bind(backend_type_to_str(BackendType::Local))
        .bind(visibility_to_str(BackendVisibility::Private))
        .execute(pool)
        .await
        .expect("insert backend");
    }

    async fn cleanup_backend(pool: &PgPool, backend_id: &str) {
        sqlx::query("DELETE FROM backends WHERE id = $1")
            .bind(backend_id)
            .execute(pool)
            .await
            .expect("cleanup backend");
    }

    fn backend_type_to_str(value: BackendType) -> &'static str {
        match value {
            BackendType::Local => "local",
            BackendType::Remote => "remote",
        }
    }

    fn visibility_to_str(value: BackendVisibility) -> &'static str {
        value.as_str()
    }
}
