use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::shared_library::InstalledAssetSource;
use agentdash_domain::workflow::{
    ActivityExecutionClaim, ActivityExecutionClaimRepository, ActivityExecutionClaimStatus,
    ActivityLifecycleDefinition, ActivityLifecycleDefinitionRepository, ExecutorRunRef,
    LifecycleRun, LifecycleRunRepository, WorkflowBindingKind, WorkflowDefinition,
    WorkflowDefinitionRepository, WorkflowTemplateInstallBundle, WorkflowTemplateInstallRepository,
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
            &[
                "workflow_definitions",
                "lifecycle_definitions",
                "lifecycle_runs",
                "activity_execution_claims",
            ],
        )
        .await
    }
}

const WF_COLS: &str = "id,project_id,key,name,description,binding_kinds,source,version,contract,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at";
const ACTIVITY_LC_COLS: &str = "id,project_id,key,name,description,binding_kinds,source,version,entry_activity_key,activities,transitions,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at";
const RUN_COLS: &str = "id,project_id,lifecycle_id,session_id,status,execution_log,activity_state,created_at,updated_at,last_activity_at";
const RUN_INSERT_COLS: &str = "id,project_id,lifecycle_id,session_id,status,record_artifacts,execution_log,activity_state,created_at,updated_at,last_activity_at";
const ACTIVITY_CLAIM_COLS: &str = "claim_id,run_id,graph_instance_id,activity_key,attempt,executor_kind,status,idempotency_key,executor_run_ref,created_at,updated_at";

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
            .bind(workflow.created_at).bind(workflow.updated_at)
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
            .bind(chrono::Utc::now())
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
            "INSERT INTO lifecycle_definitions (id,project_id,key,name,description,binding_kinds,source,version,entry_activity_key,activities,transitions,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
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

    async fn get_by_id(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<ActivityLifecycleDefinition>, DomainError> {
        sqlx::query_as::<_, ActivityLifecycleDefinitionRow>(&format!(
            "SELECT {ACTIVITY_LC_COLS} FROM lifecycle_definitions WHERE id = $1"
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
            "SELECT {ACTIVITY_LC_COLS} FROM lifecycle_definitions WHERE project_id = $1 AND key = $2"
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
            "SELECT {ACTIVITY_LC_COLS} FROM lifecycle_definitions WHERE project_id = $1 ORDER BY created_at DESC"
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
        let result = sqlx::query("UPDATE lifecycle_definitions SET project_id=$1,key=$2,name=$3,description=$4,binding_kinds=$5,source=$6,version=$7,entry_activity_key=$8,activities=$9,transitions=$10,library_asset_id=$11,source_ref=$12,source_version=$13,source_digest=$14,installed_at=$15,updated_at=$16 WHERE id=$17")
            .bind(lifecycle.project_id.to_string())
            .bind(&lifecycle.key)
            .bind(&lifecycle.name)
            .bind(&lifecycle.description)
            .bind(serde_json::to_string(&lifecycle.binding_kinds)?)
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
        ensure_rows_affected(
            result.rows_affected(),
            "activity_lifecycle_definition",
            &lifecycle.id,
        )
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM lifecycle_definitions WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        ensure_rows_affected(result.rows_affected(), "activity_lifecycle_definition", &id)
    }
}

#[async_trait::async_trait]
impl WorkflowTemplateInstallRepository for PostgresWorkflowRepository {
    async fn install_workflow_template_bundle(
        &self,
        bundle: WorkflowTemplateInstallBundle,
    ) -> Result<WorkflowTemplateInstallResult, DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let mut workflow_keys = std::collections::BTreeSet::new();
        for workflow in &bundle.workflows {
            if !workflow_keys.insert(workflow.key.clone()) {
                return Err(DomainError::InvalidConfig(format!(
                    "workflow template 内 workflow key 重复: {}",
                    workflow.key
                )));
            }
        }
        if workflow_keys.contains(&bundle.lifecycle.key) {
            return Err(DomainError::InvalidConfig(format!(
                "workflow template 的 workflow key 与 lifecycle key 冲突: {}",
                bundle.lifecycle.key
            )));
        }

        let mut persisted_workflows = Vec::with_capacity(bundle.workflows.len());
        for mut workflow in bundle.workflows {
            let existing = sqlx::query_as::<_, ExistingProjectResourceRow>(
                "SELECT id,version,created_at FROM workflow_definitions WHERE project_id = $1 AND key = $2",
            )
            .bind(workflow.project_id.to_string())
            .bind(&workflow.key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?;

            if let Some(existing) = existing {
                if !bundle.overwrite {
                    return Err(DomainError::InvalidConfig(format!(
                        "Project Workflow key 已存在: {}",
                        workflow.key
                    )));
                }
                workflow.id = parse_uuid(&existing.id, "workflow_definition")?;
                workflow.version = existing.version + 1;
                workflow.created_at = existing.created_at;
                workflow.updated_at = chrono::Utc::now();
                sqlx::query("UPDATE workflow_definitions SET project_id=$1,key=$2,name=$3,description=$4,binding_kinds=$5,source=$6,version=$7,contract=$8,library_asset_id=$9,source_ref=$10,source_version=$11,source_digest=$12,installed_at=$13,updated_at=$14 WHERE id=$15")
                    .bind(workflow.project_id.to_string())
                    .bind(&workflow.key)
                    .bind(&workflow.name)
                    .bind(&workflow.description)
                    .bind(serde_json::to_string(&workflow.binding_kinds)?)
                    .bind(serde_json::to_string(&workflow.source)?)
                    .bind(workflow.version)
                    .bind(serde_json::to_string(&workflow.contract)?)
                    .bind(installed_library_asset_id(&workflow.installed_source))
                    .bind(installed_source_ref(&workflow.installed_source))
                    .bind(installed_source_version(&workflow.installed_source))
                    .bind(installed_source_digest(&workflow.installed_source))
                    .bind(installed_at(&workflow.installed_source))
                    .bind(workflow.updated_at)
                    .bind(workflow.id.to_string())
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
            } else {
                sqlx::query(&format!("INSERT INTO workflow_definitions ({WF_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)"))
                    .bind(workflow.id.to_string())
                    .bind(workflow.project_id.to_string())
                    .bind(&workflow.key)
                    .bind(&workflow.name)
                    .bind(&workflow.description)
                    .bind(serde_json::to_string(&workflow.binding_kinds)?)
                    .bind(serde_json::to_string(&workflow.source)?)
                    .bind(workflow.version)
                    .bind(serde_json::to_string(&workflow.contract)?)
                    .bind(installed_library_asset_id(&workflow.installed_source))
                    .bind(installed_source_ref(&workflow.installed_source))
                    .bind(installed_source_version(&workflow.installed_source))
                    .bind(installed_source_digest(&workflow.installed_source))
                    .bind(installed_at(&workflow.installed_source))
                    .bind(workflow.created_at)
                    .bind(workflow.updated_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
            }
            persisted_workflows.push(workflow);
        }

        let mut lifecycle = bundle.lifecycle;
        let existing = sqlx::query_as::<_, ExistingProjectResourceRow>(
            "SELECT id,version,created_at FROM lifecycle_definitions WHERE project_id = $1 AND key = $2",
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
            lifecycle.id = parse_uuid(&existing.id, "activity_lifecycle_definition")?;
            lifecycle.version = existing.version + 1;
            lifecycle.created_at = existing.created_at;
            lifecycle.updated_at = chrono::Utc::now();
            sqlx::query("UPDATE lifecycle_definitions SET project_id=$1,key=$2,name=$3,description=$4,binding_kinds=$5,source=$6,version=$7,entry_activity_key=$8,activities=$9,transitions=$10,library_asset_id=$11,source_ref=$12,source_version=$13,source_digest=$14,installed_at=$15,updated_at=$16 WHERE id=$17")
                .bind(lifecycle.project_id.to_string())
                .bind(&lifecycle.key)
                .bind(&lifecycle.name)
                .bind(&lifecycle.description)
                .bind(serde_json::to_string(&lifecycle.binding_kinds)?)
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
            sqlx::query(
                "INSERT INTO lifecycle_definitions (id,project_id,key,name,description,binding_kinds,source,version,entry_activity_key,activities,transitions,library_asset_id,source_ref,source_version,source_digest,installed_at,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
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
            workflows: persisted_workflows,
            lifecycle,
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

    async fn find_running_by_executor_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ActivityExecutionClaim>, DomainError> {
        sqlx::query_as::<_, ActivityExecutionClaimRow>(&format!(
            "SELECT {ACTIVITY_CLAIM_COLS} FROM activity_execution_claims \
             WHERE status = 'running' \
             AND executor_run_ref::jsonb -> 'AgentSession' ->> 'session_id' = $1 \
             ORDER BY updated_at DESC LIMIT 1"
        ))
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
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
        .bind("{}")
        .bind(serde_json::to_string(&run.execution_log)?)
        .bind(serialize_activity_state(&run.activity_state)?)
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
        let result = sqlx::query("UPDATE lifecycle_runs SET project_id=$1,lifecycle_id=$2,session_id=$3,status=$4,execution_log=$5,activity_state=$6,updated_at=$7,last_activity_at=$8 WHERE id=$9")
            .bind(run.project_id.to_string()).bind(run.lifecycle_id.to_string()).bind(&run.session_id)
            .bind(serde_json::to_string(&run.status)?)
            .bind(serde_json::to_string(&run.execution_log)?)
            .bind(serialize_activity_state(&run.activity_state)?)
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
    installed_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
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
            created_at: row.created_at,
            updated_at: row.updated_at,
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
    installed_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
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
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct LifecycleRunRow {
    id: String,
    project_id: String,
    lifecycle_id: String,
    session_id: Option<String>,
    status: String,
    execution_log: String,
    activity_state: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_activity_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<LifecycleRunRow> for LifecycleRun {
    type Error = DomainError;
    fn try_from(row: LifecycleRunRow) -> Result<Self, Self::Error> {
        let activity_state: Option<agentdash_domain::workflow::ActivityLifecycleRunState> = row
            .activity_state
            .as_deref()
            .map(|raw| parse_json_column(raw, "lifecycle_runs.activity_state"))
            .transpose()?;
        let active_node_keys: Vec<String> = if let Some(activity_state) = &activity_state {
            activity_state
                .attempts
                .iter()
                .filter(|attempt| {
                    matches!(
                        attempt.status,
                        agentdash_domain::workflow::ActivityAttemptStatus::Ready
                            | agentdash_domain::workflow::ActivityAttemptStatus::Claiming
                            | agentdash_domain::workflow::ActivityAttemptStatus::Running
                    )
                })
                .map(|attempt| attempt.activity_key.clone())
                .collect()
        } else {
            Vec::new()
        };
        Ok(LifecycleRun {
            id: parse_uuid(&row.id, "lifecycle_run")?,
            project_id: parse_uuid(&row.project_id, "project")?,
            lifecycle_id: parse_uuid(&row.lifecycle_id, "lifecycle_definition")?,
            session_id: row.session_id.filter(|s| !s.is_empty()),
            status: serde_json::from_str(&row.status)?,
            active_node_keys,
            execution_log: parse_json_column(&row.execution_log, "lifecycle_runs.execution_log")?,
            activity_state,
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

fn serialize_activity_state(
    activity_state: &Option<agentdash_domain::workflow::ActivityLifecycleRunState>,
) -> Result<Option<String>, DomainError> {
    activity_state
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
        AgentActivityExecutorSpec, AgentSessionPolicy, WorkflowContract,
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
                serde_json::to_string(&ExecutorRunRef::AgentSession {
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
            Some(ExecutorRunRef::AgentSession {
                session_id: "child-session".to_string()
            })
        );
    }

    fn test_workflow(project_id: uuid::Uuid, key: &str, digest: &str) -> WorkflowDefinition {
        let mut workflow = WorkflowDefinition::new(
            project_id,
            key,
            format!("Workflow {digest}"),
            "",
            vec![WorkflowBindingKind::Project],
            agentdash_domain::workflow::WorkflowDefinitionSource::UserAuthored,
            WorkflowContract::default(),
        )
        .expect("workflow");
        workflow.installed_source = Some(InstalledAssetSource::new(
            uuid::Uuid::new_v4(),
            "template",
            digest,
            format!("sha256:{digest}"),
        ));
        workflow
    }

    fn test_lifecycle(
        project_id: uuid::Uuid,
        key: &str,
        workflow_key: &str,
        digest: &str,
    ) -> ActivityLifecycleDefinition {
        let mut lifecycle = ActivityLifecycleDefinition::new(
            project_id,
            key,
            format!("Lifecycle {digest}"),
            "",
            vec![WorkflowBindingKind::Project],
            agentdash_domain::workflow::WorkflowDefinitionSource::UserAuthored,
            "plan",
            vec![ActivityDefinition {
                key: "plan".to_string(),
                description: String::new(),
                executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                    workflow_key: workflow_key.to_string(),
                    session_policy: AgentSessionPolicy::SpawnChild,
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
        let workflow_key = format!("wf_{}", uuid::Uuid::new_v4().simple());
        let lifecycle_key = format!("lc_{}", uuid::Uuid::new_v4().simple());

        repo.install_workflow_template_bundle(WorkflowTemplateInstallBundle {
            workflows: vec![test_workflow(project_id, &workflow_key, "v1")],
            lifecycle: test_lifecycle(project_id, &lifecycle_key, &workflow_key, "v1"),
            overwrite: false,
        })
        .await
        .expect("first install");

        let conflict = repo
            .install_workflow_template_bundle(WorkflowTemplateInstallBundle {
                workflows: vec![test_workflow(project_id, &workflow_key, "v2")],
                lifecycle: test_lifecycle(project_id, &lifecycle_key, &workflow_key, "v2"),
                overwrite: false,
            })
            .await
            .expect_err("conflict should fail without overwrite");
        assert!(conflict.to_string().contains("已存在"));

        let workflow_after_conflict =
            WorkflowDefinitionRepository::get_by_project_and_key(&repo, project_id, &workflow_key)
                .await
                .expect("get workflow")
                .expect("workflow exists");
        assert_eq!(workflow_after_conflict.version, 1);
        assert_eq!(
            workflow_after_conflict
                .installed_source
                .as_ref()
                .expect("source")
                .source_version,
            "v1"
        );

        let result = repo
            .install_workflow_template_bundle(WorkflowTemplateInstallBundle {
                workflows: vec![test_workflow(project_id, &workflow_key, "v2")],
                lifecycle: test_lifecycle(project_id, &lifecycle_key, &workflow_key, "v2"),
                overwrite: true,
            })
            .await
            .expect("overwrite install");

        assert_eq!(result.workflows[0].version, 2);
        assert_eq!(result.lifecycle.version, 2);
        let workflow =
            WorkflowDefinitionRepository::get_by_project_and_key(&repo, project_id, &workflow_key)
                .await
                .expect("get workflow")
                .expect("workflow exists");
        let lifecycle = ActivityLifecycleDefinitionRepository::get_by_project_and_key(
            &repo,
            project_id,
            &lifecycle_key,
        )
        .await
        .expect("get lifecycle")
        .expect("lifecycle exists");
        assert_eq!(workflow.version, 2);
        assert_eq!(lifecycle.version, 2);
        assert_eq!(
            workflow
                .installed_source
                .expect("workflow source")
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
