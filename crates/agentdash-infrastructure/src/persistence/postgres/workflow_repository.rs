use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::shared_library::InstalledAssetSource;
use agentdash_domain::workflow::{
    ActivityExecutionClaim, ActivityExecutionClaimRepository, ActivityExecutionClaimStatus,
    AgentProcedure, AgentProcedureRepository, ExecutorRunRef, LifecycleRun, LifecycleRunRepository,
    LifecycleRunTopology, WorkflowGraph, WorkflowGraphRepository, WorkflowTemplateInstallBundle,
    WorkflowTemplateInstallRepository, WorkflowTemplateInstallResult,
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
            &[
                "agent_procedures",
                "workflow_graphs",
                "lifecycle_runs",
                "activity_execution_claims",
            ],
        )
        .await
    }
}

const WF_COLS: &str = "id,project_id,key,name,description,source,version,contract,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at";
const WG_COLS: &str = "id,project_id,key,name,description,source,version,entry_activity_key,activities,transitions,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at";
const RUN_COLS: &str = "id,project_id,topology,root_graph_id,status,active_node_keys,execution_log,created_at,updated_at,last_activity_at";
const RUN_INSERT_COLS: &str = "id,project_id,topology,root_graph_id,status,active_node_keys,execution_log,created_at,updated_at,last_activity_at";
const ACTIVITY_CLAIM_COLS: &str = "claim_id,run_id,graph_instance_id,activity_key,attempt,executor_kind,status,idempotency_key,executor_run_ref,created_at,updated_at";

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
impl ActivityExecutionClaimRepository for PostgresWorkflowRepository {
    async fn create_or_get(
        &self,
        claim: &ActivityExecutionClaim,
    ) -> Result<ActivityExecutionClaim, DomainError> {
        sqlx::query_as::<_, ActivityExecutionClaimRow>(&format!(
            "INSERT INTO activity_execution_claims ({ACTIVITY_CLAIM_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
             ON CONFLICT (idempotency_key) DO UPDATE SET updated_at = activity_execution_claims.updated_at \
             RETURNING {ACTIVITY_CLAIM_COLS}"
        ))
        .bind(claim.claim_id.to_string())
        .bind(claim.run_id.to_string())
        .bind(claim.graph_instance_id.to_string())
        .bind(&claim.activity_key)
        .bind(claim.attempt as i32)
        .bind(&claim.executor_kind)
        .bind(claim.status.as_str())
        .bind(&claim.idempotency_key)
        .bind(serialize_executor_run_ref(&claim.executor_run_ref)?)
        .bind(claim.created_at)
        .bind(claim.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?
        .try_into()
    }

    async fn get_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<ActivityExecutionClaim>, DomainError> {
        sqlx::query_as::<_, ActivityExecutionClaimRow>(&format!(
            "SELECT {ACTIVITY_CLAIM_COLS} FROM activity_execution_claims WHERE idempotency_key = $1"
        ))
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_active_by_run(
        &self,
        run_id: uuid::Uuid,
    ) -> Result<Vec<ActivityExecutionClaim>, DomainError> {
        sqlx::query_as::<_, ActivityExecutionClaimRow>(&format!(
            "SELECT {ACTIVITY_CLAIM_COLS} FROM activity_execution_claims WHERE run_id = $1 AND status IN ('claiming','running') ORDER BY created_at ASC"
        ))
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn update(&self, claim: &ActivityExecutionClaim) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE activity_execution_claims SET status=$1,executor_run_ref=$2,updated_at=$3 WHERE claim_id=$4",
        )
        .bind(claim.status.as_str())
        .bind(serialize_executor_run_ref(&claim.executor_run_ref)?)
        .bind(claim.updated_at)
        .bind(claim.claim_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        ensure_rows_affected(
            result.rows_affected(),
            "activity_execution_claim",
            &claim.claim_id,
        )
    }

    async fn abandon_claiming_before(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<ActivityExecutionClaim>, DomainError> {
        let now = chrono::Utc::now();
        sqlx::query_as::<_, ActivityExecutionClaimRow>(&format!(
            "UPDATE activity_execution_claims SET status='abandoned',updated_at=$1 \
             WHERE status='claiming' AND updated_at < $2 RETURNING {ACTIVITY_CLAIM_COLS}"
        ))
        .bind(now)
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }
}

#[async_trait::async_trait]
impl LifecycleRunRepository for PostgresWorkflowRepository {
    async fn create(&self, run: &LifecycleRun) -> Result<(), DomainError> {
        sqlx::query(&format!(
            "INSERT INTO lifecycle_runs ({RUN_INSERT_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"
        ))
        .bind(run.id.to_string())
        .bind(run.project_id.to_string())
        .bind(topology_to_db(run.topology))
        .bind(run.root_graph_id.map(|id| id.to_string()))
        .bind(serde_json::to_string(&run.status)?)
        .bind(serde_json::to_string(&run.active_node_keys)?)
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
        let result = sqlx::query("UPDATE lifecycle_runs SET project_id=$1,topology=$2,root_graph_id=$3,status=$4,active_node_keys=$5,execution_log=$6,updated_at=$7,last_activity_at=$8 WHERE id=$9")
            .bind(run.project_id.to_string())
            .bind(topology_to_db(run.topology))
            .bind(run.root_graph_id.map(|id| id.to_string()))
            .bind(serde_json::to_string(&run.status)?)
            .bind(serde_json::to_string(&run.active_node_keys)?)
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
    status: String,
    active_node_keys: String,
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
            status: serde_json::from_str(&row.status)?,
            active_node_keys: parse_json_column(
                &row.active_node_keys,
                "lifecycle_runs.active_node_keys",
            )?,
            execution_log: parse_json_column(&row.execution_log, "lifecycle_runs.execution_log")?,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_activity_at: row.last_activity_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct ActivityExecutionClaimRow {
    claim_id: String,
    run_id: String,
    graph_instance_id: String,
    activity_key: String,
    attempt: i32,
    executor_kind: String,
    status: String,
    idempotency_key: String,
    executor_run_ref: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<ActivityExecutionClaimRow> for ActivityExecutionClaim {
    type Error = DomainError;

    fn try_from(row: ActivityExecutionClaimRow) -> Result<Self, Self::Error> {
        let status = row
            .status
            .parse::<ActivityExecutionClaimStatus>()
            .map_err(DomainError::InvalidConfig)?;
        let executor_run_ref = row
            .executor_run_ref
            .map(|raw| {
                parse_json_column::<ExecutorRunRef>(
                    &raw,
                    "activity_execution_claims.executor_run_ref",
                )
            })
            .transpose()?;
        Ok(ActivityExecutionClaim {
            run_id: parse_uuid(&row.run_id, "lifecycle_run")?,
            graph_instance_id: parse_uuid(
                &row.graph_instance_id,
                "activity_execution_claim.graph_instance",
            )?,
            activity_key: row.activity_key,
            attempt: u32::try_from(row.attempt).map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "activity_execution_claims.attempt 无效: {}",
                    row.attempt
                ))
            })?,
            claim_id: parse_uuid(&row.claim_id, "activity_execution_claim")?,
            executor_kind: row.executor_kind,
            status,
            idempotency_key: row.idempotency_key,
            executor_run_ref,
            created_at: row.created_at,
            updated_at: row.updated_at,
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

fn serialize_executor_run_ref(
    executor_run_ref: &Option<ExecutorRunRef>,
) -> Result<Option<String>, DomainError> {
    executor_run_ref
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
        AgentActivityExecutorSpec, AgentProcedureContract, AgentReusePolicy, RuntimeSessionPolicy,
        WorkflowTemplateInstallBundle,
    };

    #[test]
    fn workflow_claim_row_parses_executor_run_ref() {
        let run_id = uuid::Uuid::new_v4();
        let graph_instance_id = uuid::Uuid::new_v4();
        let claim_id = uuid::Uuid::new_v4();
        let now = chrono::Utc::now();
        let row = ActivityExecutionClaimRow {
            claim_id: claim_id.to_string(),
            run_id: run_id.to_string(),
            graph_instance_id: graph_instance_id.to_string(),
            activity_key: "plan".to_string(),
            attempt: 2,
            executor_kind: "agent".to_string(),
            status: "running".to_string(),
            idempotency_key: format!("{run_id}:{graph_instance_id}:plan:2"),
            executor_run_ref: Some(
                serde_json::to_string(&ExecutorRunRef::RuntimeSession {
                    session_id: "child-session".to_string(),
                })
                .expect("executor run json"),
            ),
            created_at: now.clone(),
            updated_at: now,
        };

        let claim = ActivityExecutionClaim::try_from(row).expect("claim");

        assert_eq!(claim.run_id, run_id);
        assert_eq!(claim.graph_instance_id, graph_instance_id);
        assert_eq!(claim.claim_id, claim_id);
        assert_eq!(claim.activity_key, "plan");
        assert_eq!(claim.attempt, 2);
        assert_eq!(claim.status, ActivityExecutionClaimStatus::Running);
        assert!(claim.status.is_active());
        assert_eq!(
            claim.executor_run_ref,
            Some(ExecutorRunRef::RuntimeSession {
                session_id: "child-session".to_string()
            })
        );
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
}
