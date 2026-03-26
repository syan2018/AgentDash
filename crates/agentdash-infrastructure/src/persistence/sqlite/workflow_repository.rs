use sqlx::SqlitePool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, LifecycleRun, LifecycleRunRepository,
    WorkflowAgentRole, WorkflowAssignment, WorkflowAssignmentRepository, WorkflowDefinition,
    WorkflowDefinitionRepository, WorkflowDefinitionStatus, WorkflowTargetKind,
};

pub struct SqliteWorkflowRepository {
    pool: SqlitePool,
}

impl SqliteWorkflowRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        recreate_if_column_exists(&self.pool, "workflow_definitions", "phases").await?;
        recreate_if_column_exists(&self.pool, "workflow_assignments", "workflow_id").await?;
        recreate_if_column_exists(&self.pool, "workflow_runs", "current_phase_key").await?;
        recreate_if_column_exists(&self.pool, "lifecycle_runs", "workflow_id").await?;
        recreate_if_column_exists(&self.pool, "workflow_definitions", "record_policy").await?;
        recreate_if_column_exists(&self.pool, "workflow_definitions", "recommended_role").await?;
        recreate_if_column_exists(&self.pool, "lifecycle_definitions", "recommended_role").await?;
        recreate_if_column_exists(&self.pool, "lifecycle_runs", "runtime_attachments").await?;

        sqlx::query(r#"CREATE TABLE IF NOT EXISTS workflow_definitions (
            id TEXT PRIMARY KEY, key TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '', target_kind TEXT NOT NULL, recommended_roles TEXT NOT NULL DEFAULT '[]',
            source TEXT NOT NULL, status TEXT NOT NULL, version INTEGER NOT NULL, contract TEXT NOT NULL,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL
        )"#)
        .execute(&self.pool).await.map_err(db_err)?;

        sqlx::query(r#"CREATE TABLE IF NOT EXISTS lifecycle_definitions (
            id TEXT PRIMARY KEY, key TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '', target_kind TEXT NOT NULL, recommended_roles TEXT NOT NULL DEFAULT '[]',
            source TEXT NOT NULL, status TEXT NOT NULL, version INTEGER NOT NULL,
            entry_step_key TEXT NOT NULL, steps TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
        )"#)
        .execute(&self.pool).await.map_err(db_err)?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS workflow_assignments (
            id TEXT PRIMARY KEY, project_id TEXT NOT NULL, lifecycle_id TEXT NOT NULL,
            role TEXT NOT NULL, enabled INTEGER NOT NULL, is_default INTEGER NOT NULL,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL
        )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS lifecycle_runs (
            id TEXT PRIMARY KEY, project_id TEXT NOT NULL, lifecycle_id TEXT NOT NULL,
            target_kind TEXT NOT NULL, target_id TEXT NOT NULL, status TEXT NOT NULL,
            current_step_key TEXT, step_states TEXT NOT NULL,
            record_artifacts TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            last_activity_at TEXT NOT NULL
        )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_lifecycle_runs_project_id ON lifecycle_runs(project_id)")
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl WorkflowDefinitionRepository for SqliteWorkflowRepository {
    async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO workflow_definitions (id,key,name,description,target_kind,recommended_roles,source,status,version,contract,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(workflow.id.to_string()).bind(&workflow.key).bind(&workflow.name).bind(&workflow.description)
            .bind(serde_json::to_string(&workflow.target_kind)?)
            .bind(serde_json::to_string(&workflow.recommended_roles)?)
            .bind(serde_json::to_string(&workflow.source)?).bind(serde_json::to_string(&workflow.status)?)
            .bind(workflow.version).bind(serde_json::to_string(&workflow.contract)?)
            .bind(workflow.created_at.to_rfc3339()).bind(workflow.updated_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>("SELECT id,key,name,description,target_kind,recommended_roles,source,status,version,contract,created_at,updated_at FROM workflow_definitions WHERE id = ?")
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>("SELECT id,key,name,description,target_kind,recommended_roles,source,status,version,contract,created_at,updated_at FROM workflow_definitions WHERE key = ?")
            .bind(key).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>("SELECT id,key,name,description,target_kind,recommended_roles,source,status,version,contract,created_at,updated_at FROM workflow_definitions ORDER BY created_at DESC")
            .fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_status(
        &self,
        status: WorkflowDefinitionStatus,
    ) -> Result<Vec<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>("SELECT id,key,name,description,target_kind,recommended_roles,source,status,version,contract,created_at,updated_at FROM workflow_definitions WHERE status = ? ORDER BY created_at DESC")
            .bind(serde_json::to_string(&status)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_target_kind(
        &self,
        target_kind: WorkflowTargetKind,
    ) -> Result<Vec<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>("SELECT id,key,name,description,target_kind,recommended_roles,source,status,version,contract,created_at,updated_at FROM workflow_definitions WHERE target_kind = ? ORDER BY created_at DESC")
            .bind(serde_json::to_string(&target_kind)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE workflow_definitions SET key=?,name=?,description=?,target_kind=?,recommended_roles=?,source=?,status=?,version=?,contract=?,updated_at=? WHERE id=?")
            .bind(&workflow.key).bind(&workflow.name).bind(&workflow.description)
            .bind(serde_json::to_string(&workflow.target_kind)?)
            .bind(serde_json::to_string(&workflow.recommended_roles)?)
            .bind(serde_json::to_string(&workflow.source)?).bind(serde_json::to_string(&workflow.status)?)
            .bind(workflow.version).bind(serde_json::to_string(&workflow.contract)?)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(workflow.id.to_string()).execute(&self.pool).await.map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "workflow_definition", &workflow.id)
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM workflow_definitions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "workflow_definition", &id)
    }
}

#[async_trait::async_trait]
impl LifecycleDefinitionRepository for SqliteWorkflowRepository {
    async fn create(&self, lifecycle: &LifecycleDefinition) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO lifecycle_definitions (id,key,name,description,target_kind,recommended_roles,source,status,version,entry_step_key,steps,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(lifecycle.id.to_string()).bind(&lifecycle.key).bind(&lifecycle.name).bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.target_kind)?)
            .bind(serde_json::to_string(&lifecycle.recommended_roles)?)
            .bind(serde_json::to_string(&lifecycle.source)?).bind(serde_json::to_string(&lifecycle.status)?)
            .bind(lifecycle.version).bind(&lifecycle.entry_step_key).bind(serde_json::to_string(&lifecycle.steps)?)
            .bind(lifecycle.created_at.to_rfc3339()).bind(lifecycle.updated_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>("SELECT id,key,name,description,target_kind,recommended_roles,source,status,version,entry_step_key,steps,created_at,updated_at FROM lifecycle_definitions WHERE id = ?")
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>("SELECT id,key,name,description,target_kind,recommended_roles,source,status,version,entry_step_key,steps,created_at,updated_at FROM lifecycle_definitions WHERE key = ?")
            .bind(key).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_all(&self) -> Result<Vec<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>("SELECT id,key,name,description,target_kind,recommended_roles,source,status,version,entry_step_key,steps,created_at,updated_at FROM lifecycle_definitions ORDER BY created_at DESC")
            .fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_status(
        &self,
        status: WorkflowDefinitionStatus,
    ) -> Result<Vec<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>("SELECT id,key,name,description,target_kind,recommended_roles,source,status,version,entry_step_key,steps,created_at,updated_at FROM lifecycle_definitions WHERE status = ? ORDER BY created_at DESC")
            .bind(serde_json::to_string(&status)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_target_kind(
        &self,
        target_kind: WorkflowTargetKind,
    ) -> Result<Vec<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>("SELECT id,key,name,description,target_kind,recommended_roles,source,status,version,entry_step_key,steps,created_at,updated_at FROM lifecycle_definitions WHERE target_kind = ? ORDER BY created_at DESC")
            .bind(serde_json::to_string(&target_kind)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, lifecycle: &LifecycleDefinition) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE lifecycle_definitions SET key=?,name=?,description=?,target_kind=?,recommended_roles=?,source=?,status=?,version=?,entry_step_key=?,steps=?,updated_at=? WHERE id=?")
            .bind(&lifecycle.key).bind(&lifecycle.name).bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.target_kind)?)
            .bind(serde_json::to_string(&lifecycle.recommended_roles)?)
            .bind(serde_json::to_string(&lifecycle.source)?).bind(serde_json::to_string(&lifecycle.status)?)
            .bind(lifecycle.version).bind(&lifecycle.entry_step_key).bind(serde_json::to_string(&lifecycle.steps)?)
            .bind(chrono::Utc::now().to_rfc3339()).bind(lifecycle.id.to_string())
            .execute(&self.pool).await.map_err(db_err)?;
        ensure_rows_affected(
            result.rows_affected(),
            "lifecycle_definition",
            &lifecycle.id,
        )
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM lifecycle_definitions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "lifecycle_definition", &id)
    }
}

#[async_trait::async_trait]
impl WorkflowAssignmentRepository for SqliteWorkflowRepository {
    async fn create(&self, assignment: &WorkflowAssignment) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO workflow_assignments (id,project_id,lifecycle_id,role,enabled,is_default,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?)")
            .bind(assignment.id.to_string()).bind(assignment.project_id.to_string()).bind(assignment.lifecycle_id.to_string())
            .bind(serde_json::to_string(&assignment.role)?).bind(assignment.enabled).bind(assignment.is_default)
            .bind(assignment.created_at.to_rfc3339()).bind(assignment.updated_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowAssignment>, DomainError> {
        sqlx::query_as::<_, WorkflowAssignmentRow>("SELECT id,project_id,lifecycle_id,role,enabled,is_default,created_at,updated_at FROM workflow_assignments WHERE id = ?")
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<WorkflowAssignment>, DomainError> {
        sqlx::query_as::<_, WorkflowAssignmentRow>("SELECT id,project_id,lifecycle_id,role,enabled,is_default,created_at,updated_at FROM workflow_assignments WHERE project_id = ? ORDER BY created_at DESC")
            .bind(project_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_project_and_role(
        &self,
        project_id: uuid::Uuid,
        role: WorkflowAgentRole,
    ) -> Result<Vec<WorkflowAssignment>, DomainError> {
        sqlx::query_as::<_, WorkflowAssignmentRow>("SELECT id,project_id,lifecycle_id,role,enabled,is_default,created_at,updated_at FROM workflow_assignments WHERE project_id = ? AND role = ? ORDER BY created_at DESC")
            .bind(project_id.to_string()).bind(serde_json::to_string(&role)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, assignment: &WorkflowAssignment) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE workflow_assignments SET project_id=?,lifecycle_id=?,role=?,enabled=?,is_default=?,updated_at=? WHERE id=?")
            .bind(assignment.project_id.to_string()).bind(assignment.lifecycle_id.to_string())
            .bind(serde_json::to_string(&assignment.role)?).bind(assignment.enabled).bind(assignment.is_default)
            .bind(chrono::Utc::now().to_rfc3339()).bind(assignment.id.to_string())
            .execute(&self.pool).await.map_err(db_err)?;
        ensure_rows_affected(
            result.rows_affected(),
            "workflow_assignment",
            &assignment.id,
        )
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM workflow_assignments WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "workflow_assignment", &id)
    }
}

#[async_trait::async_trait]
impl LifecycleRunRepository for SqliteWorkflowRepository {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO lifecycle_runs (id,project_id,lifecycle_id,target_kind,target_id,status,current_step_key,step_states,record_artifacts,created_at,updated_at,last_activity_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(run.id.to_string()).bind(run.project_id.to_string()).bind(run.lifecycle_id.to_string())
            .bind(serde_json::to_string(&run.target_kind)?).bind(run.target_id.to_string()).bind(serde_json::to_string(&run.status)?)
            .bind(&run.current_step_key).bind(serde_json::to_string(&run.step_states)?)
            .bind(serde_json::to_string(&run.record_artifacts)?).bind(run.created_at.to_rfc3339()).bind(run.updated_at.to_rfc3339()).bind(run.last_activity_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>("SELECT id,project_id,lifecycle_id,target_kind,target_id,status,current_step_key,step_states,record_artifacts,created_at,updated_at,last_activity_at FROM lifecycle_runs WHERE id = ?")
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>("SELECT id,project_id,lifecycle_id,target_kind,target_id,status,current_step_key,step_states,record_artifacts,created_at,updated_at,last_activity_at FROM lifecycle_runs WHERE project_id = ? ORDER BY created_at DESC")
            .bind(project_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_lifecycle(
        &self,
        lifecycle_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>("SELECT id,project_id,lifecycle_id,target_kind,target_id,status,current_step_key,step_states,record_artifacts,created_at,updated_at,last_activity_at FROM lifecycle_runs WHERE lifecycle_id = ? ORDER BY created_at DESC")
            .bind(lifecycle_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_target(
        &self,
        target_kind: WorkflowTargetKind,
        target_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>("SELECT id,project_id,lifecycle_id,target_kind,target_id,status,current_step_key,step_states,record_artifacts,created_at,updated_at,last_activity_at FROM lifecycle_runs WHERE target_kind = ? AND target_id = ? ORDER BY created_at DESC")
            .bind(serde_json::to_string(&target_kind)?).bind(target_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE lifecycle_runs SET project_id=?,lifecycle_id=?,target_kind=?,target_id=?,status=?,current_step_key=?,step_states=?,record_artifacts=?,updated_at=?,last_activity_at=? WHERE id=?")
            .bind(run.project_id.to_string()).bind(run.lifecycle_id.to_string()).bind(serde_json::to_string(&run.target_kind)?)
            .bind(run.target_id.to_string()).bind(serde_json::to_string(&run.status)?).bind(&run.current_step_key)
            .bind(serde_json::to_string(&run.step_states)?).bind(serde_json::to_string(&run.record_artifacts)?)
            .bind(chrono::Utc::now().to_rfc3339()).bind(run.last_activity_at.to_rfc3339()).bind(run.id.to_string())
            .execute(&self.pool).await.map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "lifecycle_run", &run.id)
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM lifecycle_runs WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "lifecycle_run", &id)
    }
}

fn db_err(error: sqlx::Error) -> DomainError {
    DomainError::InvalidConfig(error.to_string())
}

async fn recreate_if_column_exists(
    pool: &SqlitePool,
    table: &str,
    column: &str,
) -> Result<(), DomainError> {
    let has_column = sqlx::query_scalar::<_, i32>(&format!(
        "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'"
    ))
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    if has_column > 0 {
        sqlx::query(&format!("DROP TABLE IF EXISTS {table}"))
            .execute(pool)
            .await
            .map_err(db_err)?;
    }
    Ok(())
}

fn ensure_rows_affected(
    rows_affected: u64,
    entity: &'static str,
    id: &uuid::Uuid,
) -> Result<(), DomainError> {
    if rows_affected == 0 {
        Err(DomainError::NotFound {
            entity,
            id: id.to_string(),
        })
    } else {
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct WorkflowDefinitionRow {
    id: String,
    key: String,
    name: String,
    description: String,
    target_kind: String,
    recommended_roles: String,
    source: String,
    status: String,
    version: i32,
    contract: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<WorkflowDefinitionRow> for WorkflowDefinition {
    type Error = DomainError;
    fn try_from(row: WorkflowDefinitionRow) -> Result<Self, Self::Error> {
        Ok(WorkflowDefinition {
            id: parse_uuid(&row.id, "workflow_definition")?,
            key: row.key,
            name: row.name,
            description: row.description,
            target_kind: serde_json::from_str(&row.target_kind)?,
            recommended_roles: serde_json::from_str(&row.recommended_roles).unwrap_or_default(),
            source: serde_json::from_str(&row.source)?,
            status: serde_json::from_str(&row.status)?,
            version: row.version,
            contract: serde_json::from_str(&row.contract)?,
            created_at: parse_time(&row.created_at),
            updated_at: parse_time(&row.updated_at),
        })
    }
}

#[derive(sqlx::FromRow)]
struct LifecycleDefinitionRow {
    id: String,
    key: String,
    name: String,
    description: String,
    target_kind: String,
    recommended_roles: String,
    source: String,
    status: String,
    version: i32,
    entry_step_key: String,
    steps: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<LifecycleDefinitionRow> for LifecycleDefinition {
    type Error = DomainError;
    fn try_from(row: LifecycleDefinitionRow) -> Result<Self, Self::Error> {
        Ok(LifecycleDefinition {
            id: parse_uuid(&row.id, "lifecycle_definition")?,
            key: row.key,
            name: row.name,
            description: row.description,
            target_kind: serde_json::from_str(&row.target_kind)?,
            recommended_roles: serde_json::from_str(&row.recommended_roles).unwrap_or_default(),
            source: serde_json::from_str(&row.source)?,
            status: serde_json::from_str(&row.status)?,
            version: row.version,
            entry_step_key: row.entry_step_key,
            steps: serde_json::from_str(&row.steps)?,
            created_at: parse_time(&row.created_at),
            updated_at: parse_time(&row.updated_at),
        })
    }
}

#[derive(sqlx::FromRow)]
struct WorkflowAssignmentRow {
    id: String,
    project_id: String,
    lifecycle_id: String,
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
            id: parse_uuid(&row.id, "workflow_assignment")?,
            project_id: parse_uuid(&row.project_id, "project")?,
            lifecycle_id: parse_uuid(&row.lifecycle_id, "lifecycle_definition")?,
            role: serde_json::from_str(&row.role)?,
            enabled: row.enabled,
            is_default: row.is_default,
            created_at: parse_time(&row.created_at),
            updated_at: parse_time(&row.updated_at),
        })
    }
}

#[derive(sqlx::FromRow)]
struct LifecycleRunRow {
    id: String,
    project_id: String,
    lifecycle_id: String,
    target_kind: String,
    target_id: String,
    status: String,
    current_step_key: Option<String>,
    step_states: String,
    record_artifacts: String,
    created_at: String,
    updated_at: String,
    last_activity_at: String,
}

impl TryFrom<LifecycleRunRow> for LifecycleRun {
    type Error = DomainError;
    fn try_from(row: LifecycleRunRow) -> Result<Self, Self::Error> {
        Ok(LifecycleRun {
            id: parse_uuid(&row.id, "lifecycle_run")?,
            project_id: parse_uuid(&row.project_id, "project")?,
            lifecycle_id: parse_uuid(&row.lifecycle_id, "lifecycle_definition")?,
            target_kind: serde_json::from_str(&row.target_kind)?,
            target_id: parse_uuid(&row.target_id, "workflow_target")?,
            status: serde_json::from_str(&row.status)?,
            current_step_key: row.current_step_key,
            step_states: serde_json::from_str(&row.step_states)?,
            record_artifacts: serde_json::from_str(&row.record_artifacts)?,
            created_at: parse_time(&row.created_at),
            updated_at: parse_time(&row.updated_at),
            last_activity_at: parse_time(&row.last_activity_at),
        })
    }
}

fn parse_uuid(raw: &str, entity: &'static str) -> Result<uuid::Uuid, DomainError> {
    raw.parse().map_err(|_| DomainError::NotFound {
        entity,
        id: raw.to_string(),
    })
}

fn parse_time(raw: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}
