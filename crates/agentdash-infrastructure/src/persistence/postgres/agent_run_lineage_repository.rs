use agentdash_application_ports::agent_run_fork_materialization::{
    AgentRunForkMaterializationError, AgentRunForkMaterializationInput,
    AgentRunForkMaterializationPort, AgentRunForkMaterializationResult,
};
use agentdash_diagnostics::{DiagnosticErrorContext, Subsystem, diag_error};
use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentFrame, AgentRunDeliveryBinding, AgentRunLineage, AgentRunLineageRepository,
    DeliveryBindingStatus, LifecycleAgent, LifecycleRun, RuntimeSessionExecutionAnchor,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::json_document::{from_optional_jsonb, to_jsonb, to_optional_jsonb};
use super::{db_err, sql_err_for};

pub struct PostgresAgentRunLineageRepository {
    pool: PgPool,
}

impl PostgresAgentRunLineageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

pub struct PostgresAgentRunForkMaterialization {
    pool: PgPool,
}

impl PostgresAgentRunForkMaterialization {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AgentRunLineageRepository for PostgresAgentRunLineageRepository {
    async fn create(&self, lineage: &AgentRunLineage) -> Result<(), DomainError> {
        insert_agent_run_lineage(&self.pool, lineage).await
    }

    async fn find_parent(
        &self,
        child_run_id: Uuid,
        child_agent_id: Uuid,
    ) -> Result<Option<AgentRunLineage>, DomainError> {
        sqlx::query_as::<_, AgentRunLineageRow>(&format!(
            "SELECT {AGENT_RUN_LINEAGE_COLS} FROM agent_run_lineages \
             WHERE child_run_id=$1 AND child_agent_id=$2"
        ))
        .bind(child_run_id.to_string())
        .bind(child_agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_lineages", error))?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_children(
        &self,
        parent_run_id: Uuid,
        parent_agent_id: Uuid,
    ) -> Result<Vec<AgentRunLineage>, DomainError> {
        sqlx::query_as::<_, AgentRunLineageRow>(&format!(
            "SELECT {AGENT_RUN_LINEAGE_COLS} FROM agent_run_lineages \
             WHERE parent_run_id=$1 AND parent_agent_id=$2 ORDER BY created_at DESC"
        ))
        .bind(parent_run_id.to_string())
        .bind(parent_agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_lineages", error))?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentRunLineage>, DomainError> {
        sqlx::query_as::<_, AgentRunLineageRow>(&format!(
            "SELECT {AGENT_RUN_LINEAGE_COLS} FROM agent_run_lineages \
             WHERE parent_run_id=$1 OR child_run_id=$1 ORDER BY created_at DESC"
        ))
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_lineages", error))?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }
}

#[async_trait::async_trait]
impl AgentRunForkMaterializationPort for PostgresAgentRunForkMaterialization {
    async fn materialize_forked_agent_run(
        &self,
        input: AgentRunForkMaterializationInput,
    ) -> Result<AgentRunForkMaterializationResult, AgentRunForkMaterializationError> {
        materialize_forked_agent_run_tx(&self.pool, input)
            .await
            .map_err(|error| AgentRunForkMaterializationError::Internal {
                message: error.to_string(),
            })
    }
}

async fn materialize_forked_agent_run_tx(
    pool: &PgPool,
    input: AgentRunForkMaterializationInput,
) -> Result<AgentRunForkMaterializationResult, DomainError> {
    let parent_runtime_session_id = input.parent_runtime_session_id.clone();
    let child_runtime_session_id = input.child_runtime_session_id.clone();
    let child_run =
        LifecycleRun::new_plain_for_user(input.parent_run.project_id, &input.forked_by_user_id);

    let mut child_agent = LifecycleAgent::new_root_for_user(
        child_run.id,
        child_run.project_id,
        input.parent_agent.source,
        &input.forked_by_user_id,
    )
    .with_bootstrap_status(&input.parent_agent.bootstrap_status);
    if let Some(project_agent_id) = input.parent_agent.project_agent_id {
        child_agent = child_agent.with_project_agent(project_agent_id);
    }

    let mut child_frame =
        AgentFrame::new_revision(child_agent.id, 1, "agent_run_fork_materialization");
    child_frame.surface = Some(input.parent_frame.surface_document());
    child_frame.effective_capability_json = input.parent_frame.effective_capability_json.clone();
    child_frame.context_slice_json = input.parent_frame.context_slice_json.clone();
    child_frame.vfs_surface_json = input.parent_frame.vfs_surface_json.clone();
    child_frame.mcp_surface_json = input.parent_frame.mcp_surface_json.clone();
    child_frame.execution_profile_json = input.parent_frame.execution_profile_json.clone();
    child_frame.visible_workspace_module_refs_json = input
        .parent_frame
        .visible_workspace_module_refs_json
        .clone();
    child_frame.created_by_id = Some(input.forked_by_user_id.clone());

    let mut anchor = RuntimeSessionExecutionAnchor::new_dispatch(
        child_runtime_session_id.clone(),
        child_run.id,
        child_frame.id,
        child_agent.id,
    );
    anchor.created_by_kind = "agent_run_fork".to_string();

    let delivery_binding =
        AgentRunDeliveryBinding::from_anchor(&anchor, DeliveryBindingStatus::Ready, Utc::now());

    let lineage = AgentRunLineage::new_fork(
        input.parent_run.id,
        input.parent_agent.id,
        child_run.id,
        child_agent.id,
        input.fork_point_event_seq,
        input.fork_point_ref_json,
        input.forked_by_user_id,
        input.metadata_json,
    )
    .with_frame_baseline(
        input.parent_frame.id,
        input.parent_frame.revision,
        child_frame.id,
        child_frame.revision,
    );

    let context = AgentRunForkMaterializationLogContext {
        parent_run_id: input.parent_run.id,
        parent_agent_id: input.parent_agent.id,
        parent_frame_id: input.parent_frame.id,
        parent_runtime_session_id,
        child_run_id: child_run.id,
        child_agent_id: child_agent.id,
        child_frame_id: child_frame.id,
        child_runtime_session_id: child_runtime_session_id.clone(),
        lineage_id: lineage.id,
        forked_by_user_id: lineage.forked_by_user_id.clone(),
    };

    let mut tx = pool.begin().await.map_err(db_err).inspect_err(|error| {
        log_agent_run_fork_materialization_error("begin_transaction", &context, error);
    })?;
    insert_lifecycle_run_tx(&mut tx, &child_run)
        .await
        .inspect_err(|error| {
            log_agent_run_fork_materialization_error("insert_lifecycle_run", &context, error);
        })?;
    insert_lifecycle_agent_tx(&mut tx, &child_agent)
        .await
        .inspect_err(|error| {
            log_agent_run_fork_materialization_error("insert_lifecycle_agent", &context, error);
        })?;
    insert_agent_frame_tx(&mut tx, &child_frame)
        .await
        .inspect_err(|error| {
            log_agent_run_fork_materialization_error("insert_agent_frame", &context, error);
        })?;
    create_anchor_once_tx(&mut tx, &anchor)
        .await
        .inspect_err(|error| {
            log_agent_run_fork_materialization_error(
                "create_execution_anchor_once",
                &context,
                error,
            );
        })?;
    upsert_delivery_binding_tx(&mut tx, &delivery_binding)
        .await
        .inspect_err(|error| {
            log_agent_run_fork_materialization_error("upsert_delivery_binding", &context, error);
        })?;
    insert_agent_run_lineage_tx(&mut tx, &lineage)
        .await
        .inspect_err(|error| {
            log_agent_run_fork_materialization_error("insert_agent_run_lineage", &context, error);
        })?;
    tx.commit().await.map_err(db_err).inspect_err(|error| {
        log_agent_run_fork_materialization_error("commit_transaction", &context, error);
    })?;

    Ok(AgentRunForkMaterializationResult {
        child_run,
        child_agent,
        child_frame,
        child_runtime_session_id,
        lineage,
    })
}

#[derive(Debug, Clone)]
struct AgentRunForkMaterializationLogContext {
    parent_run_id: Uuid,
    parent_agent_id: Uuid,
    parent_frame_id: Uuid,
    parent_runtime_session_id: String,
    child_run_id: Uuid,
    child_agent_id: Uuid,
    child_frame_id: Uuid,
    child_runtime_session_id: String,
    lineage_id: Uuid,
    forked_by_user_id: String,
}

fn log_agent_run_fork_materialization_error(
    stage: &'static str,
    context: &AgentRunForkMaterializationLogContext,
    error: &DomainError,
) {
    let error_context = agent_run_fork_materialization_error_context(stage, context);
    diag_error!(Error, Subsystem::AgentRun,
        context = &error_context,
        error = error,
        parent_run_id = %context.parent_run_id,
        parent_agent_id = %context.parent_agent_id,
        parent_frame_id = %context.parent_frame_id,
        parent_runtime_session_id = %context.parent_runtime_session_id,
        child_run_id = %context.child_run_id,
        child_agent_id = %context.child_agent_id,
        child_frame_id = %context.child_frame_id,
        child_runtime_session_id = %context.child_runtime_session_id,
        lineage_id = %context.lineage_id,
        forked_by_user_id = %context.forked_by_user_id,
        "AgentRun fork materialization failed"
    );
}

fn agent_run_fork_materialization_error_context(
    stage: &str,
    _context: &AgentRunForkMaterializationLogContext,
) -> DiagnosticErrorContext {
    DiagnosticErrorContext::new("agent_run.fork_materialization", stage)
}

async fn insert_agent_run_lineage(
    pool: &PgPool,
    lineage: &AgentRunLineage,
) -> Result<(), DomainError> {
    sqlx::query(agent_run_lineage_insert_sql())
        .bind(lineage.id.to_string())
        .bind(lineage.parent_run_id.to_string())
        .bind(lineage.parent_agent_id.to_string())
        .bind(lineage.child_run_id.to_string())
        .bind(lineage.child_agent_id.to_string())
        .bind(&lineage.relation_kind)
        .bind(lineage.parent_frame_id.map(|id| id.to_string()))
        .bind(lineage.parent_frame_revision)
        .bind(lineage.child_frame_id.map(|id| id.to_string()))
        .bind(lineage.child_frame_revision)
        .bind(option_u64_to_i64(lineage.fork_point_event_seq)?)
        .bind(to_optional_jsonb(
            lineage.fork_point_ref_json.as_ref(),
            "agent_run_lineages.fork_point_ref",
        )?)
        .bind(&lineage.forked_by_user_id)
        .bind(to_optional_jsonb(
            lineage.metadata_json.as_ref(),
            "agent_run_lineages.metadata",
        )?)
        .bind(lineage.created_at)
        .execute(pool)
        .await
        .map_err(|error| sql_err_for("agent_run_lineages", error))?;
    Ok(())
}

async fn insert_agent_run_lineage_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    lineage: &AgentRunLineage,
) -> Result<(), DomainError> {
    sqlx::query(agent_run_lineage_insert_sql())
        .bind(lineage.id.to_string())
        .bind(lineage.parent_run_id.to_string())
        .bind(lineage.parent_agent_id.to_string())
        .bind(lineage.child_run_id.to_string())
        .bind(lineage.child_agent_id.to_string())
        .bind(&lineage.relation_kind)
        .bind(lineage.parent_frame_id.map(|id| id.to_string()))
        .bind(lineage.parent_frame_revision)
        .bind(lineage.child_frame_id.map(|id| id.to_string()))
        .bind(lineage.child_frame_revision)
        .bind(option_u64_to_i64(lineage.fork_point_event_seq)?)
        .bind(to_optional_jsonb(
            lineage.fork_point_ref_json.as_ref(),
            "agent_run_lineages.fork_point_ref",
        )?)
        .bind(&lineage.forked_by_user_id)
        .bind(to_optional_jsonb(
            lineage.metadata_json.as_ref(),
            "agent_run_lineages.metadata",
        )?)
        .bind(lineage.created_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| sql_err_for("agent_run_lineages", error))?;
    Ok(())
}

fn agent_run_lineage_insert_sql() -> &'static str {
    r#"INSERT INTO agent_run_lineages
        (id,parent_run_id,parent_agent_id,child_run_id,child_agent_id,relation_kind,
         parent_frame_id,parent_frame_revision,child_frame_id,child_frame_revision,
         fork_point_event_seq,fork_point_ref,forked_by_user_id,metadata,created_at)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)"#
}

async fn insert_lifecycle_run_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run: &LifecycleRun,
) -> Result<(), DomainError> {
    sqlx::query(
        r#"INSERT INTO lifecycle_runs
            (id,project_id,created_by_user_id,topology,orchestrations,tasks,
             status,execution_log,created_at,updated_at,last_activity_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
    )
    .bind(run.id.to_string())
    .bind(run.project_id.to_string())
    .bind(&run.created_by_user_id)
    .bind(match run.topology {
        agentdash_domain::workflow::LifecycleRunTopology::Plain => "plain",
        agentdash_domain::workflow::LifecycleRunTopology::WorkflowGraph => "workflow_graph",
    })
    .bind(to_jsonb(
        &run.orchestrations,
        "lifecycle_runs.orchestrations",
    )?)
    .bind(to_jsonb(&run.tasks, "lifecycle_runs.tasks")?)
    .bind(lifecycle_run_status_to_db(run.status))
    .bind(to_jsonb(
        &run.execution_log,
        "lifecycle_runs.execution_log",
    )?)
    .bind(run.created_at)
    .bind(run.updated_at)
    .bind(run.last_activity_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| sql_err_for("lifecycle_runs", error))?;
    Ok(())
}

async fn insert_lifecycle_agent_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    agent: &LifecycleAgent,
) -> Result<(), DomainError> {
    sqlx::query(
        r#"INSERT INTO lifecycle_agents
            (id,run_id,project_id,created_by_user_id,source,project_agent_id,
             status,bootstrap_status,created_at,updated_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
    )
    .bind(agent.id.to_string())
    .bind(agent.run_id.to_string())
    .bind(agent.project_id.to_string())
    .bind(&agent.created_by_user_id)
    .bind(agent.source.as_str())
    .bind(agent.project_agent_id.map(|id| id.to_string()))
    .bind(&agent.status)
    .bind(&agent.bootstrap_status)
    .bind(agent.created_at)
    .bind(agent.updated_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| sql_err_for("lifecycle_agents", error))?;
    Ok(())
}

async fn upsert_delivery_binding_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    binding: &AgentRunDeliveryBinding,
) -> Result<(), DomainError> {
    sqlx::query(
        r#"INSERT INTO agent_run_delivery_bindings
            (run_id,agent_id,runtime_session_id,launch_frame_id,orchestration_id,
             node_path,node_attempt,status,active_turn_id,last_turn_id,terminal_state,
             terminal_message,observed_at,updated_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
           ON CONFLICT (run_id, agent_id) DO UPDATE SET
               runtime_session_id = EXCLUDED.runtime_session_id,
               launch_frame_id = EXCLUDED.launch_frame_id,
               orchestration_id = EXCLUDED.orchestration_id,
               node_path = EXCLUDED.node_path,
               node_attempt = EXCLUDED.node_attempt,
               status = EXCLUDED.status,
               active_turn_id = EXCLUDED.active_turn_id,
               last_turn_id = EXCLUDED.last_turn_id,
               terminal_state = EXCLUDED.terminal_state,
               terminal_message = EXCLUDED.terminal_message,
               observed_at = EXCLUDED.observed_at,
               updated_at = EXCLUDED.updated_at"#,
    )
    .bind(binding.run_id.to_string())
    .bind(binding.agent_id.to_string())
    .bind(&binding.runtime_session_id)
    .bind(binding.launch_frame_id.to_string())
    .bind(binding.orchestration_id.map(|id| id.to_string()))
    .bind(&binding.node_path)
    .bind(binding.node_attempt.map(|value| value as i32))
    .bind(binding.status.as_str())
    .bind(&binding.active_turn_id)
    .bind(&binding.last_turn_id)
    .bind(&binding.terminal_state)
    .bind(&binding.terminal_message)
    .bind(binding.observed_at)
    .bind(binding.updated_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| sql_err_for("agent_run_delivery_bindings", error))?;
    Ok(())
}

async fn insert_agent_frame_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    frame: &AgentFrame,
) -> Result<(), DomainError> {
    let surface = frame.surface_document();
    sqlx::query(
        r#"INSERT INTO agent_frames
            (id,agent_id,revision,surface,effective_capability_json,context_slice_json,vfs_surface_json,
             mcp_surface_json,visible_workspace_module_refs_json,
             execution_profile_json,created_by_kind,created_by_id,created_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
    )
    .bind(frame.id.to_string())
    .bind(frame.agent_id.to_string())
    .bind(frame.revision)
    .bind(to_jsonb(
        &frame.surface_document(),
        "agent_frames.surface",
    )?)
    .bind(to_optional_jsonb(
        surface.capability_state.as_ref(),
        "agent_frames.effective_capability_json",
    )?)
    .bind(to_optional_jsonb(
        surface.context_slice.as_ref(),
        "agent_frames.context_slice_json",
    )?)
    .bind(to_optional_jsonb(
        surface.vfs_surface.as_ref(),
        "agent_frames.vfs_surface_json",
    )?)
    .bind(to_optional_jsonb(
        surface.mcp_surface.as_ref(),
        "agent_frames.mcp_surface_json",
    )?)
    .bind(to_optional_jsonb(
        surface.visible_workspace_module_refs.as_ref(),
        "agent_frames.visible_workspace_module_refs_json",
    )?)
    .bind(to_optional_jsonb(
        surface.execution_profile.as_ref(),
        "agent_frames.execution_profile_json",
    )?)
    .bind(&frame.created_by_kind)
    .bind(&frame.created_by_id)
    .bind(frame.created_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| sql_err_for("agent_frames", error))?;
    Ok(())
}

async fn create_anchor_once_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    anchor: &RuntimeSessionExecutionAnchor,
) -> Result<(), DomainError> {
    let result = sqlx::query(
        r#"INSERT INTO runtime_session_execution_anchors
            (runtime_session_id,run_id,launch_frame_id,agent_id,orchestration_id,node_path,
             node_attempt,created_by_kind,created_at,updated_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
           ON CONFLICT (runtime_session_id) DO NOTHING"#,
    )
    .bind(&anchor.runtime_session_id)
    .bind(anchor.run_id.to_string())
    .bind(anchor.launch_frame_id.to_string())
    .bind(anchor.agent_id.to_string())
    .bind(anchor.orchestration_id.map(|id| id.to_string()))
    .bind(&anchor.node_path)
    .bind(anchor.node_attempt.map(|value| value as i32))
    .bind(&anchor.created_by_kind)
    .bind(anchor.created_at)
    .bind(anchor.updated_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| sql_err_for("runtime_session_execution_anchors", error))?;
    if result.rows_affected() > 0 {
        return Ok(());
    }

    let existing = select_anchor_by_session_tx(tx, &anchor.runtime_session_id)
        .await?
        .ok_or_else(|| DomainError::Database {
            operation: "create_runtime_session_execution_anchor",
            message: format!(
                "runtime_session_id={} conflicted but existing anchor was not visible",
                anchor.runtime_session_id
            ),
        })?;
    if existing.has_same_launch_coordinates_as(anchor) {
        return Ok(());
    }
    Err(existing.immutable_conflict(anchor))
}

async fn select_anchor_by_session_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    runtime_session_id: &str,
) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
    sqlx::query_as::<_, ExecutionAnchorRow>(
        r#"SELECT runtime_session_id,run_id,launch_frame_id,agent_id,orchestration_id,node_path,
                  node_attempt,created_by_kind,created_at,updated_at
           FROM runtime_session_execution_anchors
           WHERE runtime_session_id = $1"#,
    )
    .bind(runtime_session_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| sql_err_for("runtime_session_execution_anchors", error))?
    .map(TryInto::try_into)
    .transpose()
}

const AGENT_RUN_LINEAGE_COLS: &str = "id,parent_run_id,parent_agent_id,child_run_id,child_agent_id,relation_kind,parent_frame_id,parent_frame_revision,child_frame_id,child_frame_revision,fork_point_event_seq,fork_point_ref,forked_by_user_id,metadata,created_at";

#[derive(sqlx::FromRow)]
struct ExecutionAnchorRow {
    runtime_session_id: String,
    run_id: String,
    launch_frame_id: String,
    agent_id: String,
    orchestration_id: Option<String>,
    node_path: Option<String>,
    node_attempt: Option<i32>,
    created_by_kind: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<ExecutionAnchorRow> for RuntimeSessionExecutionAnchor {
    type Error = DomainError;

    fn try_from(row: ExecutionAnchorRow) -> Result<Self, Self::Error> {
        Ok(Self {
            runtime_session_id: row.runtime_session_id,
            run_id: parse_uuid(&row.run_id, "runtime_session_execution_anchors.run_id")?,
            launch_frame_id: parse_uuid(
                &row.launch_frame_id,
                "runtime_session_execution_anchors.launch_frame_id",
            )?,
            agent_id: parse_uuid(&row.agent_id, "runtime_session_execution_anchors.agent_id")?,
            orchestration_id: opt_uuid(
                row.orchestration_id.as_ref(),
                "runtime_session_execution_anchors.orchestration_id",
            )?,
            node_path: row.node_path,
            node_attempt: row.node_attempt.map(|attempt| attempt as u32),
            created_by_kind: row.created_by_kind,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct AgentRunLineageRow {
    id: String,
    parent_run_id: String,
    parent_agent_id: String,
    child_run_id: String,
    child_agent_id: String,
    relation_kind: String,
    parent_frame_id: Option<String>,
    parent_frame_revision: Option<i32>,
    child_frame_id: Option<String>,
    child_frame_revision: Option<i32>,
    fork_point_event_seq: Option<i64>,
    fork_point_ref: Option<Value>,
    forked_by_user_id: String,
    metadata: Option<Value>,
    created_at: DateTime<Utc>,
}

impl TryFrom<AgentRunLineageRow> for AgentRunLineage {
    type Error = DomainError;

    fn try_from(row: AgentRunLineageRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(&row.id, "agent_run_lineages.id")?,
            parent_run_id: parse_uuid(&row.parent_run_id, "agent_run_lineages.parent_run_id")?,
            parent_agent_id: parse_uuid(
                &row.parent_agent_id,
                "agent_run_lineages.parent_agent_id",
            )?,
            child_run_id: parse_uuid(&row.child_run_id, "agent_run_lineages.child_run_id")?,
            child_agent_id: parse_uuid(&row.child_agent_id, "agent_run_lineages.child_agent_id")?,
            relation_kind: row.relation_kind,
            parent_frame_id: opt_uuid(
                row.parent_frame_id.as_ref(),
                "agent_run_lineages.parent_frame_id",
            )?,
            parent_frame_revision: row.parent_frame_revision,
            child_frame_id: opt_uuid(
                row.child_frame_id.as_ref(),
                "agent_run_lineages.child_frame_id",
            )?,
            child_frame_revision: row.child_frame_revision,
            fork_point_event_seq: option_i64_to_u64(row.fork_point_event_seq)?,
            fork_point_ref_json: from_optional_jsonb(
                row.fork_point_ref,
                "agent_run_lineages.fork_point_ref",
            )?,
            forked_by_user_id: row.forked_by_user_id,
            metadata_json: from_optional_jsonb(row.metadata, "agent_run_lineages.metadata")?,
            created_at: row.created_at,
        })
    }
}

fn parse_uuid(raw: &str, ctx: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{ctx} invalid uuid `{raw}`: {error}")))
}

fn opt_uuid(raw: Option<&String>, ctx: &str) -> Result<Option<Uuid>, DomainError> {
    raw.map(|value| parse_uuid(value, ctx)).transpose()
}

fn option_u64_to_i64(value: Option<u64>) -> Result<Option<i64>, DomainError> {
    value
        .map(|value| {
            i64::try_from(value).map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "agent_run_lineages.fork_point_event_seq 超出 bigint 范围: {value}"
                ))
            })
        })
        .transpose()
}

fn lifecycle_run_status_to_db(
    status: agentdash_domain::workflow::LifecycleRunStatus,
) -> &'static str {
    match status {
        agentdash_domain::workflow::LifecycleRunStatus::Draft => "draft",
        agentdash_domain::workflow::LifecycleRunStatus::Ready => "ready",
        agentdash_domain::workflow::LifecycleRunStatus::Running => "running",
        agentdash_domain::workflow::LifecycleRunStatus::Blocked => "blocked",
        agentdash_domain::workflow::LifecycleRunStatus::Completed => "completed",
        agentdash_domain::workflow::LifecycleRunStatus::Failed => "failed",
        agentdash_domain::workflow::LifecycleRunStatus::Cancelled => "cancelled",
    }
}

fn option_i64_to_u64(value: Option<i64>) -> Result<Option<u64>, DomainError> {
    value
        .map(|value| {
            u64::try_from(value).map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "agent_run_lineages.fork_point_event_seq 不能为负数: {value}"
                ))
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_run_lineage_row_maps_json_and_refs() {
        let parent_run_id = Uuid::new_v4();
        let parent_agent_id = Uuid::new_v4();
        let parent_frame_id = Uuid::new_v4();
        let child_run_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let child_frame_id = Uuid::new_v4();
        let created_at = Utc::now();

        let lineage = AgentRunLineage::try_from(AgentRunLineageRow {
            id: Uuid::new_v4().to_string(),
            parent_run_id: parent_run_id.to_string(),
            parent_agent_id: parent_agent_id.to_string(),
            child_run_id: child_run_id.to_string(),
            child_agent_id: child_agent_id.to_string(),
            relation_kind: "fork".to_string(),
            parent_frame_id: Some(parent_frame_id.to_string()),
            parent_frame_revision: Some(7),
            child_frame_id: Some(child_frame_id.to_string()),
            child_frame_revision: Some(1),
            fork_point_event_seq: Some(42),
            fork_point_ref: Some(json!({ "turn_id": "turn-1", "entry_index": 3 })),
            forked_by_user_id: "user-child".to_string(),
            metadata: Some(json!({ "reason": "explore" })),
            created_at,
        })
        .expect("lineage row should map");

        assert_eq!(lineage.parent_run_id, parent_run_id);
        assert_eq!(lineage.parent_agent_id, parent_agent_id);
        assert_eq!(lineage.child_run_id, child_run_id);
        assert_eq!(lineage.child_agent_id, child_agent_id);
        assert_eq!(lineage.relation_kind, "fork");
        assert_eq!(lineage.parent_frame_id, Some(parent_frame_id));
        assert_eq!(lineage.parent_frame_revision, Some(7));
        assert_eq!(lineage.child_frame_id, Some(child_frame_id));
        assert_eq!(lineage.child_frame_revision, Some(1));
        assert_eq!(lineage.fork_point_event_seq, Some(42));
        assert_eq!(
            lineage.fork_point_ref_json,
            Some(json!({ "turn_id": "turn-1", "entry_index": 3 }))
        );
        assert_eq!(lineage.forked_by_user_id, "user-child");
        assert_eq!(lineage.metadata_json, Some(json!({ "reason": "explore" })));
        assert_eq!(lineage.created_at, created_at);
    }

    #[test]
    fn agent_run_lineage_row_rejects_negative_fork_point_event_seq() {
        let row = AgentRunLineageRow {
            id: Uuid::new_v4().to_string(),
            parent_run_id: Uuid::new_v4().to_string(),
            parent_agent_id: Uuid::new_v4().to_string(),
            child_run_id: Uuid::new_v4().to_string(),
            child_agent_id: Uuid::new_v4().to_string(),
            relation_kind: "fork".to_string(),
            parent_frame_id: None,
            parent_frame_revision: None,
            child_frame_id: None,
            child_frame_revision: None,
            fork_point_event_seq: Some(-1),
            fork_point_ref: None,
            forked_by_user_id: "user-child".to_string(),
            metadata: None,
            created_at: Utc::now(),
        };

        let error = AgentRunLineage::try_from(row).expect_err("negative seq should fail");
        assert!(error.to_string().contains("fork_point_event_seq"));
    }
}
