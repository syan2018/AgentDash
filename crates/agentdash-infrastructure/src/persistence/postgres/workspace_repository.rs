use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};
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
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id),
                name TEXT NOT NULL,
                identity_kind TEXT NOT NULL DEFAULT 'local_dir',
                identity_payload TEXT NOT NULL DEFAULT '{}',
                resolution_policy TEXT NOT NULL DEFAULT 'prefer_online',
                default_binding_id TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_workspaces_project ON workspaces(project_id);
            CREATE INDEX IF NOT EXISTS idx_workspaces_status ON workspaces(status);

            CREATE TABLE IF NOT EXISTS workspace_bindings (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                backend_id TEXT NOT NULL,
                root_ref TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                detected_facts TEXT NOT NULL DEFAULT '{}',
                last_verified_at TEXT,
                priority INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_workspace_bindings_workspace ON workspace_bindings(workspace_id);
            CREATE INDEX IF NOT EXISTS idx_workspace_bindings_backend ON workspace_bindings(backend_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.ensure_workspace_column("name", "TEXT NOT NULL DEFAULT ''")
            .await?;
        self.ensure_workspace_column("identity_kind", "TEXT NOT NULL DEFAULT 'local_dir'")
            .await?;
        self.ensure_workspace_column("identity_payload", "TEXT NOT NULL DEFAULT '{}'")
            .await?;
        self.ensure_workspace_column("resolution_policy", "TEXT NOT NULL DEFAULT 'prefer_online'")
            .await?;
        self.ensure_workspace_column("default_binding_id", "TEXT")
            .await?;
        self.ensure_workspace_column("status", "TEXT NOT NULL DEFAULT 'pending'")
            .await?;
        self.ensure_workspace_column("created_at", "TEXT").await?;
        self.ensure_workspace_column("updated_at", "TEXT").await?;
        self.ensure_workspace_column("mount_capabilities", "TEXT NOT NULL DEFAULT '[]'")
            .await?;

        Ok(())
    }

    async fn ensure_workspace_column(
        &self,
        column_name: &str,
        column_sql: &str,
    ) -> Result<(), DomainError> {
        let pragma = sqlx::query(
            "SELECT column_name AS name
             FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = 'workspaces'",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        let column_names = pragma
            .iter()
            .map(|row| {
                row.try_get::<String, _>("name")
                    .map_err(|e| DomainError::InvalidConfig(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let exists = column_names.iter().any(|value| value == column_name);
        if exists {
            return Ok(());
        }

        let query = format!("ALTER TABLE workspaces ADD COLUMN {column_name} {column_sql}");
        sqlx::query(&query)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
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
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        for binding in bindings {
            let detected_facts =
                serde_json::to_string(&binding.detected_facts).map_err(|error| {
                    DomainError::InvalidConfig(format!("序列化 workspace binding 失败: {error}"))
                })?;
            sqlx::query(
                "INSERT INTO workspace_bindings (id, workspace_id, backend_id, root_ref, status, detected_facts, last_verified_at, priority, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(binding.id.to_string())
            .bind(workspace_id.to_string())
            .bind(binding.backend_id.trim())
            .bind(binding.root_ref.trim())
            .bind(binding_status_to_str(&binding.status))
            .bind(detected_facts)
            .bind(binding.last_verified_at.map(|value| value.to_rfc3339()))
            .bind(binding.priority)
            .bind(binding.created_at.to_rfc3339())
            .bind(binding.updated_at.to_rfc3339())
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }

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
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter()
            .map(|row| workspace_binding_from_row(&row))
            .collect()
    }
}

#[async_trait::async_trait]
impl WorkspaceRepository for PostgresWorkspaceRepository {
    async fn create(&self, workspace: &Workspace) -> Result<(), DomainError> {
        let payload = serde_json::to_string(&workspace.identity_payload)
            .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let mount_caps = serde_json::to_string(&workspace.mount_capabilities)
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
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
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Self::save_bindings_in_tx(&mut tx, workspace.id, &workspace.bindings).await?;
        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
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
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

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
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let mut workspaces = Vec::with_capacity(rows.len());
        for row in rows {
            let mut workspace = workspace_from_row(&row)?;
            workspace.bindings = self.load_bindings(workspace.id).await?;
            workspace.refresh_default_binding();
            workspaces.push(workspace);
        }
        Ok(workspaces)
    }

    async fn update(&self, workspace: &Workspace) -> Result<(), DomainError> {
        let payload = serde_json::to_string(&workspace.identity_payload)
            .map_err(|error| DomainError::InvalidConfig(error.to_string()))?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let mount_caps = serde_json::to_string(&workspace.mount_capabilities)
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
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
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "workspace",
                id: workspace.id.to_string(),
            });
        }

        Self::save_bindings_in_tx(&mut tx, workspace.id, &workspace.bindings).await?;
        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        sqlx::query("DELETE FROM workspace_bindings WHERE workspace_id = $1")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let result = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "workspace",
                id: id.to_string(),
            });
        }
        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }
}

fn workspace_from_row(row: &sqlx::postgres::PgRow) -> Result<Workspace, DomainError> {
    let id = parse_uuid(
        row.try_get::<String, _>("id")
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?,
        "workspace",
    )?;
    let project_id = parse_uuid(
        row.try_get::<String, _>("project_id")
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?,
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
        .unwrap_or_else(|_| "[]".to_string());
    let mount_capabilities: Vec<agentdash_domain::context_container::ContextContainerCapability> =
        serde_json::from_str(&mount_capabilities_raw).unwrap_or_default();

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
            row.try_get::<String, _>("id")
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?,
            "workspace_binding",
        )?,
        workspace_id: parse_uuid(
            row.try_get::<String, _>("workspace_id")
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?,
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
