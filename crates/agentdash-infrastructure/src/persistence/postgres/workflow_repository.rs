use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::shared_library::InstalledAssetSource;
use agentdash_domain::workflow::{
    ActivityLifecycleDefinition, ActivityLifecycleDefinitionRepository, LifecycleDefinition,
    LifecycleDefinitionRepository, LifecycleRun, LifecycleRunRepository, WorkflowBindingKind,
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
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS workflow_definitions (
            id TEXT PRIMARY KEY, project_id TEXT NOT NULL, key TEXT NOT NULL,
            name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
            binding_kinds TEXT NOT NULL DEFAULT '["story"]',
            source TEXT NOT NULL, version INTEGER NOT NULL, contract TEXT NOT NULL,
            library_asset_id TEXT, source_ref TEXT, source_version TEXT, source_digest TEXT, installed_at TEXT,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(project_id, key)
        )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS lifecycle_definitions (
            id TEXT PRIMARY KEY, project_id TEXT NOT NULL, key TEXT NOT NULL,
            name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
            binding_kinds TEXT NOT NULL DEFAULT '["story"]',
            source TEXT NOT NULL, version INTEGER NOT NULL,
            entry_step_key TEXT NOT NULL, steps TEXT NOT NULL, edges TEXT NOT NULL DEFAULT '[]',
            entry_activity_key TEXT NOT NULL DEFAULT '', activities TEXT NOT NULL DEFAULT '[]',
            transitions TEXT NOT NULL DEFAULT '[]',
            library_asset_id TEXT, source_ref TEXT, source_version TEXT, source_digest TEXT, installed_at TEXT,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(project_id, key)
        )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS lifecycle_runs (
            id TEXT PRIMARY KEY, project_id TEXT NOT NULL, lifecycle_id TEXT NOT NULL,
            session_id TEXT NOT NULL DEFAULT '', status TEXT NOT NULL,
            step_states TEXT NOT NULL, record_artifacts TEXT NOT NULL DEFAULT '{}',
            execution_log TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            last_activity_at TEXT NOT NULL
        )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_lifecycle_runs_project_id ON lifecycle_runs(project_id)")
            .execute(&self.pool).await.map_err(db_err)?;

        add_installed_source_columns(&self.pool).await?;
        add_lifecycle_run_columns(&self.pool).await?;
        add_activity_lifecycle_columns(&self.pool).await?;

        Ok(())
    }
}

const WF_COLS: &str = "id,project_id,key,name,description,binding_kinds,source,version,contract,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at";
const LC_COLS: &str = "id,project_id,key,name,description,binding_kinds,source,version,entry_step_key,steps,edges,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at";
const ACTIVITY_LC_COLS: &str = "id,project_id,key,name,description,binding_kinds,source,version,entry_activity_key,activities,transitions,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at";
const RUN_COLS: &str = "id,project_id,lifecycle_id,session_id,status,step_states,execution_log,created_at,updated_at,last_activity_at";
const RUN_INSERT_COLS: &str = "id,project_id,lifecycle_id,session_id,status,step_states,record_artifacts,execution_log,created_at,updated_at,last_activity_at";

#[async_trait::async_trait]
impl WorkflowDefinitionRepository for PostgresWorkflowRepository {
    async fn create(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        sqlx::query(&format!("INSERT INTO workflow_definitions ({WF_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)"))
            .bind(workflow.id.to_string()).bind(workflow.project_id.to_string())
            .bind(&workflow.key).bind(&workflow.name).bind(&workflow.description)
            .bind(serde_json::to_string(&workflow.binding_kinds)?)
            .bind(serde_json::to_string(&workflow.source)?)
            .bind(workflow.version).bind(serde_json::to_string(&workflow.contract)?)
            .bind(installed_library_asset_id(&workflow.installed_source))
            .bind(installed_source_ref(&workflow.installed_source))
            .bind(installed_source_version(&workflow.installed_source))
            .bind(installed_source_digest(&workflow.installed_source))
            .bind(installed_at(&workflow.installed_source))
            .bind(workflow.created_at.to_rfc3339()).bind(workflow.updated_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!(
            "SELECT {WF_COLS} FROM workflow_definitions WHERE id = $1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!(
            "SELECT {WF_COLS} FROM workflow_definitions WHERE key = $1 LIMIT 1"
        ))
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn get_by_project_and_key(
        &self,
        project_id: uuid::Uuid,
        key: &str,
    ) -> Result<Option<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!(
            "SELECT {WF_COLS} FROM workflow_definitions WHERE project_id = $1 AND key = $2"
        ))
        .bind(project_id.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_all(&self) -> Result<Vec<WorkflowDefinition>, DomainError> {
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!(
            "SELECT {WF_COLS} FROM workflow_definitions ORDER BY created_at DESC"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
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
        sqlx::query_as::<_, WorkflowDefinitionRow>(&format!("SELECT {WF_COLS} FROM workflow_definitions WHERE binding_kinds::jsonb ? $1 ORDER BY created_at DESC"))
            .bind(binding_kind.binding_scope_key()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, workflow: &WorkflowDefinition) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE workflow_definitions SET project_id=$1,key=$2,name=$3,description=$4,binding_kinds=$5,source=$6,version=$7,contract=$8,library_asset_id=$9,source_ref=$10,source_version=$11,source_digest=$12,installed_at=$13,updated_at=$14 WHERE id=$15")
            .bind(workflow.project_id.to_string())
            .bind(&workflow.key).bind(&workflow.name).bind(&workflow.description)
            .bind(serde_json::to_string(&workflow.binding_kinds)?)
            .bind(serde_json::to_string(&workflow.source)?)
            .bind(workflow.version).bind(serde_json::to_string(&workflow.contract)?)
            .bind(installed_library_asset_id(&workflow.installed_source))
            .bind(installed_source_ref(&workflow.installed_source))
            .bind(installed_source_version(&workflow.installed_source))
            .bind(installed_source_digest(&workflow.installed_source))
            .bind(installed_at(&workflow.installed_source))
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
impl ActivityLifecycleDefinitionRepository for PostgresWorkflowRepository {
    async fn create(&self, lifecycle: &ActivityLifecycleDefinition) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO lifecycle_definitions (id,project_id,key,name,description,binding_kinds,source,version,entry_step_key,steps,edges,entry_activity_key,activities,transitions,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'[]','[]',$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)",
        )
        .bind(lifecycle.id.to_string())
        .bind(lifecycle.project_id.to_string())
        .bind(&lifecycle.key)
        .bind(&lifecycle.name)
        .bind(&lifecycle.description)
        .bind(serde_json::to_string(&lifecycle.binding_kinds)?)
        .bind(serde_json::to_string(&lifecycle.source)?)
        .bind(lifecycle.version)
        .bind(&lifecycle.entry_activity_key)
        .bind(&lifecycle.entry_activity_key)
        .bind(serde_json::to_string(&lifecycle.activities)?)
        .bind(serde_json::to_string(&lifecycle.transitions)?)
        .bind(installed_library_asset_id(&lifecycle.installed_source))
        .bind(installed_source_ref(&lifecycle.installed_source))
        .bind(installed_source_version(&lifecycle.installed_source))
        .bind(installed_source_digest(&lifecycle.installed_source))
        .bind(installed_at(&lifecycle.installed_source))
        .bind(lifecycle.created_at.to_rfc3339())
        .bind(lifecycle.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<ActivityLifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, ActivityLifecycleDefinitionRow>(&format!(
            "SELECT {ACTIVITY_LC_COLS} FROM lifecycle_definitions WHERE id = $1 AND entry_activity_key <> ''"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn get_by_project_and_key(
        &self,
        project_id: uuid::Uuid,
        key: &str,
    ) -> Result<Option<ActivityLifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, ActivityLifecycleDefinitionRow>(&format!(
            "SELECT {ACTIVITY_LC_COLS} FROM lifecycle_definitions WHERE project_id = $1 AND key = $2 AND entry_activity_key <> ''"
        ))
        .bind(project_id.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<ActivityLifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, ActivityLifecycleDefinitionRow>(&format!(
            "SELECT {ACTIVITY_LC_COLS} FROM lifecycle_definitions WHERE project_id = $1 AND entry_activity_key <> '' ORDER BY created_at DESC"
        ))
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn update(&self, lifecycle: &ActivityLifecycleDefinition) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE lifecycle_definitions SET project_id=$1,key=$2,name=$3,description=$4,binding_kinds=$5,source=$6,version=$7,entry_step_key=$8,entry_activity_key=$9,activities=$10,transitions=$11,library_asset_id=$12,source_ref=$13,source_version=$14,source_digest=$15,installed_at=$16,updated_at=$17 WHERE id=$18")
            .bind(lifecycle.project_id.to_string())
            .bind(&lifecycle.key)
            .bind(&lifecycle.name)
            .bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.binding_kinds)?)
            .bind(serde_json::to_string(&lifecycle.source)?)
            .bind(lifecycle.version)
            .bind(&lifecycle.entry_activity_key)
            .bind(&lifecycle.entry_activity_key)
            .bind(serde_json::to_string(&lifecycle.activities)?)
            .bind(serde_json::to_string(&lifecycle.transitions)?)
            .bind(installed_library_asset_id(&lifecycle.installed_source))
            .bind(installed_source_ref(&lifecycle.installed_source))
            .bind(installed_source_version(&lifecycle.installed_source))
            .bind(installed_source_digest(&lifecycle.installed_source))
            .bind(installed_at(&lifecycle.installed_source))
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(lifecycle.id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(
            result.rows_affected(),
            "activity_lifecycle_definition",
            &lifecycle.id,
        )
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query(
            "DELETE FROM lifecycle_definitions WHERE id = $1 AND entry_activity_key <> ''",
        )
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "activity_lifecycle_definition", &id)
    }
}

#[async_trait::async_trait]
impl LifecycleDefinitionRepository for PostgresWorkflowRepository {
    async fn create(&self, lifecycle: &LifecycleDefinition) -> Result<(), DomainError> {
        sqlx::query(&format!("INSERT INTO lifecycle_definitions ({LC_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)"))
            .bind(lifecycle.id.to_string()).bind(lifecycle.project_id.to_string())
            .bind(&lifecycle.key).bind(&lifecycle.name).bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.binding_kinds)?)
            .bind(serde_json::to_string(&lifecycle.source)?)
            .bind(lifecycle.version).bind(&lifecycle.entry_step_key).bind(serde_json::to_string(&lifecycle.steps)?)
            .bind(serde_json::to_string(&lifecycle.edges)?)
            .bind(installed_library_asset_id(&lifecycle.installed_source))
            .bind(installed_source_ref(&lifecycle.installed_source))
            .bind(installed_source_version(&lifecycle.installed_source))
            .bind(installed_source_digest(&lifecycle.installed_source))
            .bind(installed_at(&lifecycle.installed_source))
            .bind(lifecycle.created_at.to_rfc3339()).bind(lifecycle.updated_at.to_rfc3339())
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!(
            "SELECT {LC_COLS} FROM lifecycle_definitions WHERE id = $1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!(
            "SELECT {LC_COLS} FROM lifecycle_definitions WHERE key = $1 LIMIT 1"
        ))
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn get_by_project_and_key(
        &self,
        project_id: uuid::Uuid,
        key: &str,
    ) -> Result<Option<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!(
            "SELECT {LC_COLS} FROM lifecycle_definitions WHERE project_id = $1 AND key = $2"
        ))
        .bind(project_id.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_all(&self) -> Result<Vec<LifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!(
            "SELECT {LC_COLS} FROM lifecycle_definitions ORDER BY created_at DESC"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
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
        sqlx::query_as::<_, LifecycleDefinitionRow>(&format!("SELECT {LC_COLS} FROM lifecycle_definitions WHERE binding_kinds::jsonb ? $1 ORDER BY created_at DESC"))
            .bind(binding_kind.binding_scope_key()).fetch_all(&self.pool).await.map_err(db_err)?
            .into_iter().map(TryInto::try_into).collect()
    }

    async fn update(&self, lifecycle: &LifecycleDefinition) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE lifecycle_definitions SET project_id=$1,key=$2,name=$3,description=$4,binding_kinds=$5,source=$6,version=$7,entry_step_key=$8,steps=$9,edges=$10,library_asset_id=$11,source_ref=$12,source_version=$13,source_digest=$14,installed_at=$15,updated_at=$16 WHERE id=$17")
            .bind(lifecycle.project_id.to_string())
            .bind(&lifecycle.key).bind(&lifecycle.name).bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.binding_kinds)?)
            .bind(serde_json::to_string(&lifecycle.source)?)
            .bind(lifecycle.version).bind(&lifecycle.entry_step_key).bind(serde_json::to_string(&lifecycle.steps)?)
            .bind(serde_json::to_string(&lifecycle.edges)?)
            .bind(installed_library_asset_id(&lifecycle.installed_source))
            .bind(installed_source_ref(&lifecycle.installed_source))
            .bind(installed_source_version(&lifecycle.installed_source))
            .bind(installed_source_digest(&lifecycle.installed_source))
            .bind(installed_at(&lifecycle.installed_source))
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
        sqlx::query(&format!(
            "INSERT INTO lifecycle_runs ({RUN_INSERT_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"
        ))
        .bind(run.id.to_string())
        .bind(run.project_id.to_string())
        .bind(run.lifecycle_id.to_string())
        .bind(&run.session_id)
        .bind(serde_json::to_string(&run.status)?)
        .bind(serde_json::to_string(&run.step_states)?)
        .bind("{}")
        .bind(serde_json::to_string(&run.execution_log)?)
        .bind(run.created_at.to_rfc3339())
        .bind(run.updated_at.to_rfc3339())
        .bind(run.last_activity_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>(&format!(
            "SELECT {RUN_COLS} FROM lifecycle_runs WHERE id = $1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_by_project(
        &self,
        project_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>(&format!(
            "SELECT {RUN_COLS} FROM lifecycle_runs WHERE project_id = $1 ORDER BY created_at DESC"
        ))
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_by_lifecycle(
        &self,
        lifecycle_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>(&format!(
            "SELECT {RUN_COLS} FROM lifecycle_runs WHERE lifecycle_id = $1 ORDER BY created_at DESC"
        ))
        .bind(lifecycle_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_by_session(&self, session_id: &str) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>(&format!(
            "SELECT {RUN_COLS} FROM lifecycle_runs WHERE session_id = $1 ORDER BY created_at DESC"
        ))
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE lifecycle_runs SET project_id=$1,lifecycle_id=$2,session_id=$3,status=$4,step_states=$5,execution_log=$6,updated_at=$7,last_activity_at=$8 WHERE id=$9")
            .bind(run.project_id.to_string()).bind(run.lifecycle_id.to_string()).bind(&run.session_id)
            .bind(serde_json::to_string(&run.status)?)
            .bind(serde_json::to_string(&run.step_states)?)
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

async fn add_installed_source_columns(pool: &PgPool) -> Result<(), DomainError> {
    for query in [
        "ALTER TABLE workflow_definitions ADD COLUMN IF NOT EXISTS library_asset_id TEXT",
        "ALTER TABLE workflow_definitions ADD COLUMN IF NOT EXISTS source_ref TEXT",
        "ALTER TABLE workflow_definitions ADD COLUMN IF NOT EXISTS source_version TEXT",
        "ALTER TABLE workflow_definitions ADD COLUMN IF NOT EXISTS source_digest TEXT",
        "ALTER TABLE workflow_definitions ADD COLUMN IF NOT EXISTS installed_at TEXT",
        "ALTER TABLE lifecycle_definitions ADD COLUMN IF NOT EXISTS library_asset_id TEXT",
        "ALTER TABLE lifecycle_definitions ADD COLUMN IF NOT EXISTS source_ref TEXT",
        "ALTER TABLE lifecycle_definitions ADD COLUMN IF NOT EXISTS source_version TEXT",
        "ALTER TABLE lifecycle_definitions ADD COLUMN IF NOT EXISTS source_digest TEXT",
        "ALTER TABLE lifecycle_definitions ADD COLUMN IF NOT EXISTS installed_at TEXT",
        "CREATE INDEX IF NOT EXISTS idx_workflow_definitions_library_asset_id ON workflow_definitions(library_asset_id)",
        "CREATE INDEX IF NOT EXISTS idx_lifecycle_definitions_library_asset_id ON lifecycle_definitions(library_asset_id)",
    ] {
        sqlx::query(query).execute(pool).await.map_err(db_err)?;
    }
    Ok(())
}

async fn add_lifecycle_run_columns(pool: &PgPool) -> Result<(), DomainError> {
    sqlx::query(
        "ALTER TABLE lifecycle_runs ADD COLUMN IF NOT EXISTS record_artifacts TEXT NOT NULL DEFAULT '{}'",
    )
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(())
}

async fn add_activity_lifecycle_columns(pool: &PgPool) -> Result<(), DomainError> {
    for query in [
        "ALTER TABLE lifecycle_definitions ADD COLUMN IF NOT EXISTS entry_activity_key TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE lifecycle_definitions ADD COLUMN IF NOT EXISTS activities TEXT NOT NULL DEFAULT '[]'",
        "ALTER TABLE lifecycle_definitions ADD COLUMN IF NOT EXISTS transitions TEXT NOT NULL DEFAULT '[]'",
    ] {
        sqlx::query(query).execute(pool).await.map_err(db_err)?;
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
    project_id: String,
    key: String,
    name: String,
    description: String,
    binding_kinds: String,
    source: String,
    version: i32,
    contract: String,
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<String>,
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
            binding_kinds: parse_json_column(
                &row.binding_kinds,
                "workflow_definitions.binding_kinds",
            )?,
            source: serde_json::from_str(&row.source)?,
            installed_source: parse_installed_source(
                row.library_asset_id,
                row.source_ref,
                row.source_version,
                row.source_digest,
                row.installed_at,
            )?,
            version: row.version,
            contract: serde_json::from_str(&row.contract)?,
            created_at: parse_time(&row.created_at)?,
            updated_at: parse_time(&row.updated_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct ActivityLifecycleDefinitionRow {
    id: String,
    project_id: String,
    key: String,
    name: String,
    description: String,
    binding_kinds: String,
    source: String,
    version: i32,
    entry_activity_key: String,
    activities: String,
    transitions: String,
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ActivityLifecycleDefinitionRow> for ActivityLifecycleDefinition {
    type Error = DomainError;
    fn try_from(row: ActivityLifecycleDefinitionRow) -> Result<Self, Self::Error> {
        Ok(ActivityLifecycleDefinition {
            id: parse_uuid(&row.id, "activity_lifecycle_definition")?,
            project_id: parse_uuid(&row.project_id, "project")?,
            key: row.key,
            name: row.name,
            description: row.description,
            binding_kinds: parse_json_column(
                &row.binding_kinds,
                "lifecycle_definitions.binding_kinds",
            )?,
            source: serde_json::from_str(&row.source)?,
            installed_source: parse_installed_source(
                row.library_asset_id,
                row.source_ref,
                row.source_version,
                row.source_digest,
                row.installed_at,
            )?,
            version: row.version,
            entry_activity_key: row.entry_activity_key,
            activities: parse_json_column(&row.activities, "lifecycle_definitions.activities")?,
            transitions: parse_json_column(&row.transitions, "lifecycle_definitions.transitions")?,
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
    binding_kinds: String,
    source: String,
    version: i32,
    entry_step_key: String,
    steps: String,
    edges: String,
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<String>,
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
            binding_kinds: parse_json_column(
                &row.binding_kinds,
                "lifecycle_definitions.binding_kinds",
            )?,
            source: serde_json::from_str(&row.source)?,
            installed_source: parse_installed_source(
                row.library_asset_id,
                row.source_ref,
                row.source_version,
                row.source_digest,
                row.installed_at,
            )?,
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

fn installed_library_asset_id(source: &Option<InstalledAssetSource>) -> Option<String> {
    source
        .as_ref()
        .map(|source| source.library_asset_id.to_string())
}

fn installed_source_ref(source: &Option<InstalledAssetSource>) -> Option<&str> {
    source.as_ref().map(|source| source.source_ref.as_str())
}

fn installed_source_version(source: &Option<InstalledAssetSource>) -> Option<&str> {
    source.as_ref().map(|source| source.source_version.as_str())
}

fn installed_source_digest(source: &Option<InstalledAssetSource>) -> Option<&str> {
    source.as_ref().map(|source| source.source_digest.as_str())
}

fn installed_at(source: &Option<InstalledAssetSource>) -> Option<String> {
    source
        .as_ref()
        .map(|source| source.installed_at.to_rfc3339())
}

fn parse_installed_source(
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<String>,
) -> Result<Option<InstalledAssetSource>, DomainError> {
    let Some(library_asset_id) = library_asset_id else {
        return Ok(None);
    };
    Ok(Some(InstalledAssetSource {
        library_asset_id: library_asset_id.parse().map_err(|_| {
            DomainError::InvalidConfig("installed_source.library_asset_id 无效".to_string())
        })?,
        source_ref: source_ref.ok_or_else(|| {
            DomainError::InvalidConfig("installed_source.source_ref 为空".to_string())
        })?,
        source_version: source_version.ok_or_else(|| {
            DomainError::InvalidConfig("installed_source.source_version 为空".to_string())
        })?,
        source_digest: source_digest.ok_or_else(|| {
            DomainError::InvalidConfig("installed_source.source_digest 为空".to_string())
        })?,
        installed_at: super::parse_pg_timestamp_checked(
            installed_at.as_deref().ok_or_else(|| {
                DomainError::InvalidConfig("installed_source.installed_at 为空".to_string())
            })?,
            "installed_source.installed_at",
        )?,
    }))
}
