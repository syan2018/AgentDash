use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, LifecycleRun, LifecycleRunRepository,
    WorkflowAssignment, WorkflowAssignmentRepository, WorkflowBindingKind, WorkflowBindingRole,
    WorkflowDefinition, WorkflowDefinitionRepository, WorkflowDefinitionStatus,
};

pub struct PostgresWorkflowRepository {
    pool: PgPool,
}

impl PostgresWorkflowRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(r#"CREATE TABLE IF NOT EXISTS workflow_definitions (
            id TEXT PRIMARY KEY, key TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '', binding_kind TEXT NOT NULL, recommended_binding_roles TEXT NOT NULL DEFAULT '[]',
            source TEXT NOT NULL, status TEXT NOT NULL, version INTEGER NOT NULL, contract TEXT NOT NULL,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL
        )"#)
        .execute(&self.pool).await.map_err(db_err)?;

        sqlx::query(r#"CREATE TABLE IF NOT EXISTS lifecycle_definitions (
            id TEXT PRIMARY KEY, key TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '', binding_kind TEXT NOT NULL, recommended_binding_roles TEXT NOT NULL DEFAULT '[]',
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
            session_id TEXT NOT NULL DEFAULT '', status TEXT NOT NULL,
            current_step_key TEXT, step_states TEXT NOT NULL,
            record_artifacts TEXT NOT NULL, execution_log TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
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
impl WorkflowDefinitionRepository for PostgresWorkflowRepository {
    async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO workflow_definitions (id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,contract,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(workflow.id.to_string()).bind(&workflow.key).bind(&workflow.name).bind(&workflow.description)
            .bind(serde_json::to_string(&workflow.binding_kind)?)
            .bind(serde_json::to_string(&workflow.recommended_binding_roles)?)
            .bind(serde_json::to_string(&workflow.source)?).bind(serde_json::to_string(&workflow.status)?)
            .bind(workflow.version).bind(serde_json::to_string(&workflow.contract)?)
            .bind(workflow.created_at.to_rfc3339()).bind(workflow.updated_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>("SELECT id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,contract,created_at,updated_at FROM workflow_definitions WHERE id = $1")
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>("SELECT id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,contract,created_at,updated_at FROM workflow_definitions WHERE key = $1")
            .bind(key).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>("SELECT id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,contract,created_at,updated_at FROM workflow_definitions ORDER BY created_at DESC")
            .fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_status(
        &self,
        status: WorkflowDefinitionStatus,
    ) -> Result<Vec<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>("SELECT id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,contract,created_at,updated_at FROM workflow_definitions WHERE status = $1 ORDER BY created_at DESC")
            .bind(serde_json::to_string(&status)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_binding_kind(
        &self,
        binding_kind: WorkflowBindingKind,
    ) -> Result<Vec<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>("SELECT id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,contract,created_at,updated_at FROM workflow_definitions WHERE binding_kind = $1 ORDER BY created_at DESC")
            .bind(serde_json::to_string(&binding_kind)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE workflow_definitions SET key=$1,name=$2,description=$3,binding_kind=$4,recommended_binding_roles=$5,source=$6,status=$7,version=$8,contract=$9,updated_at=$10 WHERE id=$11")
            .bind(&workflow.key).bind(&workflow.name).bind(&workflow.description)
            .bind(serde_json::to_string(&workflow.binding_kind)?)
            .bind(serde_json::to_string(&workflow.recommended_binding_roles)?)
            .bind(serde_json::to_string(&workflow.source)?).bind(serde_json::to_string(&workflow.status)?)
            .bind(workflow.version).bind(serde_json::to_string(&workflow.contract)?)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(workflow.id.to_string()).execute(&self.pool).await.map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "workflow_definition", &workflow.id)
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM workflow_definitions WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "workflow_definition", &id)
    }
}

#[async_trait::async_trait]
impl LifecycleDefinitionRepository for PostgresWorkflowRepository {
    async fn create(&self, lifecycle: &LifecycleDefinition) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO lifecycle_definitions (id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,entry_step_key,steps,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
            .bind(lifecycle.id.to_string()).bind(&lifecycle.key).bind(&lifecycle.name).bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.binding_kind)?)
            .bind(serde_json::to_string(&lifecycle.recommended_binding_roles)?)
            .bind(serde_json::to_string(&lifecycle.source)?).bind(serde_json::to_string(&lifecycle.status)?)
            .bind(lifecycle.version).bind(&lifecycle.entry_step_key).bind(serde_json::to_string(&lifecycle.steps)?)
            .bind(lifecycle.created_at.to_rfc3339()).bind(lifecycle.updated_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>("SELECT id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,entry_step_key,steps,created_at,updated_at FROM lifecycle_definitions WHERE id = $1")
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>("SELECT id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,entry_step_key,steps,created_at,updated_at FROM lifecycle_definitions WHERE key = $1")
            .bind(key).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_all(&self) -> Result<Vec<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>("SELECT id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,entry_step_key,steps,created_at,updated_at FROM lifecycle_definitions ORDER BY created_at DESC")
            .fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_status(
        &self,
        status: WorkflowDefinitionStatus,
    ) -> Result<Vec<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>("SELECT id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,entry_step_key,steps,created_at,updated_at FROM lifecycle_definitions WHERE status = $1 ORDER BY created_at DESC")
            .bind(serde_json::to_string(&status)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_binding_kind(
        &self,
        binding_kind: WorkflowBindingKind,
    ) -> Result<Vec<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>("SELECT id,key,name,description,binding_kind,recommended_binding_roles,source,status,version,entry_step_key,steps,created_at,updated_at FROM lifecycle_definitions WHERE binding_kind = $1 ORDER BY created_at DESC")
            .bind(serde_json::to_string(&binding_kind)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, lifecycle: &LifecycleDefinition) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE lifecycle_definitions SET key=$1,name=$2,description=$3,binding_kind=$4,recommended_binding_roles=$5,source=$6,status=$7,version=$8,entry_step_key=$9,steps=$10,updated_at=$11 WHERE id=$12")
            .bind(&lifecycle.key).bind(&lifecycle.name).bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.binding_kind)?)
            .bind(serde_json::to_string(&lifecycle.recommended_binding_roles)?)
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
        let result = sqlx::query("DELETE FROM lifecycle_definitions WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "lifecycle_definition", &id)
    }
}

#[async_trait::async_trait]
impl WorkflowAssignmentRepository for PostgresWorkflowRepository {
    async fn create(&self, assignment: &WorkflowAssignment) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO workflow_assignments (id,project_id,lifecycle_id,role,enabled,is_default,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(assignment.id.to_string()).bind(assignment.project_id.to_string()).bind(assignment.lifecycle_id.to_string())
            .bind(serde_json::to_string(&assignment.role)?).bind(assignment.enabled).bind(assignment.is_default)
            .bind(assignment.created_at.to_rfc3339()).bind(assignment.updated_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowAssignment>, DomainError> {
        sqlx::query_as::<_, WorkflowAssignmentRow>("SELECT id,project_id,lifecycle_id,role,enabled,is_default,created_at,updated_at FROM workflow_assignments WHERE id = $1")
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<WorkflowAssignment>, DomainError> {
        sqlx::query_as::<_, WorkflowAssignmentRow>("SELECT id,project_id,lifecycle_id,role,enabled,is_default,created_at,updated_at FROM workflow_assignments WHERE project_id = $1 ORDER BY created_at DESC")
            .bind(project_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_project_and_role(
        &self,
        project_id: uuid::Uuid,
        role: WorkflowBindingRole,
    ) -> Result<Vec<WorkflowAssignment>, DomainError> {
        sqlx::query_as::<_, WorkflowAssignmentRow>("SELECT id,project_id,lifecycle_id,role,enabled,is_default,created_at,updated_at FROM workflow_assignments WHERE project_id = $1 AND role = $2 ORDER BY created_at DESC")
            .bind(project_id.to_string()).bind(serde_json::to_string(&role)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, assignment: &WorkflowAssignment) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE workflow_assignments SET project_id=$1,lifecycle_id=$2,role=$3,enabled=$4,is_default=$5,updated_at=$6 WHERE id=$7")
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
        let result = sqlx::query("DELETE FROM workflow_assignments WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "workflow_assignment", &id)
    }
}

#[async_trait::async_trait]
impl LifecycleRunRepository for PostgresWorkflowRepository {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        sqlx::query("INSERT INTO lifecycle_runs (id,project_id,lifecycle_id,session_id,status,current_step_key,step_states,record_artifacts,execution_log,created_at,updated_at,last_activity_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(run.id.to_string()).bind(run.project_id.to_string()).bind(run.lifecycle_id.to_string())
            .bind(&run.session_id).bind(serde_json::to_string(&run.status)?)
            .bind(&run.current_step_key).bind(serde_json::to_string(&run.step_states)?)
            .bind(serde_json::to_string(&run.record_artifacts)?).bind(serde_json::to_string(&run.execution_log)?)
            .bind(run.created_at.to_rfc3339()).bind(run.updated_at.to_rfc3339()).bind(run.last_activity_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>("SELECT id,project_id,lifecycle_id,session_id,status,current_step_key,step_states,record_artifacts,execution_log,created_at,updated_at,last_activity_at FROM lifecycle_runs WHERE id = $1")
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>("SELECT id,project_id,lifecycle_id,session_id,status,current_step_key,step_states,record_artifacts,execution_log,created_at,updated_at,last_activity_at FROM lifecycle_runs WHERE project_id = $1 ORDER BY created_at DESC")
            .bind(project_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_lifecycle(
        &self,
        lifecycle_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>("SELECT id,project_id,lifecycle_id,session_id,status,current_step_key,step_states,record_artifacts,execution_log,created_at,updated_at,last_activity_at FROM lifecycle_runs WHERE lifecycle_id = $1 ORDER BY created_at DESC")
            .bind(lifecycle_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_session(&self, session_id: &str) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>("SELECT id,project_id,lifecycle_id,session_id,status,current_step_key,step_states,record_artifacts,execution_log,created_at,updated_at,last_activity_at FROM lifecycle_runs WHERE session_id = $1 ORDER BY created_at DESC")
            .bind(session_id).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE lifecycle_runs SET project_id=$1,lifecycle_id=$2,session_id=$3,status=$4,current_step_key=$5,step_states=$6,record_artifacts=$7,execution_log=$8,updated_at=$9,last_activity_at=$10 WHERE id=$11")
            .bind(run.project_id.to_string()).bind(run.lifecycle_id.to_string()).bind(&run.session_id)
            .bind(serde_json::to_string(&run.status)?).bind(&run.current_step_key)
            .bind(serde_json::to_string(&run.step_states)?).bind(serde_json::to_string(&run.record_artifacts)?)
            .bind(serde_json::to_string(&run.execution_log)?)
            .bind(chrono::Utc::now().to_rfc3339()).bind(run.last_activity_at.to_rfc3339()).bind(run.id.to_string())
            .execute(&self.pool).await.map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "lifecycle_run", &run.id)
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM lifecycle_runs WHERE id = $1")
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
    binding_kind: String,
    recommended_binding_roles: String,
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
            binding_kind: serde_json::from_str(&row.binding_kind)?,
            recommended_binding_roles: parse_json_column(
                &row.recommended_binding_roles,
                "workflow_definitions.recommended_binding_roles",
            )?,
            source: serde_json::from_str(&row.source)?,
            status: serde_json::from_str(&row.status)?,
            version: row.version,
            contract: serde_json::from_str(&row.contract)?,
            created_at: parse_time(&row.created_at)?,
            updated_at: parse_time(&row.updated_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct LifecycleDefinitionRow {
    id: String,
    key: String,
    name: String,
    description: String,
    binding_kind: String,
    recommended_binding_roles: String,
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
            binding_kind: serde_json::from_str(&row.binding_kind)?,
            recommended_binding_roles: parse_json_column(
                &row.recommended_binding_roles,
                "lifecycle_definitions.recommended_binding_roles",
            )?,
            source: serde_json::from_str(&row.source)?,
            status: serde_json::from_str(&row.status)?,
            version: row.version,
            entry_step_key: row.entry_step_key,
            steps: serde_json::from_str(&row.steps)?,
            created_at: parse_time(&row.created_at)?,
            updated_at: parse_time(&row.updated_at)?,
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
            created_at: parse_time(&row.created_at)?,
            updated_at: parse_time(&row.updated_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct LifecycleRunRow {
    id: String,
    project_id: String,
    lifecycle_id: String,
    session_id: String,
    status: String,
    current_step_key: Option<String>,
    step_states: String,
    record_artifacts: String,
    execution_log: String,
    created_at: String,
    updated_at: String,
    last_activity_at: String,
}

impl TryFrom<LifecycleRunRow> for LifecycleRun {
    type Error = DomainError;
    fn try_from(row: LifecycleRunRow) -> Result<Self, Self::Error> {
        let step_states: Vec<agentdash_domain::workflow::LifecycleStepState> =
            serde_json::from_str(&row.step_states)?;
        let active_node_keys: Vec<String> = step_states
            .iter()
            .filter(|s| {
                matches!(
                    s.status,
                    agentdash_domain::workflow::LifecycleStepExecutionStatus::Ready
                        | agentdash_domain::workflow::LifecycleStepExecutionStatus::Running
                )
            })
            .map(|s| s.step_key.clone())
            .collect();
        Ok(LifecycleRun {
            id: parse_uuid(&row.id, "lifecycle_run")?,
            project_id: parse_uuid(&row.project_id, "project")?,
            lifecycle_id: parse_uuid(&row.lifecycle_id, "lifecycle_definition")?,
            session_id: row.session_id,
            status: serde_json::from_str(&row.status)?,
            current_step_key: row.current_step_key,
            active_node_keys,
            step_states,
            record_artifacts: serde_json::from_str(&row.record_artifacts)?,
            execution_log: parse_json_column(&row.execution_log, "lifecycle_runs.execution_log")?,
            created_at: parse_time(&row.created_at)?,
            updated_at: parse_time(&row.updated_at)?,
            last_activity_at: parse_time(&row.last_activity_at)?,
        })
    }
}

fn parse_uuid(raw: &str, entity: &'static str) -> Result<uuid::Uuid, DomainError> {
    raw.parse().map_err(|_| DomainError::NotFound {
        entity,
        id: raw.to_string(),
    })
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn parse_time(raw: &str) -> Result<chrono::DateTime<chrono::Utc>, DomainError> {
    super::parse_pg_timestamp_checked(raw, "workflow.timestamp")
}
