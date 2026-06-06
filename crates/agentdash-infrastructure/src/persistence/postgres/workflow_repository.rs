use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::shared_library::InstalledAssetSource;
use agentdash_domain::workflow::{
    AgentProcedure, AgentProcedureRepository, LifecycleContext, LifecycleRun,
    LifecycleRunRepository, LifecycleRunTopology, OrchestrationInstance, WorkflowGraph,
    WorkflowGraphRepository, WorkflowTemplateInstallBundle, WorkflowTemplateInstallRepository,
    WorkflowTemplateInstallResult,
};

pub struct PostgresWorkflowRepository {
    pool: PgPool,
}

impl PostgresWorkflowRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &["agent_procedures", "workflow_graphs", "lifecycle_runs"],
        )
        .await
    }
}

const WF_COLS: &str = "id,project_id,key,name,description,source,version,contract,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at";
const WG_COLS: &str = "id,project_id,key,name,description,source,version,entry_activity_key,activities,transitions,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at";
const RUN_COLS: &str = "id,project_id,topology,root_graph_id,context,orchestrations,view_projection,status,execution_log,created_at,updated_at,last_activity_at";
const RUN_INSERT_COLS: &str = "id,project_id,topology,root_graph_id,context,orchestrations,view_projection,status,execution_log,created_at,updated_at,last_activity_at";

#[async_trait::async_trait]
impl AgentProcedureRepository for PostgresWorkflowRepository {
    async fn create(&self, procedure: &AgentProcedure) -> Result<(), DomainError> {
        sqlx::query(&format!("INSERT INTO agent_procedures ({WF_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)"))
            .bind(procedure.id.to_string()).bind(procedure.project_id.to_string())
            .bind(&procedure.key).bind(&procedure.name).bind(&procedure.description)
            .bind(serde_json::to_string(&procedure.source)?)
            .bind(procedure.version).bind(serde_json::to_string(&procedure.contract)?)
            .bind(installed_library_asset_id(&procedure.installed_source))
            .bind(installed_source_ref(&procedure.installed_source))
            .bind(installed_source_version(&procedure.installed_source))
            .bind(installed_source_digest(&procedure.installed_source))
            .bind(installed_at(&procedure.installed_source))
            .bind(procedure.created_at).bind(procedure.updated_at)
            .execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<AgentProcedure>, DomainError> {
        sqlx::query_as::<_, AgentProcedureRow>(&format!(
            "SELECT {WF_COLS} FROM agent_procedures WHERE id = $1"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn get_by_key(&self, key: &str) -> Result<Option<AgentProcedure>, DomainError> {
        sqlx::query_as::<_, AgentProcedureRow>(&format!(
            "SELECT {WF_COLS} FROM agent_procedures WHERE key = $1 LIMIT 1"
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
    ) -> Result<Option<AgentProcedure>, DomainError> {
        sqlx::query_as::<_, AgentProcedureRow>(&format!(
            "SELECT {WF_COLS} FROM agent_procedures WHERE project_id = $1 AND key = $2"
        ))
        .bind(project_id.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_all(&self) -> Result<Vec<AgentProcedure>, DomainError> {
        sqlx::query_as::<_, AgentProcedureRow>(&format!(
            "SELECT {WF_COLS} FROM agent_procedures ORDER BY created_at DESC"
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
    ) -> Result<Vec<AgentProcedure>, DomainError> {
        sqlx::query_as::<_, AgentProcedureRow>(&format!(
            "SELECT {WF_COLS} FROM agent_procedures WHERE project_id = $1 ORDER BY created_at DESC"
        ))
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn update(&self, procedure: &AgentProcedure) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE agent_procedures SET project_id=$1,key=$2,name=$3,description=$4,source=$5,version=$6,contract=$7,library_asset_id=$8,source_ref=$9,source_version=$10,source_digest=$11,installed_at=$12,updated_at=$13 WHERE id=$14")
            .bind(procedure.project_id.to_string())
            .bind(&procedure.key).bind(&procedure.name).bind(&procedure.description)
            .bind(serde_json::to_string(&procedure.source)?)
            .bind(procedure.version).bind(serde_json::to_string(&procedure.contract)?)
            .bind(installed_library_asset_id(&procedure.installed_source))
            .bind(installed_source_ref(&procedure.installed_source))
            .bind(installed_source_version(&procedure.installed_source))
            .bind(installed_source_digest(&procedure.installed_source))
            .bind(installed_at(&procedure.installed_source))
            .bind(chrono::Utc::now())
            .bind(procedure.id.to_string()).execute(&self.pool).await.map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "agent_procedure", &procedure.id)
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM agent_procedures WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "agent_procedure", &id)
    }
}

#[async_trait::async_trait]
impl WorkflowGraphRepository for PostgresWorkflowRepository {
    async fn create(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO workflow_graphs ({WG_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)"
        ))
        .bind(lifecycle.id.to_string())
        .bind(lifecycle.project_id.to_string())
        .bind(&lifecycle.key)
        .bind(&lifecycle.name)
        .bind(&lifecycle.description)
        .bind(serde_json::to_string(&lifecycle.source)?)
        .bind(lifecycle.version)
        .bind(&lifecycle.entry_activity_key)
        .bind(serde_json::to_string(&lifecycle.activities)?)
        .bind(serde_json::to_string(&lifecycle.transitions)?)
        .bind(installed_library_asset_id(&lifecycle.installed_source))
        .bind(installed_source_ref(&lifecycle.installed_source))
        .bind(installed_source_version(&lifecycle.installed_source))
        .bind(installed_source_digest(&lifecycle.installed_source))
        .bind(installed_at(&lifecycle.installed_source))
        .bind(lifecycle.created_at)
        .bind(lifecycle.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<WorkflowGraph>, DomainError> {
        sqlx::query_as::<_, WorkflowGraphRow>(&format!(
            "SELECT {WG_COLS} FROM workflow_graphs WHERE id = $1"
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
    ) -> Result<Option<WorkflowGraph>, DomainError> {
        sqlx::query_as::<_, WorkflowGraphRow>(&format!(
            "SELECT {WG_COLS} FROM workflow_graphs WHERE project_id = $1 AND key = $2"
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
    ) -> Result<Vec<WorkflowGraph>, DomainError> {
        sqlx::query_as::<_, WorkflowGraphRow>(&format!(
            "SELECT {WG_COLS} FROM workflow_graphs WHERE project_id = $1 ORDER BY created_at DESC"
        ))
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn update(&self, lifecycle: &WorkflowGraph) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE workflow_graphs SET project_id=$1,key=$2,name=$3,description=$4,source=$5,version=$6,entry_activity_key=$7,activities=$8,transitions=$9,library_asset_id=$10,source_ref=$11,source_version=$12,source_digest=$13,installed_at=$14,updated_at=$15 WHERE id=$16")
            .bind(lifecycle.project_id.to_string())
            .bind(&lifecycle.key)
            .bind(&lifecycle.name)
            .bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.source)?)
            .bind(lifecycle.version)
            .bind(&lifecycle.entry_activity_key)
            .bind(serde_json::to_string(&lifecycle.activities)?)
            .bind(serde_json::to_string(&lifecycle.transitions)?)
            .bind(installed_library_asset_id(&lifecycle.installed_source))
            .bind(installed_source_ref(&lifecycle.installed_source))
            .bind(installed_source_version(&lifecycle.installed_source))
            .bind(installed_source_digest(&lifecycle.installed_source))
            .bind(installed_at(&lifecycle.installed_source))
            .bind(chrono::Utc::now())
            .bind(lifecycle.id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "workflow_graph", &lifecycle.id)
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM workflow_graphs WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "workflow_graph", &id)
    }
}

#[async_trait::async_trait]
impl WorkflowTemplateInstallRepository for PostgresWorkflowRepository {
    async fn install_workflow_template_bundle(
        &self,
        bundle: WorkflowTemplateInstallBundle,
    ) -> Result<WorkflowTemplateInstallResult, DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let mut procedure_keys = std::collections::BTreeSet::new();
        for procedure in &bundle.procedures {
            if !procedure_keys.insert(procedure.key.clone()) {
                return Err(DomainError::InvalidConfig(format!(
                    "workflow template 内 procedure key 重复: {}",
                    procedure.key
                )));
            }
        }
        if procedure_keys.contains(&bundle.graph.key) {
            return Err(DomainError::InvalidConfig(format!(
                "workflow template 的 procedure key 与 lifecycle key 冲突: {}",
                bundle.graph.key
            )));
        }

        let mut persisted_procedures = Vec::with_capacity(bundle.procedures.len());
        for mut procedure in bundle.procedures {
            let existing = sqlx::query_as::<_, ExistingProjectResourceRow>(
                "SELECT id,version,created_at FROM agent_procedures WHERE project_id = $1 AND key = $2",
            )
            .bind(procedure.project_id.to_string())
            .bind(&procedure.key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?;

            if let Some(existing) = existing {
                if !bundle.overwrite {
                    return Err(DomainError::InvalidConfig(format!(
                        "Project Procedure key 已存在: {}",
                        procedure.key
                    )));
                }
                procedure.id = parse_uuid(&existing.id, "agent_procedure")?;
                procedure.version = existing.version + 1;
                procedure.created_at = existing.created_at;
                procedure.updated_at = chrono::Utc::now();
                sqlx::query("UPDATE agent_procedures SET project_id=$1,key=$2,name=$3,description=$4,source=$5,version=$6,contract=$7,library_asset_id=$8,source_ref=$9,source_version=$10,source_digest=$11,installed_at=$12,updated_at=$13 WHERE id=$14")
                    .bind(procedure.project_id.to_string())
                    .bind(&procedure.key)
                    .bind(&procedure.name)
                    .bind(&procedure.description)
                    .bind(serde_json::to_string(&procedure.source)?)
                    .bind(procedure.version)
                    .bind(serde_json::to_string(&procedure.contract)?)
                    .bind(installed_library_asset_id(&procedure.installed_source))
                    .bind(installed_source_ref(&procedure.installed_source))
                    .bind(installed_source_version(&procedure.installed_source))
                    .bind(installed_source_digest(&procedure.installed_source))
                    .bind(installed_at(&procedure.installed_source))
                    .bind(procedure.updated_at)
                    .bind(procedure.id.to_string())
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
            } else {
                sqlx::query(&format!("INSERT INTO agent_procedures ({WF_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)"))
                    .bind(procedure.id.to_string())
                    .bind(procedure.project_id.to_string())
                    .bind(&procedure.key)
                    .bind(&procedure.name)
                    .bind(&procedure.description)
                    .bind(serde_json::to_string(&procedure.source)?)
                    .bind(procedure.version)
                    .bind(serde_json::to_string(&procedure.contract)?)
                    .bind(installed_library_asset_id(&procedure.installed_source))
                    .bind(installed_source_ref(&procedure.installed_source))
                    .bind(installed_source_version(&procedure.installed_source))
                    .bind(installed_source_digest(&procedure.installed_source))
                    .bind(installed_at(&procedure.installed_source))
                    .bind(procedure.created_at)
                    .bind(procedure.updated_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
            }
            persisted_procedures.push(procedure);
        }

        let mut lifecycle = bundle.graph;
        let existing = sqlx::query_as::<_, ExistingProjectResourceRow>(
            "SELECT id,version,created_at FROM workflow_graphs WHERE project_id = $1 AND key = $2",
        )
        .bind(lifecycle.project_id.to_string())
        .bind(&lifecycle.key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        if let Some(existing) = existing {
            if !bundle.overwrite {
                return Err(DomainError::InvalidConfig(format!(
                    "Project Lifecycle key 已存在: {}",
                    lifecycle.key
                )));
            }
            lifecycle.id = parse_uuid(&existing.id, "workflow_graph")?;
            lifecycle.version = existing.version + 1;
            lifecycle.created_at = existing.created_at;
            lifecycle.updated_at = chrono::Utc::now();
            sqlx::query("UPDATE workflow_graphs SET project_id=$1,key=$2,name=$3,description=$4,source=$5,version=$6,entry_activity_key=$7,activities=$8,transitions=$9,library_asset_id=$10,source_ref=$11,source_version=$12,source_digest=$13,installed_at=$14,updated_at=$15 WHERE id=$16")
                .bind(lifecycle.project_id.to_string())
                .bind(&lifecycle.key)
                .bind(&lifecycle.name)
                .bind(&lifecycle.description)
                .bind(serde_json::to_string(&lifecycle.source)?)
                .bind(lifecycle.version)
                .bind(&lifecycle.entry_activity_key)
                .bind(serde_json::to_string(&lifecycle.activities)?)
                .bind(serde_json::to_string(&lifecycle.transitions)?)
                .bind(installed_library_asset_id(&lifecycle.installed_source))
                .bind(installed_source_ref(&lifecycle.installed_source))
                .bind(installed_source_version(&lifecycle.installed_source))
                .bind(installed_source_digest(&lifecycle.installed_source))
                .bind(installed_at(&lifecycle.installed_source))
                .bind(lifecycle.updated_at)
                .bind(lifecycle.id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        } else {
            sqlx::query(&format!(
                "INSERT INTO workflow_graphs ({WG_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)"
            ))
            .bind(lifecycle.id.to_string())
            .bind(lifecycle.project_id.to_string())
            .bind(&lifecycle.key)
            .bind(&lifecycle.name)
            .bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.source)?)
            .bind(lifecycle.version)
            .bind(&lifecycle.entry_activity_key)
            .bind(serde_json::to_string(&lifecycle.activities)?)
            .bind(serde_json::to_string(&lifecycle.transitions)?)
            .bind(installed_library_asset_id(&lifecycle.installed_source))
            .bind(installed_source_ref(&lifecycle.installed_source))
            .bind(installed_source_version(&lifecycle.installed_source))
            .bind(installed_source_digest(&lifecycle.installed_source))
            .bind(installed_at(&lifecycle.installed_source))
            .bind(lifecycle.created_at)
            .bind(lifecycle.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }

        tx.commit().await.map_err(db_err)?;
        Ok(WorkflowTemplateInstallResult {
            procedures: persisted_procedures,
            graph: lifecycle,
        })
    }
}

#[async_trait::async_trait]
impl LifecycleRunRepository for PostgresWorkflowRepository {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO lifecycle_runs ({RUN_INSERT_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"
        ))
        .bind(run.id.to_string())
        .bind(run.project_id.to_string())
        .bind(topology_to_db(run.topology))
        .bind(run.root_graph_id.map(|id| id.to_string()))
        .bind(serde_json::to_string(&run.context)?)
        .bind(serde_json::to_string(&run.orchestrations)?)
        .bind(serialize_optional_json(&run.view_projection)?)
        .bind(serde_json::to_string(&run.status)?)
        .bind(serde_json::to_string(&run.execution_log)?)
        .bind(run.created_at)
        .bind(run.updated_at)
        .bind(run.last_activity_at)
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

    async fn list_by_ids(&self, ids: &[uuid::Uuid]) -> Result<Vec<LifecycleRun>, DomainError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: Vec<String> = ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect();
        let sql = format!(
            "SELECT {RUN_COLS} FROM lifecycle_runs WHERE id IN ({}) ORDER BY created_at DESC",
            placeholders.join(",")
        );
        let mut query = sqlx::query_as::<_, LifecycleRunRow>(&sql);
        for id in ids {
            query = query.bind(id.to_string());
        }
        query
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

    async fn list_by_root_graph(
        &self,
        root_graph_id: uuid::Uuid,
    ) -> Result<Vec<LifecycleRun>, DomainError> {
        sqlx::query_as::<_, LifecycleRunRow>(&format!(
            "SELECT {RUN_COLS} FROM lifecycle_runs WHERE root_graph_id = $1 ORDER BY created_at DESC"
        ))
        .bind(root_graph_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn update(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE lifecycle_runs SET project_id=$1,topology=$2,root_graph_id=$3,context=$4,orchestrations=$5,view_projection=$6,status=$7,execution_log=$8,updated_at=$9,last_activity_at=$10 WHERE id=$11")
            .bind(run.project_id.to_string())
            .bind(topology_to_db(run.topology))
            .bind(run.root_graph_id.map(|id| id.to_string()))
            .bind(serde_json::to_string(&run.context)?)
            .bind(serde_json::to_string(&run.orchestrations)?)
            .bind(serialize_optional_json(&run.view_projection)?)
            .bind(serde_json::to_string(&run.status)?)
            .bind(serde_json::to_string(&run.execution_log)?)
            .bind(chrono::Utc::now()).bind(run.last_activity_at).bind(run.id.to_string())
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

use super::db_err;

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
struct ExistingProjectResourceRow {
    id: String,
    version: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct AgentProcedureRow {
    id: String,
    project_id: String,
    key: String,
    name: String,
    description: String,
    source: String,
    version: i32,
    contract: String,
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<AgentProcedureRow> for AgentProcedure {
    type Error = DomainError;
    fn try_from(row: AgentProcedureRow) -> Result<Self, Self::Error> {
        Ok(AgentProcedure {
            id: parse_uuid(&row.id, "agent_procedure")?,
            project_id: parse_uuid(&row.project_id, "project")?,
            key: row.key,
            name: row.name,
            description: row.description,
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
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct WorkflowGraphRow {
    id: String,
    project_id: String,
    key: String,
    name: String,
    description: String,
    source: String,
    version: i32,
    entry_activity_key: String,
    activities: String,
    transitions: String,
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<WorkflowGraphRow> for WorkflowGraph {
    type Error = DomainError;
    fn try_from(row: WorkflowGraphRow) -> Result<Self, Self::Error> {
        Ok(WorkflowGraph {
            id: parse_uuid(&row.id, "workflow_graph")?,
            project_id: parse_uuid(&row.project_id, "project")?,
            key: row.key,
            name: row.name,
            description: row.description,
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
            activities: parse_json_column(&row.activities, "workflow_graphs.activities")?,
            transitions: parse_json_column(&row.transitions, "workflow_graphs.transitions")?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct LifecycleRunRow {
    id: String,
    project_id: String,
    topology: String,
    root_graph_id: Option<String>,
    context: String,
    orchestrations: String,
    view_projection: Option<String>,
    status: String,
    execution_log: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_activity_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<LifecycleRunRow> for LifecycleRun {
    type Error = DomainError;
    fn try_from(row: LifecycleRunRow) -> Result<Self, Self::Error> {
        Ok(LifecycleRun {
            id: parse_uuid(&row.id, "lifecycle_run")?,
            project_id: parse_uuid(&row.project_id, "project")?,
            topology: parse_topology(&row.topology)?,
            root_graph_id: row
                .root_graph_id
                .as_deref()
                .map(|id| parse_uuid(id, "root_graph"))
                .transpose()?,
            context: parse_json_column::<LifecycleContext>(&row.context, "lifecycle_runs.context")?,
            orchestrations: parse_json_column::<Vec<OrchestrationInstance>>(
                &row.orchestrations,
                "lifecycle_runs.orchestrations",
            )?,
            view_projection: row
                .view_projection
                .as_deref()
                .map(|raw| parse_json_column(raw, "lifecycle_runs.view_projection"))
                .transpose()?,
            status: serde_json::from_str(&row.status)?,
            execution_log: parse_json_column(&row.execution_log, "lifecycle_runs.execution_log")?,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_activity_at: row.last_activity_at,
        })
    }
}

fn parse_uuid(raw: &str, entity: &'static str) -> Result<uuid::Uuid, DomainError> {
    raw.parse().map_err(|_| DomainError::NotFound {
        entity,
        id: raw.to_string(),
    })
}

fn topology_to_db(topology: LifecycleRunTopology) -> &'static str {
    match topology {
        LifecycleRunTopology::Graphless => "graphless",
        LifecycleRunTopology::WorkflowGraph => "workflow_graph",
    }
}

fn parse_topology(raw: &str) -> Result<LifecycleRunTopology, DomainError> {
    match raw {
        "graphless" => Ok(LifecycleRunTopology::Graphless),
        "workflow_graph" => Ok(LifecycleRunTopology::WorkflowGraph),
        other => Err(DomainError::InvalidConfig(format!(
            "lifecycle_runs.topology 无效: {other}"
        ))),
    }
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn serialize_optional_json(
    value: &Option<serde_json::Value>,
) -> Result<Option<String>, DomainError> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
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

fn installed_at(source: &Option<InstalledAssetSource>) -> Option<chrono::DateTime<chrono::Utc>> {
    source.as_ref().map(|source| source.installed_at)
}

fn parse_installed_source(
    library_asset_id: Option<String>,
    source_ref: Option<String>,
    source_version: Option<String>,
    source_digest: Option<String>,
    installed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Option<InstalledAssetSource>, DomainError> {
    let Some(library_asset_id) = library_asset_id else {
        return Ok(None);
    };
    Ok(Some(InstalledAssetSource {
        library_asset_id: library_asset_id.parse().map_err(|_| {
            DomainError::InvalidConfig(String::from("installed_source.library_asset_id 无效"))
        })?,
        source_ref: source_ref.ok_or_else(|| {
            DomainError::InvalidConfig(String::from("installed_source.source_ref 为空"))
        })?,
        source_version: source_version.ok_or_else(|| {
            DomainError::InvalidConfig(String::from("installed_source.source_version 为空"))
        })?,
        source_digest: source_digest.ok_or_else(|| {
            DomainError::InvalidConfig(String::from("installed_source.source_digest 为空"))
        })?,
        installed_at: installed_at.ok_or_else(|| {
            DomainError::InvalidConfig(String::from("installed_source.installed_at 为空"))
        })?,
    }))
}

#[cfg(test)]
mod workflow_claim_tests {
    use super::*;
    use crate::persistence::postgres::test_pg_pool;
    use agentdash_domain::workflow::{
        ActivityCompletionPolicy, ActivityDefinition, ActivityExecutorSpec,
        AgentActivityExecutorSpec, AgentProcedureContract, AgentReusePolicy, AgentRunRef,
        BashExecExecutorSpec, ExecutorSpec, FunctionActivityExecutorSpec,
        HumanActivityExecutorSpec, HumanApprovalExecutorSpec, LifecycleContext, LifecycleRunStatus,
        OrchestrationInstance, OrchestrationPlanSnapshot, OrchestrationSourceRef, PlanNode,
        PlanNodeKind, RuntimeSessionPolicy, WorkflowTemplateInstallBundle,
    };
    use serde_json::json;

    fn lifecycle_run_row() -> LifecycleRunRow {
        let now = chrono::Utc::now();
        LifecycleRunRow {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: uuid::Uuid::new_v4().to_string(),
            topology: "graphless".to_string(),
            root_graph_id: None,
            context: "{}".to_string(),
            orchestrations: "[]".to_string(),
            view_projection: None,
            status: serde_json::to_string(&LifecycleRunStatus::Ready).expect("status json"),
            execution_log: "[]".to_string(),
            created_at: now,
            updated_at: now,
            last_activity_at: now,
        }
    }

    #[test]
    fn workflow_repository_lifecycle_run_row_parses_empty_orchestration_contract() {
        let run = LifecycleRun::try_from(lifecycle_run_row()).expect("run");

        assert_eq!(run.context, LifecycleContext::default());
        assert!(run.orchestrations.is_empty());
        assert!(run.view_projection.is_none());
    }

    #[test]
    fn workflow_repository_lifecycle_run_row_reports_bad_orchestration_column() {
        let mut row = lifecycle_run_row();
        row.orchestrations = "not-json".to_string();

        let error = LifecycleRun::try_from(row).expect_err("bad JSON should fail");
        assert!(
            error.to_string().contains("lifecycle_runs.orchestrations"),
            "unexpected error: {error}"
        );
    }

    fn orchestration_instance(role: &str, executor: ExecutorSpec) -> OrchestrationInstance {
        let source_ref = OrchestrationSourceRef::WorkflowGraph {
            graph_id: uuid::Uuid::new_v4(),
            graph_version: Some(1),
        };
        let plan_snapshot = OrchestrationPlanSnapshot {
            plan_digest: format!("sha256:{role}"),
            plan_version: 1,
            source_ref: source_ref.clone(),
            nodes: vec![PlanNode {
                node_id: role.to_string(),
                node_path: role.to_string(),
                parent_node_id: None,
                kind: PlanNodeKind::Activity,
                label: Some(role.to_string()),
                executor: Some(executor),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: None,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec![role.to_string()],
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            limits: Default::default(),
            metadata: None,
            created_at: chrono::Utc::now(),
        };
        OrchestrationInstance::new(role, source_ref, plan_snapshot)
    }

    fn agent_executor() -> ExecutorSpec {
        ExecutorSpec::AgentProcedure {
            procedure_key: "workflow.plan".to_string(),
            agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
            runtime_session_policy: RuntimeSessionPolicy::CreateNew,
        }
    }

    fn function_executor() -> ExecutorSpec {
        ExecutorSpec::Function {
            spec: FunctionActivityExecutorSpec::BashExec(BashExecExecutorSpec {
                command: "pnpm".to_string(),
                args: vec!["test".to_string()],
                working_directory: None,
            }),
        }
    }

    fn human_executor() -> ExecutorSpec {
        ExecutorSpec::Human {
            spec: HumanActivityExecutorSpec::Approval(HumanApprovalExecutorSpec {
                form_schema_key: "approval.plan_review".to_string(),
                title: None,
            }),
        }
    }

    fn test_procedure(project_id: uuid::Uuid, key: &str, digest: &str) -> AgentProcedure {
        let mut procedure = AgentProcedure::new(
            project_id,
            key,
            format!("Procedure {digest}"),
            "",
            agentdash_domain::workflow::DefinitionSource::UserAuthored,
            AgentProcedureContract::default(),
        )
        .expect("procedure");
        procedure.installed_source = Some(InstalledAssetSource::new(
            uuid::Uuid::new_v4(),
            "template",
            digest,
            format!("sha256:{digest}"),
        ));
        procedure
    }

    fn test_lifecycle(
        project_id: uuid::Uuid,
        key: &str,
        procedure_key: &str,
        digest: &str,
    ) -> WorkflowGraph {
        let mut lifecycle = WorkflowGraph::new(
            project_id,
            key,
            format!("Lifecycle {digest}"),
            "",
            agentdash_domain::workflow::DefinitionSource::UserAuthored,
            "plan",
            vec![ActivityDefinition {
                key: "plan".to_string(),
                description: String::new(),
                executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                    procedure_key: procedure_key.to_string(),
                    agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
                    runtime_session_policy: RuntimeSessionPolicy::CreateNew,
                }),
                input_ports: vec![],
                output_ports: vec![],
                completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
                iteration_policy: Default::default(),
                join_policy: Default::default(),
            }],
            vec![],
        )
        .expect("lifecycle");
        lifecycle.installed_source = Some(InstalledAssetSource::new(
            uuid::Uuid::new_v4(),
            "template",
            digest,
            format!("sha256:{digest}"),
        ));
        lifecycle
    }

    #[tokio::test]
    async fn workflow_template_install_overwrite_is_transactional_and_bumps_versions() {
        let Some(pool) = test_pg_pool("workflow_template_install").await else {
            return;
        };
        let repo = PostgresWorkflowRepository::new(pool);
        repo.initialize().await.expect("initialize");

        let project_id = uuid::Uuid::new_v4();
        let procedure_key = format!("wf_{}", uuid::Uuid::new_v4().simple());
        let lifecycle_key = format!("lc_{}", uuid::Uuid::new_v4().simple());

        repo.install_workflow_template_bundle(WorkflowTemplateInstallBundle {
            procedures: vec![test_procedure(project_id, &procedure_key, "v1")],
            graph: test_lifecycle(project_id, &lifecycle_key, &procedure_key, "v1"),
            overwrite: false,
        })
        .await
        .expect("first install");

        let conflict = repo
            .install_workflow_template_bundle(WorkflowTemplateInstallBundle {
                procedures: vec![test_procedure(project_id, &procedure_key, "v2")],
                graph: test_lifecycle(project_id, &lifecycle_key, &procedure_key, "v2"),
                overwrite: false,
            })
            .await
            .expect_err("conflict should fail without overwrite");
        assert!(conflict.to_string().contains("已存在"));

        let procedure_after_conflict =
            AgentProcedureRepository::get_by_project_and_key(&repo, project_id, &procedure_key)
                .await
                .expect("get procedure")
                .expect("procedure exists");
        assert_eq!(procedure_after_conflict.version, 1);
        assert_eq!(
            procedure_after_conflict
                .installed_source
                .as_ref()
                .expect("source")
                .source_version,
            "v1"
        );

        let result = repo
            .install_workflow_template_bundle(WorkflowTemplateInstallBundle {
                procedures: vec![test_procedure(project_id, &procedure_key, "v2")],
                graph: test_lifecycle(project_id, &lifecycle_key, &procedure_key, "v2"),
                overwrite: true,
            })
            .await
            .expect("overwrite install");

        assert_eq!(result.procedures[0].version, 2);
        assert_eq!(result.graph.version, 2);
        let procedure =
            AgentProcedureRepository::get_by_project_and_key(&repo, project_id, &procedure_key)
                .await
                .expect("get procedure")
                .expect("procedure exists");
        let lifecycle =
            WorkflowGraphRepository::get_by_project_and_key(&repo, project_id, &lifecycle_key)
                .await
                .expect("get lifecycle")
                .expect("lifecycle exists");
        assert_eq!(procedure.version, 2);
        assert_eq!(lifecycle.version, 2);
        assert_eq!(
            procedure
                .installed_source
                .expect("procedure source")
                .source_version,
            "v2"
        );
        assert_eq!(
            lifecycle
                .installed_source
                .expect("lifecycle source")
                .source_version,
            "v2"
        );
    }

    #[tokio::test]
    async fn workflow_repository_lifecycle_run_orchestration_roundtrips() {
        let Some(pool) = test_pg_pool("workflow_lifecycle_orchestration").await else {
            return;
        };
        let repo = PostgresWorkflowRepository::new(pool);
        repo.initialize().await.expect("initialize");

        let project_id = uuid::Uuid::new_v4();
        let mut run = LifecycleRun::new_control(project_id, uuid::Uuid::new_v4());
        let agent_run_id = uuid::Uuid::new_v4();
        let context = LifecycleContext {
            main_agent_run_id: Some(agent_run_id),
            agent_runs: vec![AgentRunRef {
                agent_run_id,
                role: "primary".to_string(),
                status: "active".to_string(),
                current_frame_id: Some(uuid::Uuid::new_v4()),
                project_agent_id: Some(uuid::Uuid::new_v4()),
            }],
            ..LifecycleContext::default()
        };
        run.set_lifecycle_context(context.clone());
        run.view_projection = Some(json!({"summary": "one"}));

        let agent_orchestration = orchestration_instance("agent", agent_executor());
        assert!(run.add_orchestration(agent_orchestration.clone()));
        LifecycleRunRepository::create(&repo, &run)
            .await
            .expect("create run");

        let created = LifecycleRunRepository::get_by_id(&repo, run.id)
            .await
            .expect("get run")
            .expect("run exists");
        assert_eq!(created.context, context);
        assert_eq!(created.orchestrations, vec![agent_orchestration]);
        assert_eq!(created.view_projection, Some(json!({"summary": "one"})));

        let mut updated = created;
        assert!(updated.add_orchestration(orchestration_instance("function", function_executor())));
        assert!(updated.add_orchestration(orchestration_instance("human", human_executor())));
        updated.view_projection = Some(json!({"summary": "multiple", "count": 3}));
        LifecycleRunRepository::update(&repo, &updated)
            .await
            .expect("update run");

        let restored = LifecycleRunRepository::get_by_id(&repo, run.id)
            .await
            .expect("get updated run")
            .expect("updated run exists");
        assert_eq!(restored.context, context);
        assert_eq!(restored.orchestrations.len(), 3);
        assert_eq!(
            restored.view_projection,
            Some(json!({"summary": "multiple", "count": 3}))
        );
        assert!(matches!(
            restored.orchestrations[0].plan_snapshot.nodes[0]
                .executor
                .as_ref()
                .expect("agent executor"),
            ExecutorSpec::AgentProcedure { .. }
        ));
        assert!(matches!(
            restored.orchestrations[1].plan_snapshot.nodes[0]
                .executor
                .as_ref()
                .expect("function executor"),
            ExecutorSpec::Function { .. }
        ));
        assert!(matches!(
            restored.orchestrations[2].plan_snapshot.nodes[0]
                .executor
                .as_ref()
                .expect("human executor"),
            ExecutorSpec::Human { .. }
        ));
    }
}
