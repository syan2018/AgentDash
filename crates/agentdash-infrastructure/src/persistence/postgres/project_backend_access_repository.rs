use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use agentdash_domain::backend::{
    BackendWorkspaceInventory, BackendWorkspaceInventoryRepository,
    BackendWorkspaceInventorySource, BackendWorkspaceInventoryStatus, ProjectBackendAccess,
    ProjectBackendAccessMode, ProjectBackendAccessRepository, ProjectBackendAccessStatus,
};
use agentdash_domain::common::error::DomainError;
use agentdash_domain::workspace::WorkspaceIdentityKind;

pub struct PostgresProjectBackendAccessRepository {
    pool: PgPool,
}

impl PostgresProjectBackendAccessRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["project_backend_access", "backend_workspace_inventory"],
        )
        .await
    }
}

#[async_trait::async_trait]
impl ProjectBackendAccessRepository for PostgresProjectBackendAccessRepository {
    async fn create(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
        let root_policy =
            serialize_json(&access.root_policy, "project_backend_access.root_policy")?;
        let capability_policy = serialize_json(
            &access.capability_policy,
            "project_backend_access.capability_policy",
        )?;
        sqlx::query(
            "INSERT INTO project_backend_access
             (id, project_id, backend_id, status, access_mode, priority, root_policy, capability_policy, note, created_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(access.id.to_string())
        .bind(access.project_id.to_string())
        .bind(access.backend_id.trim())
        .bind(access_status_to_str(access.status))
        .bind(access_mode_to_str(access.access_mode))
        .bind(access.priority)
        .bind(root_policy)
        .bind(capability_policy)
        .bind(access.note.as_deref())
        .bind(access.created_by.as_deref())
        .bind(access.created_at)
        .bind(access.updated_at)
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        Ok(())
    }

    async fn update(&self, access: &ProjectBackendAccess) -> Result<(), DomainError> {
        let root_policy =
            serialize_json(&access.root_policy, "project_backend_access.root_policy")?;
        let capability_policy = serialize_json(
            &access.capability_policy,
            "project_backend_access.capability_policy",
        )?;
        let result = sqlx::query(
            "UPDATE project_backend_access
             SET status = $1, access_mode = $2, priority = $3, root_policy = $4, capability_policy = $5, note = $6, updated_at = $7
             WHERE id = $8",
        )
        .bind(access_status_to_str(access.status))
        .bind(access_mode_to_str(access.access_mode))
        .bind(access.priority)
        .bind(root_policy)
        .bind(capability_policy)
        .bind(access.note.as_deref())
        .bind(Utc::now())
        .bind(access.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "project_backend_access",
                id: access.id.to_string(),
            });
        }
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProjectBackendAccess>, DomainError> {
        let row = sqlx::query(
            "SELECT id, project_id, backend_id, status, access_mode, priority, root_policy, capability_policy, note, created_by, created_at, updated_at
             FROM project_backend_access WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;
        row.map(|row| access_from_row(&row)).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, project_id, backend_id, status, access_mode, priority, root_policy, capability_policy, note, created_by, created_at, updated_at
             FROM project_backend_access WHERE project_id = $1
             ORDER BY priority DESC, created_at ASC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        rows.into_iter().map(|row| access_from_row(&row)).collect()
    }

    async fn list_active_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, project_id, backend_id, status, access_mode, priority, root_policy, capability_policy, note, created_by, created_at, updated_at
             FROM project_backend_access WHERE project_id = $1 AND status = 'active'
             ORDER BY priority DESC, created_at ASC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        rows.into_iter().map(|row| access_from_row(&row)).collect()
    }

    async fn get_active_for_project_backend(
        &self,
        project_id: Uuid,
        backend_id: &str,
    ) -> Result<Option<ProjectBackendAccess>, DomainError> {
        let row = sqlx::query(
            "SELECT id, project_id, backend_id, status, access_mode, priority, root_policy, capability_policy, note, created_by, created_at, updated_at
             FROM project_backend_access WHERE project_id = $1 AND backend_id = $2 AND status = 'active'",
        )
        .bind(project_id.to_string())
        .bind(backend_id.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;
        row.map(|row| access_from_row(&row)).transpose()
    }

    async fn list_active_by_backend(
        &self,
        backend_id: &str,
    ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, project_id, backend_id, status, access_mode, priority, root_policy, capability_policy, note, created_by, created_at, updated_at
             FROM project_backend_access WHERE backend_id = $1 AND status = 'active'
             ORDER BY priority DESC, created_at ASC",
        )
        .bind(backend_id.trim())
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        rows.into_iter().map(|row| access_from_row(&row)).collect()
    }

    async fn list_active_by_backends(
        &self,
        backend_ids: &[String],
    ) -> Result<Vec<ProjectBackendAccess>, DomainError> {
        if backend_ids.is_empty() {
            return Ok(Vec::new());
        }
        let trimmed: Vec<String> = backend_ids.iter().map(|id| id.trim().to_string()).collect();
        let rows = sqlx::query(
            "SELECT id, project_id, backend_id, status, access_mode, priority, root_policy, capability_policy, note, created_by, created_at, updated_at
             FROM project_backend_access WHERE backend_id = ANY($1) AND status = 'active'
             ORDER BY backend_id ASC, priority DESC, created_at ASC",
        )
        .bind(&trimmed)
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        rows.into_iter().map(|row| access_from_row(&row)).collect()
    }

    async fn set_status(
        &self,
        id: Uuid,
        status: ProjectBackendAccessStatus,
    ) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE project_backend_access SET status = $1, updated_at = $2 WHERE id = $3",
        )
        .bind(access_status_to_str(status))
        .bind(Utc::now())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(super::db_err)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "project_backend_access",
                id: id.to_string(),
            });
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl BackendWorkspaceInventoryRepository for PostgresProjectBackendAccessRepository {
    async fn upsert(&self, item: &BackendWorkspaceInventory) -> Result<(), DomainError> {
        let items = [item.clone()];
        self.upsert_many(&items).await
    }

    async fn upsert_many(&self, items: &[BackendWorkspaceInventory]) -> Result<(), DomainError> {
        if items.is_empty() {
            return Ok(());
        }
        let prepared = items
            .iter()
            .map(|item| {
                let identity_payload = serialize_json(
                    &item.identity_payload,
                    "backend_workspace_inventory.identity_payload",
                )?;
                let detected_facts = serialize_json(
                    &item.detected_facts,
                    "backend_workspace_inventory.detected_facts",
                )?;
                Ok::<_, DomainError>((item, identity_payload, detected_facts))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO backend_workspace_inventory \
             (id, backend_id, root_ref, identity_kind, identity_payload, detected_facts, status, source, last_seen_at, last_error, created_at, updated_at) ",
        );
        builder.push_values(
            prepared,
            |mut row, (item, identity_payload, detected_facts)| {
                row.push_bind(item.id.to_string())
                    .push_bind(item.backend_id.trim().to_string())
                    .push_bind(item.root_ref.trim().to_string())
                    .push_bind(identity_kind_to_str(&item.identity_kind))
                    .push_bind(identity_payload)
                    .push_bind(detected_facts)
                    .push_bind(inventory_status_to_str(item.status))
                    .push_bind(inventory_source_to_str(item.source))
                    .push_bind(item.last_seen_at)
                    .push_bind(item.last_error.clone())
                    .push_bind(item.created_at)
                    .push_bind(item.updated_at);
            },
        );
        builder.push(
            " ON CONFLICT (backend_id, root_ref) DO UPDATE SET \
                identity_kind = EXCLUDED.identity_kind, \
                identity_payload = EXCLUDED.identity_payload, \
                detected_facts = EXCLUDED.detected_facts, \
                status = EXCLUDED.status, \
                source = EXCLUDED.source, \
                last_seen_at = EXCLUDED.last_seen_at, \
                last_error = EXCLUDED.last_error, \
                updated_at = EXCLUDED.updated_at",
        );
        builder
            .build()
            .execute(&self.pool)
            .await
            .map_err(super::db_err)?;
        Ok(())
    }

    async fn list_by_backend(
        &self,
        backend_id: &str,
    ) -> Result<Vec<BackendWorkspaceInventory>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, backend_id, root_ref, identity_kind, identity_payload, detected_facts, status, source, last_seen_at, last_error, created_at, updated_at
             FROM backend_workspace_inventory WHERE backend_id = $1
             ORDER BY updated_at DESC, root_ref ASC",
        )
        .bind(backend_id.trim())
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        rows.into_iter()
            .map(|row| inventory_from_row(&row))
            .collect()
    }

    async fn list_by_backends(
        &self,
        backend_ids: &[String],
    ) -> Result<Vec<BackendWorkspaceInventory>, DomainError> {
        if backend_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT id, backend_id, root_ref, identity_kind, identity_payload, detected_facts, status, source, last_seen_at, last_error, created_at, updated_at
             FROM backend_workspace_inventory WHERE backend_id = ANY($1)
             ORDER BY backend_id ASC, updated_at DESC, root_ref ASC",
        )
        .bind(backend_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;
        rows.into_iter()
            .map(|row| inventory_from_row(&row))
            .collect()
    }
}

fn access_from_row(row: &sqlx::postgres::PgRow) -> Result<ProjectBackendAccess, DomainError> {
    Ok(ProjectBackendAccess {
        id: parse_uuid(row, "id", "project_backend_access")?,
        project_id: parse_uuid(row, "project_id", "project")?,
        backend_id: string_col(row, "backend_id", "project_backend_access.backend_id")?,
        status: str_to_access_status(&string_col(row, "status", "project_backend_access.status")?)?,
        access_mode: str_to_access_mode(&string_col(
            row,
            "access_mode",
            "project_backend_access.access_mode",
        )?)?,
        priority: row.try_get::<i32, _>("priority").map_err(|error| {
            DomainError::InvalidConfig(format!("project_backend_access.priority: {error}"))
        })?,
        root_policy: parse_json_col(row, "root_policy", "project_backend_access.root_policy")?,
        capability_policy: parse_json_col(
            row,
            "capability_policy",
            "project_backend_access.capability_policy",
        )?,
        note: row.try_get::<Option<String>, _>("note").map_err(|error| {
            DomainError::InvalidConfig(format!("project_backend_access.note: {error}"))
        })?,
        created_by: row
            .try_get::<Option<String>, _>("created_by")
            .map_err(|error| {
                DomainError::InvalidConfig(format!("project_backend_access.created_by: {error}"))
            })?,
        created_at: parse_datetime_col(row, "created_at", "project_backend_access.created_at")?,
        updated_at: parse_datetime_col(row, "updated_at", "project_backend_access.updated_at")?,
    })
}

fn inventory_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<BackendWorkspaceInventory, DomainError> {
    Ok(BackendWorkspaceInventory {
        id: parse_uuid(row, "id", "backend_workspace_inventory")?,
        backend_id: string_col(row, "backend_id", "backend_workspace_inventory.backend_id")?,
        root_ref: string_col(row, "root_ref", "backend_workspace_inventory.root_ref")?,
        identity_kind: str_to_identity_kind(&string_col(
            row,
            "identity_kind",
            "backend_workspace_inventory.identity_kind",
        )?)?,
        identity_payload: parse_json_col(
            row,
            "identity_payload",
            "backend_workspace_inventory.identity_payload",
        )?,
        detected_facts: parse_json_col(
            row,
            "detected_facts",
            "backend_workspace_inventory.detected_facts",
        )?,
        status: str_to_inventory_status(&string_col(
            row,
            "status",
            "backend_workspace_inventory.status",
        )?)?,
        source: str_to_inventory_source(&string_col(
            row,
            "source",
            "backend_workspace_inventory.source",
        )?)?,
        last_seen_at: parse_datetime_col(
            row,
            "last_seen_at",
            "backend_workspace_inventory.last_seen_at",
        )?,
        last_error: row
            .try_get::<Option<String>, _>("last_error")
            .map_err(|error| {
                DomainError::InvalidConfig(format!(
                    "backend_workspace_inventory.last_error: {error}"
                ))
            })?,
        created_at: parse_datetime_col(
            row,
            "created_at",
            "backend_workspace_inventory.created_at",
        )?,
        updated_at: parse_datetime_col(
            row,
            "updated_at",
            "backend_workspace_inventory.updated_at",
        )?,
    })
}

fn serialize_json(value: &Value, field: &str) -> Result<String, DomainError> {
    serde_json::to_string(value)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn parse_json_col(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> Result<Value, DomainError> {
    let raw = string_col(row, column, field)?;
    serde_json::from_str(&raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn parse_uuid(
    row: &sqlx::postgres::PgRow,
    column: &str,
    entity: &'static str,
) -> Result<Uuid, DomainError> {
    let raw = string_col(row, column, column)?;
    Uuid::parse_str(&raw).map_err(|_| DomainError::NotFound { entity, id: raw })
}

fn parse_datetime_col(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> Result<DateTime<Utc>, DomainError> {
    row.try_get::<DateTime<Utc>, _>(column)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn string_col(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> Result<String, DomainError> {
    row.try_get::<String, _>(column)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn access_status_to_str(value: ProjectBackendAccessStatus) -> &'static str {
    value.as_str()
}

fn str_to_access_status(value: &str) -> Result<ProjectBackendAccessStatus, DomainError> {
    match value {
        "active" => Ok(ProjectBackendAccessStatus::Active),
        "paused" => Ok(ProjectBackendAccessStatus::Paused),
        "revoked" => Ok(ProjectBackendAccessStatus::Revoked),
        _ => Err(DomainError::InvalidConfig(format!(
            "project_backend_access.status: 未知值 `{value}`"
        ))),
    }
}

fn access_mode_to_str(value: ProjectBackendAccessMode) -> &'static str {
    value.as_str()
}

fn str_to_access_mode(value: &str) -> Result<ProjectBackendAccessMode, DomainError> {
    match value {
        "explicit_grant" | "use_inventory" => Ok(ProjectBackendAccessMode::ExplicitGrant),
        _ => Err(DomainError::InvalidConfig(format!(
            "project_backend_access.access_mode: 未知值 `{value}`"
        ))),
    }
}

fn inventory_status_to_str(value: BackendWorkspaceInventoryStatus) -> &'static str {
    value.as_str()
}

fn str_to_inventory_status(value: &str) -> Result<BackendWorkspaceInventoryStatus, DomainError> {
    match value {
        "available" => Ok(BackendWorkspaceInventoryStatus::Available),
        "stale" => Ok(BackendWorkspaceInventoryStatus::Stale),
        "offline" => Ok(BackendWorkspaceInventoryStatus::Offline),
        "error" => Ok(BackendWorkspaceInventoryStatus::Error),
        _ => Err(DomainError::InvalidConfig(format!(
            "backend_workspace_inventory.status: 未知值 `{value}`"
        ))),
    }
}

fn inventory_source_to_str(value: BackendWorkspaceInventorySource) -> &'static str {
    value.as_str()
}

fn str_to_inventory_source(value: &str) -> Result<BackendWorkspaceInventorySource, DomainError> {
    match value {
        "manual_register"
        | "runtime_register"
        | "manual_refresh"
        | "scheduled_refresh"
        | "capability_expansion_ack" => Ok(BackendWorkspaceInventorySource::ManualRegister),
        "identity_discovery" => Ok(BackendWorkspaceInventorySource::IdentityDiscovery),
        _ => Err(DomainError::InvalidConfig(format!(
            "backend_workspace_inventory.source: 未知值 `{value}`"
        ))),
    }
}

fn identity_kind_to_str(value: &WorkspaceIdentityKind) -> &'static str {
    match value {
        WorkspaceIdentityKind::GitRepo => "git_repo",
        WorkspaceIdentityKind::P4Workspace => "p4_workspace",
        WorkspaceIdentityKind::LocalDir => "local_dir",
    }
}

fn str_to_identity_kind(value: &str) -> Result<WorkspaceIdentityKind, DomainError> {
    match value {
        "git_repo" => Ok(WorkspaceIdentityKind::GitRepo),
        "p4_workspace" => Ok(WorkspaceIdentityKind::P4Workspace),
        "local_dir" => Ok(WorkspaceIdentityKind::LocalDir),
        _ => Err(DomainError::InvalidConfig(format!(
            "backend_workspace_inventory.identity_kind: 未知值 `{value}`"
        ))),
    }
}
