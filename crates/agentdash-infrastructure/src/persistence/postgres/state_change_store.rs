use sqlx::{PgPool, Postgres, Transaction};

use agentdash_domain::common::error::DomainError;
use agentdash_domain::story::{ChangeKind, StateChange};

pub async fn initialize_state_changes_schema(pool: &PgPool) -> Result<(), DomainError> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS state_changes (
            id BIGSERIAL PRIMARY KEY,
            project_id TEXT NOT NULL DEFAULT '',
            entity_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            payload TEXT NOT NULL DEFAULT '{}',
            backend_id TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_state_changes_entity ON state_changes(entity_id);
        CREATE INDEX IF NOT EXISTS idx_state_changes_backend ON state_changes(backend_id);
        CREATE INDEX IF NOT EXISTS idx_state_changes_project ON state_changes(project_id);
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    Ok(())
}

pub async fn append_state_change(
    pool: &PgPool,
    project_id: uuid::Uuid,
    entity_id: uuid::Uuid,
    kind: ChangeKind,
    payload: serde_json::Value,
    backend_id: Option<&str>,
) -> Result<(), DomainError> {
    sqlx::query(
        "INSERT INTO state_changes (project_id, entity_id, kind, payload, backend_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(project_id.to_string())
    .bind(entity_id.to_string())
    .bind(kind_to_db_value(&kind)?)
    .bind(payload.to_string())
    .bind(backend_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    Ok(())
}

pub async fn append_state_change_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    project_id: uuid::Uuid,
    entity_id: uuid::Uuid,
    kind: ChangeKind,
    payload: serde_json::Value,
    backend_id: Option<&str>,
) -> Result<(), DomainError> {
    sqlx::query(
        "INSERT INTO state_changes (project_id, entity_id, kind, payload, backend_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(project_id.to_string())
    .bind(entity_id.to_string())
    .bind(kind_to_db_value(&kind)?)
    .bind(payload.to_string())
    .bind(backend_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&mut **tx)
    .await
    .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    Ok(())
}

pub async fn get_state_changes_since(
    pool: &PgPool,
    since_id: i64,
    limit: i64,
) -> Result<Vec<StateChange>, DomainError> {
    let rows = sqlx::query_as::<_, StateChangeRow>(
        "SELECT id, project_id, entity_id, kind, payload, backend_id, created_at
         FROM state_changes WHERE id > $1 ORDER BY id ASC LIMIT $2",
    )
    .bind(since_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    rows.into_iter().map(TryInto::try_into).collect()
}

pub async fn get_state_changes_since_by_project(
    pool: &PgPool,
    project_id: uuid::Uuid,
    since_id: i64,
    limit: i64,
) -> Result<Vec<StateChange>, DomainError> {
    let rows = sqlx::query_as::<_, StateChangeRow>(
        "SELECT id, project_id, entity_id, kind, payload, backend_id, created_at
         FROM state_changes
         WHERE project_id = $1 AND id > $2
         ORDER BY id ASC
         LIMIT $3",
    )
    .bind(project_id.to_string())
    .bind(since_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

    rows.into_iter().map(TryInto::try_into).collect()
}

pub async fn latest_state_change_id(pool: &PgPool) -> Result<i64, DomainError> {
    let row: (i64,) = sqlx::query_as("SELECT COALESCE(MAX(id), 0) FROM state_changes")
        .fetch_one(pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
    Ok(row.0)
}

pub async fn latest_state_change_id_by_project(
    pool: &PgPool,
    project_id: uuid::Uuid,
) -> Result<i64, DomainError> {
    let row: (i64,) =
        sqlx::query_as("SELECT COALESCE(MAX(id), 0) FROM state_changes WHERE project_id = $1")
            .bind(project_id.to_string())
            .fetch_one(pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
    Ok(row.0)
}

fn kind_to_db_value(kind: &ChangeKind) -> Result<String, DomainError> {
    Ok(serde_json::to_string(kind)?.trim_matches('"').to_string())
}

#[derive(sqlx::FromRow)]
struct StateChangeRow {
    id: i64,
    project_id: String,
    entity_id: String,
    kind: String,
    payload: String,
    backend_id: Option<String>,
    created_at: String,
}

impl TryFrom<StateChangeRow> for StateChange {
    type Error = DomainError;

    fn try_from(row: StateChangeRow) -> Result<Self, Self::Error> {
        Ok(StateChange {
            id: row.id,
            project_id: row.project_id.parse().map_err(|_| DomainError::NotFound {
                entity: "project",
                id: row.project_id.clone(),
            })?,
            entity_id: row.entity_id.parse().map_err(|_| DomainError::NotFound {
                entity: "state_change",
                id: row.entity_id.clone(),
            })?,
            kind: parse_change_kind(&row.kind)?,
            payload: parse_json_payload(&row.payload)?,
            backend_id: row.backend_id.ok_or_else(|| {
                DomainError::InvalidConfig("state_changes.backend_id 缺失".to_string())
            })?,
            created_at: super::parse_pg_timestamp_checked(
                &row.created_at,
                "state_changes.created_at",
            )?,
        })
    }
}

fn parse_change_kind(raw: &str) -> Result<ChangeKind, DomainError> {
    match raw {
        "story_created" => Ok(ChangeKind::StoryCreated),
        "story_updated" => Ok(ChangeKind::StoryUpdated),
        "story_status_changed" => Ok(ChangeKind::StoryStatusChanged),
        "story_deleted" => Ok(ChangeKind::StoryDeleted),
        "task_created" => Ok(ChangeKind::TaskCreated),
        "task_updated" => Ok(ChangeKind::TaskUpdated),
        "task_status_changed" => Ok(ChangeKind::TaskStatusChanged),
        "task_deleted" => Ok(ChangeKind::TaskDeleted),
        "task_artifact_added" => Ok(ChangeKind::TaskArtifactAdded),
        _ => Err(DomainError::InvalidConfig(format!(
            "state_changes.kind: 未知值 `{raw}`"
        ))),
    }
}

fn parse_json_payload(raw: &str) -> Result<serde_json::Value, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("state_changes.payload: {error}")))
}
