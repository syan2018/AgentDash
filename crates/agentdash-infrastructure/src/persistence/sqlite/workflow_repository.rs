use sqlx::SqlitePool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    WorkflowAgentRole, WorkflowAssignment, WorkflowAssignmentRepository, WorkflowDefinition,
    WorkflowDefinitionRepository, WorkflowRun, WorkflowRunRepository, WorkflowTargetKind,
};

pub struct SqliteWorkflowRepository {
    pool: SqlitePool,
}

impl SqliteWorkflowRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS workflow_definitions (
                id TEXT PRIMARY KEY,
                key TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                target_kind TEXT NOT NULL,
                version INTEGER NOT NULL,
                enabled INTEGER NOT NULL,
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

        Ok(())
    }
}

#[async_trait::async_trait]
impl WorkflowDefinitionRepository for SqliteWorkflowRepository {
    async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO workflow_definitions
            (id, key, name, description, target_kind, version, enabled, phases, record_policy, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(workflow.id.to_string())
        .bind(&workflow.key)
        .bind(&workflow.name)
        .bind(&workflow.description)
        .bind(serde_json::to_string(&workflow.target_kind)?)
        .bind(workflow.version)
        .bind(workflow.enabled)
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
            "SELECT id, key, name, description, target_kind, version, enabled, phases, record_policy, created_at, updated_at
             FROM workflow_definitions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
        let row = sqlx::query_as::<_, WorkflowDefinitionRow>(
            "SELECT id, key, name, description, target_kind, version, enabled, phases, record_policy, created_at, updated_at
             FROM workflow_definitions WHERE key = ?",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowDefinitionRow>(
            "SELECT id, key, name, description, target_kind, version, enabled, phases, record_policy, created_at, updated_at
             FROM workflow_definitions ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_enabled(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowDefinitionRow>(
            "SELECT id, key, name, description, target_kind, version, enabled, phases, record_policy, created_at, updated_at
             FROM workflow_definitions WHERE enabled = 1 ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_target_kind(
        &self,
        target_kind: WorkflowTargetKind,
    ) -> Result<Vec<WorkflowDefinition>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowDefinitionRow>(
            "SELECT id, key, name, description, target_kind, version, enabled, phases, record_policy, created_at, updated_at
             FROM workflow_definitions WHERE target_kind = ? ORDER BY created_at DESC",
        )
        .bind(serde_json::to_string(&target_kind)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE workflow_definitions
             SET key = ?, name = ?, description = ?, target_kind = ?, version = ?, enabled = ?, phases = ?, record_policy = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&workflow.key)
        .bind(&workflow.name)
        .bind(&workflow.description)
        .bind(serde_json::to_string(&workflow.target_kind)?)
        .bind(workflow.version)
        .bind(workflow.enabled)
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
            (id, workflow_id, target_kind, target_id, status, current_phase_key, phase_states, record_artifacts, created_at, updated_at, last_activity_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run.id.to_string())
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
            "SELECT id, workflow_id, target_kind, target_id, status, current_phase_key, phase_states, record_artifacts, created_at, updated_at, last_activity_at
             FROM workflow_runs WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_by_workflow(
        &self,
        workflow_id: uuid::Uuid,
    ) -> Result<Vec<WorkflowRun>, DomainError> {
        let rows = sqlx::query_as::<_, WorkflowRunRow>(
            "SELECT id, workflow_id, target_kind, target_id, status, current_phase_key, phase_states, record_artifacts, created_at, updated_at, last_activity_at
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
            "SELECT id, workflow_id, target_kind, target_id, status, current_phase_key, phase_states, record_artifacts, created_at, updated_at, last_activity_at
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
             SET workflow_id = ?, target_kind = ?, target_id = ?, status = ?, current_phase_key = ?, phase_states = ?, record_artifacts = ?, updated_at = ?, last_activity_at = ?
             WHERE id = ?",
        )
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
    version: i32,
    enabled: bool,
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
            version: row.version,
            enabled: row.enabled,
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
