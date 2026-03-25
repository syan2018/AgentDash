use sqlx::SqlitePool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    WorkflowAgentRole, WorkflowAssignment, WorkflowAssignmentRepository, WorkflowDefinition,
    WorkflowDefinitionRepository, WorkflowDefinitionStatus, WorkflowRun, WorkflowRunRepository,
    WorkflowTargetKind,
};

pub struct SqliteWorkflowRepository {
    pool: SqlitePool,
}

impl SqliteWorkflowRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        // Detect legacy schema: if `enabled` column exists, drop and recreate the table.
        let has_legacy = sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM pragma_table_info('workflow_definitions') WHERE name = 'enabled'"
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        if has_legacy > 0 {
            tracing::info!("检测到旧版 workflow_definitions 表（含 enabled 列），重建表结构");
            sqlx::query("DROP TABLE IF EXISTS workflow_definitions")
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS workflow_definitions (
                id TEXT PRIMARY KEY,
                key TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                target_kind TEXT NOT NULL,
                recommended_role TEXT,
                source TEXT NOT NULL DEFAULT '"user_authored"',
                status TEXT NOT NULL DEFAULT '"active"',
                version INTEGER NOT NULL,
                phases TEXT NOT NULL,
                record_policy TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS workflow_assignments (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                workflow_id TEXT NOT NULL,
                role TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                is_default INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS workflow_runs (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL DEFAULT '',
                workflow_id TEXT NOT NULL,
                target_kind TEXT NOT NULL,
                target_id TEXT NOT NULL,
                status TEXT NOT NULL,
                current_phase_key TEXT,
                phase_states TEXT NOT NULL,
                record_artifacts TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_activity_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_workflow_runs_project_id ON workflow_runs(project_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn parse_definition_row(
        &self,
        row: WorkflowDefinitionRow,
    ) -> Result<Option<WorkflowDefinition>, DomainError> {
        let row_id = row.id.clone();
        let row_key = row.key.clone();
        match WorkflowDefinition::try_from(row) {
            Ok(definition) => Ok(Some(definition)),
            Err(error) => {
                tracing::warn!(
                    workflow_definition_id = %row_id,
                    workflow_definition_key = %row_key,
                    error = %error,
                    "检测到损坏的 workflow_definition，已自动清理"
                );
                sqlx::query("DELETE FROM workflow_definitions WHERE id = ?")
                    .bind(&row_id)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
                Ok(None)
            }
        }
    }

    async fn collect_valid_definitions(
        &self,
        rows: Vec<WorkflowDefinitionRow>,
    ) -> Result<Vec<WorkflowDefinition>, DomainError> {
        let mut definitions = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(definition) = self.parse_definition_row(row).await? {
                definitions.push(definition);
            }
        }
        Ok(definitions)
    }
}

#[async_trait::async_trait]
impl WorkflowDefinitionRepository for SqliteWorkflowRepository {
    async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO workflow_definitions
            (id, key, name, description, target_kind, recommended_role, source, status, version, phases, record_policy, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(workflow.id.to_string())
        .bind(&workflow.key)
        .bind(&workflow.name)
        .bind(&workflow.description)
        .bind(serde_json::to_string(&workflow.target_kind)?)
        .bind(workflow.recommended_role.map(|r| serde_json::to_string(&r).unwrap_or_default()))
        .bind(serde_json::to_string(&workflow.source)?)
        .bind(serde_json::to_string(&workflow.status)?)
        .bind(workflow.version)
        .bind(serde_json::to_string(&workflow.phases)?)
        .bind(serde_json::to_string(&workflow.record_policy)?)
        .bind(workflow.created_at.to_rfc3339())
        .bind(workflow.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
        let row = sqlx::query_as::<_, WorkflowDefinitionRow>(
            "SELECT id, key, name, description, target_kind, recommended_role, source, status, version, phases, record_policy, created_at, updated_at
             FROM workflow_definitions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        match row {
            Some(row) => self.parse_definition_row(row).await,
            None => Ok(None),
        }
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
        let row = sqlx::query_as::<_, WorkflowDefinitionRow>(
            "SELECT id, key, name, description, target_kind, recommended_role, source, status, version, phases, record_policy, created_at, updated_at
             FROM workflow_definitions WHERE key = ?",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        match row {
            Some(row) => self.parse_definition_row(row).await,
            None => Ok(None),
        }
    }

    async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowDefinitionRow>(
            "SELECT id, key, name, description, target_kind, recommended_role, source, status, version, phases, record_policy, created_at, updated_at
             FROM workflow_definitions ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.collect_valid_definitions(rows).await
    }

    async fn list_by_status(
        &self,
        status: WorkflowDefinitionStatus,
    ) -> Result<Vec<WorkflowDefinition>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowDefinitionRow>(
            "SELECT id, key, name, description, target_kind, recommended_role, source, status, version, phases, record_policy, created_at, updated_at
             FROM workflow_definitions WHERE status = ? ORDER BY created_at DESC",
        )
        .bind(serde_json::to_string(&status)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.collect_valid_definitions(rows).await
    }

    async fn list_by_target_kind(
        &self,
        target_kind: WorkflowTargetKind,
    ) -> Result<Vec<WorkflowDefinition>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowDefinitionRow>(
            "SELECT id, key, name, description, target_kind, recommended_role, source, status, version, phases, record_policy, created_at, updated_at
             FROM workflow_definitions WHERE target_kind = ? ORDER BY created_at DESC",
        )
        .bind(serde_json::to_string(&target_kind)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.collect_valid_definitions(rows).await
    }

    async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE workflow_definitions
             SET key = ?, name = ?, description = ?, target_kind = ?, recommended_role = ?, source = ?, status = ?, version = ?, phases = ?, record_policy = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&workflow.key)
        .bind(&workflow.name)
        .bind(&workflow.description)
        .bind(serde_json::to_string(&workflow.target_kind)?)
        .bind(workflow.recommended_role.map(|r| serde_json::to_string(&r).unwrap_or_default()))
        .bind(serde_json::to_string(&workflow.source)?)
        .bind(serde_json::to_string(&workflow.status)?)
        .bind(workflow.version)
        .bind(serde_json::to_string(&workflow.phases)?)
        .bind(serde_json::to_string(&workflow.record_policy)?)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(workflow.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        ensure_rows_affected(result.rows_affected(), "workflow_definition", &workflow.id)?;
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM workflow_definitions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        ensure_rows_affected(result.rows_affected(), "workflow_definition", &id)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl WorkflowAssignmentRepository for SqliteWorkflowRepository {
    async fn create(&self, assignment: &WorkflowAssignment) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO workflow_assignments
            (id, project_id, workflow_id, role, enabled, is_default, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(assignment.id.to_string())
        .bind(assignment.project_id.to_string())
        .bind(assignment.workflow_id.to_string())
        .bind(serde_json::to_string(&assignment.role)?)
        .bind(assignment.enabled)
        .bind(assignment.is_default)
        .bind(assignment.created_at.to_rfc3339())
        .bind(assignment.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowAssignment>, DomainError> {
        let row = sqlx::query_as::<_, WorkflowAssignmentRow>(
            "SELECT id, project_id, workflow_id, role, enabled, is_default, created_at, updated_at
             FROM workflow_assignments WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<WorkflowAssignment>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowAssignmentRow>(
            "SELECT id, project_id, workflow_id, role, enabled, is_default, created_at, updated_at
             FROM workflow_assignments WHERE project_id = ? ORDER BY created_at DESC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_project_and_role(
        &self,
        project_id: uuid::Uuid,
        role: WorkflowAgentRole,
    ) -> Result<Vec<WorkflowAssignment>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowAssignmentRow>(
            "SELECT id, project_id, workflow_id, role, enabled, is_default, created_at, updated_at
             FROM workflow_assignments WHERE project_id = ? AND role = ? ORDER BY created_at DESC",
        )
        .bind(project_id.to_string())
        .bind(serde_json::to_string(&role)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, assignment: &WorkflowAssignment) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE workflow_assignments
             SET project_id = ?, workflow_id = ?, role = ?, enabled = ?, is_default = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(assignment.project_id.to_string())
        .bind(assignment.workflow_id.to_string())
        .bind(serde_json::to_string(&assignment.role)?)
        .bind(assignment.enabled)
        .bind(assignment.is_default)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(assignment.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        ensure_rows_affected(
            result.rows_affected(),
            "workflow_assignment",
            &assignment.id,
        )?;
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM workflow_assignments WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        ensure_rows_affected(result.rows_affected(), "workflow_assignment", &id)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl WorkflowRunRepository for SqliteWorkflowRepository {
    async fn create(&self, run: &WorkflowRun) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO workflow_runs
            (id, project_id, workflow_id, target_kind, target_id, status, current_phase_key, phase_states, record_artifacts, created_at, updated_at, last_activity_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run.id.to_string())
        .bind(run.project_id.to_string())
        .bind(run.workflow_id.to_string())
        .bind(serde_json::to_string(&run.target_kind)?)
        .bind(run.target_id.to_string())
        .bind(serde_json::to_string(&run.status)?)
        .bind(&run.current_phase_key)
        .bind(serde_json::to_string(&run.phase_states)?)
        .bind(serde_json::to_string(&run.record_artifacts)?)
        .bind(run.created_at.to_rfc3339())
        .bind(run.updated_at.to_rfc3339())
        .bind(run.last_activity_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowRun>, DomainError> {
        let row = sqlx::query_as::<_, WorkflowRunRow>(
            "SELECT id, project_id, workflow_id, target_kind, target_id, status, current_phase_key, phase_states, record_artifacts, created_at, updated_at, last_activity_at
             FROM workflow_runs WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<WorkflowRun>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowRunRow>(
            "SELECT id, project_id, workflow_id, target_kind, target_id, status, current_phase_key, phase_states, record_artifacts, created_at, updated_at, last_activity_at
             FROM workflow_runs WHERE project_id = ? ORDER BY created_at DESC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_workflow(
        &self,
        workflow_id: uuid::Uuid,
    ) -> Result<Vec<WorkflowRun>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowRunRow>(
            "SELECT id, project_id, workflow_id, target_kind, target_id, status, current_phase_key, phase_states, record_artifacts, created_at, updated_at, last_activity_at
             FROM workflow_runs WHERE workflow_id = ? ORDER BY created_at DESC",
        )
        .bind(workflow_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_target(
        &self,
        target_kind: WorkflowTargetKind,
        target_id: uuid::Uuid,
    ) -> Result<Vec<WorkflowRun>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowRunRow>(
            "SELECT id, project_id, workflow_id, target_kind, target_id, status, current_phase_key, phase_states, record_artifacts, created_at, updated_at, last_activity_at
             FROM workflow_runs WHERE target_kind = ? AND target_id = ? ORDER BY created_at DESC",
        )
        .bind(serde_json::to_string(&target_kind)?)
        .bind(target_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, run: &WorkflowRun) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE workflow_runs
             SET project_id = ?, workflow_id = ?, target_kind = ?, target_id = ?, status = ?, current_phase_key = ?, phase_states = ?, record_artifacts = ?, updated_at = ?, last_activity_at = ?
             WHERE id = ?",
        )
        .bind(run.project_id.to_string())
        .bind(run.workflow_id.to_string())
        .bind(serde_json::to_string(&run.target_kind)?)
        .bind(run.target_id.to_string())
        .bind(serde_json::to_string(&run.status)?)
        .bind(&run.current_phase_key)
        .bind(serde_json::to_string(&run.phase_states)?)
        .bind(serde_json::to_string(&run.record_artifacts)?)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(run.last_activity_at.to_rfc3339())
        .bind(run.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        ensure_rows_affected(result.rows_affected(), "workflow_run", &run.id)?;
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM workflow_runs WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        ensure_rows_affected(result.rows_affected(), "workflow_run", &id)?;
        Ok(())
    }
}

fn ensure_rows_affected(
    rows_affected: u64,
    entity: &'static str,
    id: &uuid::Uuid,
) -> Result<(), DomainError> {
    if rows_affected == 0 {
        return Err(DomainError::NotFound {
            entity,
            id: id.to_string(),
        });
    }
    Ok(())
}

#[derive(sqlx::FromRow)]
struct WorkflowDefinitionRow {
    id: String,
    key: String,
    name: String,
    description: String,
    target_kind: String,
    recommended_role: Option<String>,
    source: String,
    status: String,
    version: i32,
    phases: String,
    record_policy: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<WorkflowDefinitionRow> for WorkflowDefinition {
    type Error = DomainError;

    fn try_from(row: WorkflowDefinitionRow) -> Result<Self, Self::Error> {
        Ok(WorkflowDefinition {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "workflow_definition",
                id: row.id.clone(),
            })?,
            key: row.key,
            name: row.name,
            description: row.description,
            target_kind: serde_json::from_str(&row.target_kind)?,
            recommended_role: row.recommended_role.as_deref().and_then(|v| serde_json::from_str(v).ok()),
            source: serde_json::from_str(&row.source).unwrap_or(agentdash_domain::workflow::WorkflowDefinitionSource::BuiltinSeed),
            status: serde_json::from_str(&row.status).unwrap_or(agentdash_domain::workflow::WorkflowDefinitionStatus::Active),
            version: row.version,
            phases: serde_json::from_str(&row.phases)?,
            record_policy: serde_json::from_str(&row.record_policy)?,
            created_at: parse_time(&row.created_at),
            updated_at: parse_time(&row.updated_at),
        })
    }
}

#[derive(sqlx::FromRow)]
struct WorkflowAssignmentRow {
    id: String,
    project_id: String,
    workflow_id: String,
    role: String,
    enabled: bool,
    is_default: bool,
    created_at: String,
    updated_at: String,
}

impl TryFrom<WorkflowAssignmentRow> for WorkflowAssignment {
    type Error = DomainError;

    fn try_from(row: WorkflowAssignmentRow) -> Result<Self, Self::Error> {
        Ok(WorkflowAssignment {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "workflow_assignment",
                id: row.id.clone(),
            })?,
            project_id: row.project_id.parse().map_err(|_| DomainError::NotFound {
                entity: "project",
                id: row.project_id.clone(),
            })?,
            workflow_id: row.workflow_id.parse().map_err(|_| DomainError::NotFound {
                entity: "workflow_definition",
                id: row.workflow_id.clone(),
            })?,
            role: serde_json::from_str(&row.role)?,
            enabled: row.enabled,
            is_default: row.is_default,
            created_at: parse_time(&row.created_at),
            updated_at: parse_time(&row.updated_at),
        })
    }
}

#[derive(sqlx::FromRow)]
struct WorkflowRunRow {
    id: String,
    project_id: String,
    workflow_id: String,
    target_kind: String,
    target_id: String,
    status: String,
    current_phase_key: Option<String>,
    phase_states: String,
    record_artifacts: String,
    created_at: String,
    updated_at: String,
    last_activity_at: String,
}

impl TryFrom<WorkflowRunRow> for WorkflowRun {
    type Error = DomainError;

    fn try_from(row: WorkflowRunRow) -> Result<Self, Self::Error> {
        Ok(WorkflowRun {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "workflow_run",
                id: row.id.clone(),
            })?,
            project_id: row.project_id.parse().map_err(|_| DomainError::NotFound {
                entity: "project",
                id: row.project_id.clone(),
            })?,
            workflow_id: row.workflow_id.parse().map_err(|_| DomainError::NotFound {
                entity: "workflow_definition",
                id: row.workflow_id.clone(),
            })?,
            target_kind: serde_json::from_str(&row.target_kind)?,
            target_id: row.target_id.parse().map_err(|_| DomainError::NotFound {
                entity: "workflow_target",
                id: row.target_id.clone(),
            })?,
            status: serde_json::from_str(&row.status)?,
            current_phase_key: row.current_phase_key,
            phase_states: serde_json::from_str(&row.phase_states)?,
            record_artifacts: serde_json::from_str(&row.record_artifacts)?,
            created_at: parse_time(&row.created_at),
            updated_at: parse_time(&row.updated_at),
            last_activity_at: parse_time(&row.last_activity_at),
        })
    }
}

fn parse_time(raw: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{
        WorkflowContextBinding, WorkflowContextBindingKind, WorkflowDefinition,
        WorkflowDefinitionSource, WorkflowPhaseCompletionMode, WorkflowPhaseDefinition,
        WorkflowTargetKind,
    };

    const TEST_WORKFLOW_KEY: &str = "trellis_dev_task";

    async fn new_repo() -> SqliteWorkflowRepository {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("应能创建内存 sqlite");
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY
            );
            CREATE TABLE IF NOT EXISTS stories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .expect("应能准备 workflow 依赖表");
        let repo = SqliteWorkflowRepository::new(pool);
        repo.initialize().await.expect("应能初始化 workflow schema");
        repo
    }

    fn phase(key: &str) -> WorkflowPhaseDefinition {
        WorkflowPhaseDefinition {
            key: key.to_string(),
            title: key.to_string(),
            description: "desc".to_string(),
            agent_instructions: vec![],
            context_bindings: vec![WorkflowContextBinding {
                kind: WorkflowContextBindingKind::DocumentPath,
                locator: ".trellis/workflow.md".to_string(),
                reason: "workflow".to_string(),
                required: true,
                title: None,
            }],
            requires_session: true,
            completion_mode: WorkflowPhaseCompletionMode::Manual,
            default_artifact_type: None,
            default_artifact_title: None,
        }
    }

    fn valid_definition() -> WorkflowDefinition {
        WorkflowDefinition::new(
            TEST_WORKFLOW_KEY,
            "Trellis Dev Workflow / Task",
            "valid workflow definition",
            WorkflowTargetKind::Task,
            WorkflowDefinitionSource::BuiltinSeed,
            vec![phase("start"), phase("implement")],
        )
        .expect("应能构建 workflow definition")
    }

    #[tokio::test]
    async fn get_by_key_cleans_up_corrupted_workflow_definition_row() {
        let repo = new_repo().await;
        let row_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO workflow_definitions
            (id, key, name, description, target_kind, recommended_role, source, status, version, phases, record_policy, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row_id.to_string())
        .bind("legacy_bad_workflow")
        .bind("Legacy Bad Workflow")
        .bind("corrupted phases payload")
        .bind("\"task\"")
        .bind(Option::<String>::None)
        .bind("\"builtin_seed\"")
        .bind("\"active\"")
        .bind(1_i32)
        .bind("{\"legacy\":true}")
        .bind("{\"emit_summary\":true}")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&repo.pool)
        .await
        .expect("应能插入损坏 workflow row");

        let definition = repo
            .get_by_key("legacy_bad_workflow")
            .await
            .expect("查询时应自动清理损坏 row");

        assert!(definition.is_none());

        let remaining: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM workflow_definitions WHERE id = ?")
                .bind(row_id.to_string())
                .fetch_one(&repo.pool)
                .await
                .expect("应能查询剩余行数");
        assert_eq!(remaining.0, 0);
    }

    #[tokio::test]
    async fn list_all_keeps_valid_definitions_and_skips_corrupted_rows() {
        let repo = new_repo().await;
        let valid = valid_definition();
        WorkflowDefinitionRepository::create(&repo, &valid)
            .await
            .expect("应能插入有效 definition");

        sqlx::query(
            "INSERT INTO workflow_definitions
            (id, key, name, description, target_kind, recommended_role, source, status, version, phases, record_policy, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind("legacy_corrupted_definition")
        .bind("Legacy Corrupted Definition")
        .bind("corrupted record_policy payload")
        .bind("\"task\"")
        .bind(Option::<String>::None)
        .bind("\"builtin_seed\"")
        .bind("\"active\"")
        .bind(1_i32)
        .bind(serde_json::to_string(&valid.phases).expect("serialize phases"))
        .bind("\"bad_policy\"")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&repo.pool)
        .await
        .expect("应能插入损坏 row");

        let definitions = repo
            .list_all()
            .await
            .expect("列举 definitions 时应跳过并清理坏数据");

        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].key, TEST_WORKFLOW_KEY);
    }
}
