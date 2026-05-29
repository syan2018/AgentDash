use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::workspace::{
    Workspace, WorkspaceBinding, WorkspaceBindingStatus, WorkspaceIdentityKind,
    WorkspaceRepository, WorkspaceResolutionPolicy, WorkspaceStatus,
};

pub struct PostgresWorkspaceRepository {
    pool: PgPool,
}

impl PostgresWorkspaceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["workspaces", "workspace_bindings"],
        )
        .await
    }

    async fn save_bindings_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        workspace_id: Uuid,
        bindings: &[WorkspaceBinding],
    ) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM workspace_bindings WHERE workspace_id = $1")
            .bind(workspace_id.to_string())
            .execute(&mut **tx)
            .await
            .map_err(super::db_err)?;

        if bindings.is_empty() {
            return Ok(());
        }
        let workspace_id_str = workspace_id.to_string();
        let prepared = bindings
            .iter()
            .map(|binding| {
                let detected_facts =
                    serde_json::to_string(&binding.detected_facts).map_err(|error| {
                        DomainError::InvalidConfig(format!(
                            "序列化 workspace binding 失败: {error}"
                        ))
                    })?;
                Ok::<_, DomainError>((binding, detected_facts))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO workspace_bindings (id, workspace_id, backend_id, root_ref, status, detected_facts, last_verified_at, priority, created_at, updated_at) ",
        );
        builder.push_values(prepared, |mut row, (binding, detected_facts)| {
            row.push_bind(binding.id.to_string())
                .push_bind(&workspace_id_str)
                .push_bind(binding.backend_id.trim().to_string())
                .push_bind(binding.root_ref.trim().to_string())
                .push_bind(binding_status_to_str(&binding.status))
                .push_bind(detected_facts)
                .push_bind(binding.last_verified_at.map(|value| value.to_rfc3339()))
                .push_bind(binding.priority)
                .push_bind(binding.created_at.to_rfc3339())
                .push_bind(binding.updated_at.to_rfc3339());
        });
        builder
            .build()
            .execute(&mut **tx)
            .await
            .map_err(super::db_err)?;

        Ok(())
    }

    async fn load_bindings(
        &self,
        workspace_id: Uuid,
    ) -> Result<Vec<WorkspaceBinding>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, workspace_id, backend_id, root_ref, status, detected_facts, last_verified_at, priority, created_at, updated_at
             FROM workspace_bindings WHERE workspace_id = $1 ORDER BY priority DESC, created_at ASC",
        )
        .bind(workspace_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        rows.into_iter()
            .map(|row| workspace_binding_from_row(&row))
            .collect()
    }
}

#[async_trait::async_trait]
impl WorkspaceRepository for PostgresWorkspaceRepository {
    async fn create(&self, workspace: &Workspace) -> Result<(), DomainError> {
        let payload = serde_json::to_string(&workspace.identity_payload)
            .map_err(DomainError::Serialization)?;
        let mut tx = self.pool.begin().await.map_err(super::db_err)?;

        let mount_caps = serde_json::to_string(&workspace.mount_capabilities)
            .map_err(DomainError::Serialization)?;
        sqlx::query(
            "INSERT INTO workspaces (id, project_id, name, identity_kind, identity_payload, resolution_policy, default_binding_id, status, mount_capabilities, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(workspace.id.to_string())
        .bind(workspace.project_id.to_string())
        .bind(workspace.name.trim())
        .bind(identity_kind_to_str(&workspace.identity_kind))
        .bind(payload)
        .bind(resolution_policy_to_str(&workspace.resolution_policy))
        .bind(workspace.default_binding_id.map(|id| id.to_string()))
        .bind(workspace_status_to_str(&workspace.status))
        .bind(mount_caps)
        .bind(workspace.created_at.to_rfc3339())
        .bind(workspace.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(super::db_err)?;

        Self::save_bindings_in_tx(&mut tx, workspace.id, &workspace.bindings).await?;
        tx.commit().await.map_err(super::db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<Workspace>, DomainError> {
        let row = sqlx::query(
            "SELECT id, project_id, name, identity_kind, identity_payload, resolution_policy, default_binding_id, status, mount_capabilities, created_at, updated_at
             FROM workspaces WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(super::db_err)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let mut workspace = workspace_from_row(&row)?;
        workspace.bindings = self.load_bindings(workspace.id).await?;
        workspace.refresh_default_binding();
        Ok(Some(workspace))
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<Workspace>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, project_id, name, identity_kind, identity_payload, resolution_policy, default_binding_id, status, mount_capabilities, created_at, updated_at
             FROM workspaces WHERE project_id = $1 ORDER BY created_at DESC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        let mut workspaces = rows
            .iter()
            .map(workspace_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if workspaces.is_empty() {
            return Ok(workspaces);
        }

        let workspace_ids: Vec<String> = workspaces.iter().map(|w| w.id.to_string()).collect();
        let binding_rows = sqlx::query(
            "SELECT id, workspace_id, backend_id, root_ref, status, detected_facts, last_verified_at, priority, created_at, updated_at
             FROM workspace_bindings WHERE workspace_id = ANY($1) ORDER BY priority DESC, created_at ASC",
        )
        .bind(&workspace_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(super::db_err)?;

        let mut bindings_by_workspace: std::collections::HashMap<Uuid, Vec<WorkspaceBinding>> =
            std::collections::HashMap::with_capacity(workspaces.len());
        for row in binding_rows {
            let binding = workspace_binding_from_row(&row)?;
            bindings_by_workspace
                .entry(binding.workspace_id)
                .or_default()
                .push(binding);
        }

        for workspace in workspaces.iter_mut() {
            workspace.bindings = bindings_by_workspace
                .remove(&workspace.id)
                .unwrap_or_default();
            workspace.refresh_default_binding();
        }
        Ok(workspaces)
    }

    async fn update(&self, workspace: &Workspace) -> Result<(), DomainError> {
        let payload = serde_json::to_string(&workspace.identity_payload)
            .map_err(DomainError::Serialization)?;
        let mut tx = self.pool.begin().await.map_err(super::db_err)?;

        let mount_caps = serde_json::to_string(&workspace.mount_capabilities)
            .map_err(DomainError::Serialization)?;
        let result = sqlx::query(
            "UPDATE workspaces
             SET name = $1, identity_kind = $2, identity_payload = $3, resolution_policy = $4, default_binding_id = $5, status = $6, mount_capabilities = $7, updated_at = $8
             WHERE id = $9",
        )
        .bind(workspace.name.trim())
        .bind(identity_kind_to_str(&workspace.identity_kind))
        .bind(payload)
        .bind(resolution_policy_to_str(&workspace.resolution_policy))
        .bind(workspace.default_binding_id.map(|id| id.to_string()))
        .bind(workspace_status_to_str(&workspace.status))
        .bind(mount_caps)
        .bind(Utc::now().to_rfc3339())
        .bind(workspace.id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(super::db_err)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "workspace",
                id: workspace.id.to_string(),
            });
        }

        Self::save_bindings_in_tx(&mut tx, workspace.id, &workspace.bindings).await?;
        tx.commit().await.map_err(super::db_err)?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(super::db_err)?;
        sqlx::query("DELETE FROM workspace_bindings WHERE workspace_id = $1")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(super::db_err)?;

        let result = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(super::db_err)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "workspace",
                id: id.to_string(),
            });
        }
        tx.commit().await.map_err(super::db_err)?;
        Ok(())
    }
}

fn workspace_from_row(row: &sqlx::postgres::PgRow) -> Result<Workspace, DomainError> {
    let id = parse_uuid(
        row.try_get::<String, _>("id").map_err(super::db_err)?,
        "workspace",
    )?;
    let project_id = parse_uuid(
        row.try_get::<String, _>("project_id")
            .map_err(super::db_err)?,
        "project",
    )?;
    let name = row
        .try_get::<String, _>("name")
        .map_err(|e| DomainError::InvalidConfig(format!("workspaces.name: {e}")))?;
    let identity_kind = row
        .try_get::<String, _>("identity_kind")
        .map_err(|e| DomainError::InvalidConfig(format!("workspaces.identity_kind: {e}")))?;
    let identity_payload_raw = row
        .try_get::<String, _>("identity_payload")
        .map_err(|e| DomainError::InvalidConfig(format!("workspaces.identity_payload: {e}")))?;
    let resolution_policy = row
        .try_get::<String, _>("resolution_policy")
        .map_err(|e| DomainError::InvalidConfig(format!("workspaces.resolution_policy: {e}")))?;
    let status = row
        .try_get::<String, _>("status")
        .map_err(|e| DomainError::InvalidConfig(format!("workspaces.status: {e}")))?;
    let created_at = row
        .try_get::<String, _>("created_at")
        .map_err(|e| DomainError::InvalidConfig(format!("workspaces.created_at: {e}")))?;
    let updated_at = row
        .try_get::<String, _>("updated_at")
        .map_err(|e| DomainError::InvalidConfig(format!("workspaces.updated_at: {e}")))?;
    let default_binding_id = row
        .try_get::<Option<String>, _>("default_binding_id")
        .map_err(|e| DomainError::InvalidConfig(format!("workspaces.default_binding_id: {e}")))?
        .map(|value| {
            Uuid::parse_str(&value).map_err(|error| {
                DomainError::InvalidConfig(format!("workspaces.default_binding_id: {error}"))
            })
        })
        .transpose()?;

    let mount_capabilities_raw = row
        .try_get::<String, _>("mount_capabilities")
        .map_err(|e| DomainError::InvalidConfig(format!("workspaces.mount_capabilities: {e}")))?;
    let mount_capabilities: Vec<agentdash_domain::common::MountCapability> =
        serde_json::from_str(&mount_capabilities_raw).map_err(|e| {
            DomainError::InvalidConfig(format!("workspaces.mount_capabilities JSON: {e}"))
        })?;

    Ok(Workspace {
        id,
        project_id,
        name,
        identity_kind: str_to_identity_kind(&identity_kind)?,
        identity_payload: parse_json_value(&identity_payload_raw, "workspaces.identity_payload")?,
        resolution_policy: str_to_resolution_policy(&resolution_policy)?,
        default_binding_id,
        status: str_to_workspace_status(&status)?,
        bindings: Vec::new(),
        mount_capabilities,
        created_at: parse_datetime(&created_at, "workspaces.created_at")?,
        updated_at: parse_datetime(&updated_at, "workspaces.updated_at")?,
    })
}

fn workspace_binding_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<WorkspaceBinding, DomainError> {
    Ok(WorkspaceBinding {
        id: parse_uuid(
            row.try_get::<String, _>("id").map_err(super::db_err)?,
            "workspace_binding",
        )?,
        workspace_id: parse_uuid(
            row.try_get::<String, _>("workspace_id")
                .map_err(super::db_err)?,
            "workspace",
        )?,
        backend_id: row.try_get::<String, _>("backend_id").map_err(|e| {
            DomainError::InvalidConfig(format!("workspace_bindings.backend_id: {e}"))
        })?,
        root_ref: row
            .try_get::<String, _>("root_ref")
            .map_err(|e| DomainError::InvalidConfig(format!("workspace_bindings.root_ref: {e}")))?,
        status: str_to_binding_status(
            &row.try_get::<String, _>("status").map_err(|e| {
                DomainError::InvalidConfig(format!("workspace_bindings.status: {e}"))
            })?,
        )?,
        detected_facts: parse_json_value(
            &row.try_get::<String, _>("detected_facts").map_err(|e| {
                DomainError::InvalidConfig(format!("workspace_bindings.detected_facts: {e}"))
            })?,
            "workspace_bindings.detected_facts",
        )?,
        last_verified_at: row
            .try_get::<Option<String>, _>("last_verified_at")
            .map_err(|e| {
                DomainError::InvalidConfig(format!("workspace_bindings.last_verified_at: {e}"))
            })?
            .map(|value| parse_datetime(&value, "workspace_bindings.last_verified_at"))
            .transpose()?,
        priority: row
            .try_get::<i32, _>("priority")
            .map_err(|e| DomainError::InvalidConfig(format!("workspace_bindings.priority: {e}")))?,
        created_at: parse_datetime(
            &row.try_get::<String, _>("created_at").map_err(|e| {
                DomainError::InvalidConfig(format!("workspace_bindings.created_at: {e}"))
            })?,
            "workspace_bindings.created_at",
        )?,
        updated_at: parse_datetime(
            &row.try_get::<String, _>("updated_at").map_err(|e| {
                DomainError::InvalidConfig(format!("workspace_bindings.updated_at: {e}"))
            })?,
            "workspace_bindings.updated_at",
        )?,
    })
}

fn parse_uuid(value: String, entity: &'static str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(&value).map_err(move |_| DomainError::NotFound { entity, id: value })
}

fn parse_datetime(value: &str, field: &str) -> Result<DateTime<Utc>, DomainError> {
    super::parse_pg_timestamp_checked(value, field)
}

fn parse_json_value(raw: &str, field: &str) -> Result<Value, DomainError> {
    serde_json::from_str::<Value>(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
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
            "workspaces.identity_kind: 未知值 `{value}`"
        ))),
    }
}

fn binding_status_to_str(value: &WorkspaceBindingStatus) -> &'static str {
    match value {
        WorkspaceBindingStatus::Pending => "pending",
        WorkspaceBindingStatus::Ready => "ready",
        WorkspaceBindingStatus::Offline => "offline",
        WorkspaceBindingStatus::Error => "error",
    }
}

fn str_to_binding_status(value: &str) -> Result<WorkspaceBindingStatus, DomainError> {
    match value {
        "pending" => Ok(WorkspaceBindingStatus::Pending),
        "ready" => Ok(WorkspaceBindingStatus::Ready),
        "offline" => Ok(WorkspaceBindingStatus::Offline),
        "error" => Ok(WorkspaceBindingStatus::Error),
        _ => Err(DomainError::InvalidConfig(format!(
            "workspace_bindings.status: 未知值 `{value}`"
        ))),
    }
}

fn resolution_policy_to_str(value: &WorkspaceResolutionPolicy) -> &'static str {
    match value {
        WorkspaceResolutionPolicy::PreferDefaultBinding => "prefer_default_binding",
        WorkspaceResolutionPolicy::PreferOnline => "prefer_online",
    }
}

fn str_to_resolution_policy(value: &str) -> Result<WorkspaceResolutionPolicy, DomainError> {
    match value {
        "prefer_default_binding" => Ok(WorkspaceResolutionPolicy::PreferDefaultBinding),
        "prefer_online" => Ok(WorkspaceResolutionPolicy::PreferOnline),
        _ => Err(DomainError::InvalidConfig(format!(
            "workspaces.resolution_policy: 未知值 `{value}`"
        ))),
    }
}

fn workspace_status_to_str(value: &WorkspaceStatus) -> &'static str {
    match value {
        WorkspaceStatus::Pending => "pending",
        WorkspaceStatus::Ready => "ready",
        WorkspaceStatus::Active => "active",
        WorkspaceStatus::Archived => "archived",
        WorkspaceStatus::Error => "error",
    }
}

fn str_to_workspace_status(value: &str) -> Result<WorkspaceStatus, DomainError> {
    match value {
        "pending" => Ok(WorkspaceStatus::Pending),
        "ready" => Ok(WorkspaceStatus::Ready),
        "active" => Ok(WorkspaceStatus::Active),
        "archived" => Ok(WorkspaceStatus::Archived),
        "error" => Ok(WorkspaceStatus::Error),
        _ => Err(DomainError::InvalidConfig(format!(
            "workspaces.status: 未知值 `{value}`"
        ))),
    }
}
