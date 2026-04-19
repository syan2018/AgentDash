use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    LifecycleDefinition, LifecycleDefinitionRepository, LifecycleRun, LifecycleRunRepository,
    WorkflowBindingKind,
    WorkflowDefinition, WorkflowDefinitionRepository,
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
            id TEXT PRIMARY KEY, project_id TEXT NOT NULL, key TEXT NOT NULL,
            name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
            binding_kind TEXT NOT NULL, recommended_binding_roles TEXT NOT NULL DEFAULT '[]',
            source TEXT NOT NULL, version INTEGER NOT NULL, contract TEXT NOT NULL,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(project_id, key)
        )"#)
        .execute(&self.pool).await.map_err(db_err)?;

        sqlx::query(r#"CREATE TABLE IF NOT EXISTS lifecycle_definitions (
            id TEXT PRIMARY KEY, project_id TEXT NOT NULL, key TEXT NOT NULL,
            name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
            binding_kind TEXT NOT NULL, recommended_binding_roles TEXT NOT NULL DEFAULT '[]',
            source TEXT NOT NULL, version INTEGER NOT NULL,
            entry_step_key TEXT NOT NULL, steps TEXT NOT NULL, edges TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(project_id, key)
        )"#)
        .execute(&self.pool).await.map_err(db_err)?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS lifecycle_runs (
            id TEXT PRIMARY KEY, project_id TEXT NOT NULL, lifecycle_id TEXT NOT NULL,
            session_id TEXT NOT NULL DEFAULT '', status TEXT NOT NULL,
            step_states TEXT NOT NULL,
            record_artifacts TEXT NOT NULL, execution_log TEXT NOT NULL DEFAULT '[]',
            port_outputs TEXT NOT NULL DEFAULT '{}',
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

const WF_COLS: &str = "id,project_id,key,name,description,binding_kind,recommended_binding_roles,source,version,contract,created_at,updated_at";
const LC_COLS: &str = "id,project_id,key,name,description,binding_kind,recommended_binding_roles,source,version,entry_step_key,steps,edges,created_at,updated_at";
const RUN_COLS: &str = "id,project_id,lifecycle_id,session_id,status,step_states,record_artifacts,execution_log,port_outputs,created_at,updated_at,last_activity_at";

#[async_trait::async_trait]
impl WorkflowDefinitionRepository for PostgresWorkflowRepository {
    async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        sqlx::query(&format!("INSERT INTO workflow_definitions ({WF_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"))
            .bind(workflow.id.to_string()).bind(workflow.project_id.to_string())
            .bind(&workflow.key).bind(&workflow.name).bind(&workflow.description)
            .bind(serde_json::to_string(&workflow.binding_kind)?)
            .bind(serde_json::to_string(&workflow.recommended_binding_roles)?)
            .bind(serde_json::to_string(&workflow.source)?)
            .bind(workflow.version).bind(serde_json::to_string(&workflow.contract)?)
            .bind(workflow.created_at.to_rfc3339()).bind(workflow.updated_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!("SELECT {WF_COLS} FROM workflow_definitions WHERE id = $1"))
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!("SELECT {WF_COLS} FROM workflow_definitions WHERE key = $1 LIMIT 1"))
            .bind(key).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn get_by_project_and_key(
        &self,
        project_id: uuid::Uuid,
        key: &str,
    ) -> Result<Option<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!("SELECT {WF_COLS} FROM workflow_definitions WHERE project_id = $1 AND key = $2"))
            .bind(project_id.to_string()).bind(key)
            .fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!("SELECT {WF_COLS} FROM workflow_definitions ORDER BY created_at DESC"))
            .fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!("SELECT {WF_COLS} FROM workflow_definitions WHERE project_id = $1 ORDER BY created_at DESC"))
            .bind(project_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_binding_kind(
        &self,
        binding_kind: WorkflowBindingKind,
    ) -> Result<Vec<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!("SELECT {WF_COLS} FROM workflow_definitions WHERE binding_kind = $1 ORDER BY created_at DESC"))
            .bind(serde_json::to_string(&binding_kind)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE workflow_definitions SET project_id=$1,key=$2,name=$3,description=$4,binding_kind=$5,recommended_binding_roles=$6,source=$7,version=$8,contract=$9,updated_at=$10 WHERE id=$11")
            .bind(workflow.project_id.to_string())
            .bind(&workflow.key).bind(&workflow.name).bind(&workflow.description)
            .bind(serde_json::to_string(&workflow.binding_kind)?)
            .bind(serde_json::to_string(&workflow.recommended_binding_roles)?)
            .bind(serde_json::to_string(&workflow.source)?)
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
        sqlx::query(&format!("INSERT INTO lifecycle_definitions ({LC_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)"))
            .bind(lifecycle.id.to_string()).bind(lifecycle.project_id.to_string())
            .bind(&lifecycle.key).bind(&lifecycle.name).bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.binding_kind)?)
            .bind(serde_json::to_string(&lifecycle.recommended_binding_roles)?)
            .bind(serde_json::to_string(&lifecycle.source)?)
            .bind(lifecycle.version).bind(&lifecycle.entry_step_key).bind(serde_json::to_string(&lifecycle.steps)?)
            .bind(serde_json::to_string(&lifecycle.edges)?)
            .bind(lifecycle.created_at.to_rfc3339()).bind(lifecycle.updated_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!("SELECT {LC_COLS} FROM lifecycle_definitions WHERE id = $1"))
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!("SELECT {LC_COLS} FROM lifecycle_definitions WHERE key = $1 LIMIT 1"))
            .bind(key).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn get_by_project_and_key(
        &self,
        project_id: uuid::Uuid,
        key: &str,
    ) -> Result<Option<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!("SELECT {LC_COLS} FROM lifecycle_definitions WHERE project_id = $1 AND key = $2"))
            .bind(project_id.to_string()).bind(key)
            .fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_all(&self) -> Result<Vec<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!("SELECT {LC_COLS} FROM lifecycle_definitions ORDER BY created_at DESC"))
            .fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!("SELECT {LC_COLS} FROM lifecycle_definitions WHERE project_id = $1 ORDER BY created_at DESC"))
            .bind(project_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_binding_kind(
        &self,
        binding_kind: WorkflowBindingKind,
    ) -> Result<Vec<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!("SELECT {LC_COLS} FROM lifecycle_definitions WHERE binding_kind = $1 ORDER BY created_at DESC"))
            .bind(serde_json::to_string(&binding_kind)?).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, lifecycle: &LifecycleDefinition) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE lifecycle_definitions SET project_id=$1,key=$2,name=$3,description=$4,binding_kind=$5,recommended_binding_roles=$6,source=$7,version=$8,entry_step_key=$9,steps=$10,edges=$11,updated_at=$12 WHERE id=$13")
            .bind(lifecycle.project_id.to_string())
            .bind(&lifecycle.key).bind(&lifecycle.name).bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.binding_kind)?)
            .bind(serde_json::to_string(&lifecycle.recommended_binding_roles)?)
            .bind(serde_json::to_string(&lifecycle.source)?)
            .bind(lifecycle.version).bind(&lifecycle.entry_step_key).bind(serde_json::to_string(&lifecycle.steps)?)
            .bind(serde_json::to_string(&lifecycle.edges)?)
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
impl LifecycleRunRepository for PostgresWorkflowRepository {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        sqlx::query(&format!("INSERT INTO lifecycle_runs ({RUN_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"))
            .bind(run.id.to_string()).bind(run.project_id.to_string()).bind(run.lifecycle_id.to_string())
            .bind(&run.session_id).bind(serde_json::to_string(&run.status)?)
            .bind(serde_json::to_string(&run.step_states)?)
            .bind("[]").bind(serde_json::to_string(&run.execution_log)?)
            .bind("{}")
            .bind(run.created_at.to_rfc3339()).bind(run.updated_at.to_rfc3339()).bind(run.last_activity_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>(&format!("SELECT {RUN_COLS} FROM lifecycle_runs WHERE id = $1"))
            .bind(id.to_string()).fetch_optional(&self.pool).await.map_err(db_err)?
            .map(TryInto::try_into).transpose()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>(&format!("SELECT {RUN_COLS} FROM lifecycle_runs WHERE project_id = $1 ORDER BY created_at DESC"))
            .bind(project_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_lifecycle(
        &self,
        lifecycle_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>(&format!("SELECT {RUN_COLS} FROM lifecycle_runs WHERE lifecycle_id = $1 ORDER BY created_at DESC"))
            .bind(lifecycle_id.to_string()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn list_by_session(&self, session_id: &str) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>(&format!("SELECT {RUN_COLS} FROM lifecycle_runs WHERE session_id = $1 ORDER BY created_at DESC"))
            .bind(session_id).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE lifecycle_runs SET project_id=$1,lifecycle_id=$2,session_id=$3,status=$4,step_states=$5,record_artifacts=$6,execution_log=$7,port_outputs=$8,updated_at=$9,last_activity_at=$10 WHERE id=$11")
            .bind(run.project_id.to_string()).bind(run.lifecycle_id.to_string()).bind(&run.session_id)
            .bind(serde_json::to_string(&run.status)?)
            .bind(serde_json::to_string(&run.step_states)?).bind("[]")
            .bind(serde_json::to_string(&run.execution_log)?).bind("{}")
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
    project_id: String,
    key: String,
    name: String,
    description: String,
    binding_kind: String,
    recommended_binding_roles: String,
    source: String,
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
            project_id: parse_uuid(&row.project_id, "project")?,
            key: row.key,
            name: row.name,
            description: row.description,
            binding_kind: serde_json::from_str(&row.binding_kind)?,
            recommended_binding_roles: parse_json_column(
                &row.recommended_binding_roles,
                "workflow_definitions.recommended_binding_roles",
            )?,
            source: serde_json::from_str(&row.source)?,
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
    project_id: String,
    key: String,
    name: String,
    description: String,
    binding_kind: String,
    recommended_binding_roles: String,
    source: String,
    version: i32,
    entry_step_key: String,
    steps: String,
    edges: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<LifecycleDefinitionRow> for LifecycleDefinition {
    type Error = DomainError;
    fn try_from(row: LifecycleDefinitionRow) -> Result<Self, Self::Error> {
        Ok(LifecycleDefinition {
            id: parse_uuid(&row.id, "lifecycle_definition")?,
            project_id: parse_uuid(&row.project_id, "project")?,
            key: row.key,
            name: row.name,
            description: row.description,
            binding_kind: serde_json::from_str(&row.binding_kind)?,
            recommended_binding_roles: parse_json_column(
                &row.recommended_binding_roles,
                "lifecycle_definitions.recommended_binding_roles",
            )?,
            source: serde_json::from_str(&row.source)?,
            version: row.version,
            entry_step_key: row.entry_step_key,
            steps: serde_json::from_str(&row.steps)?,
            edges: parse_json_column(&row.edges, "lifecycle_definitions.edges")?,
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
    step_states: String,
    #[allow(dead_code)]
    record_artifacts: String,
    execution_log: String,
    #[allow(dead_code)]
    port_outputs: String,
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
            active_node_keys,
            step_states,
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
