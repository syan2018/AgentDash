use agentdash_application_ports::agent_run_fork_materialization::{
    AgentRunForkMaterializationError, AgentRunForkMaterializationInput,
    AgentRunForkMaterializationPort, AgentRunForkMaterializationResult,
};
use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentFrame, AgentRunLineage, AgentRunLineageRepository, DeliveryBindingStatus, LifecycleAgent,
    LifecycleRun, RuntimeSessionExecutionAnchor,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

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
    let mut child_run =
        LifecycleRun::new_plain_for_user(input.parent_run.project_id, &input.forked_by_user_id);
    child_run.context = input.parent_run.context.clone();
    child_run.view_projection = input.parent_run.view_projection.clone();

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
    child_frame.effective_capability_json = input.parent_frame.effective_capability_json.clone();
    child_frame.context_slice_json = input.parent_frame.context_slice_json.clone();
    child_frame.vfs_surface_json = input.parent_frame.vfs_surface_json.clone();
    child_frame.mcp_surface_json = input.parent_frame.mcp_surface_json.clone();
    child_frame.execution_profile_json = input.parent_frame.execution_profile_json.clone();
    child_frame.visible_canvas_mount_ids_json =
        input.parent_frame.visible_canvas_mount_ids_json.clone();
    child_frame.visible_workspace_module_refs_json = input
        .parent_frame
        .visible_workspace_module_refs_json
        .clone();
    child_frame.created_by_id = Some(input.forked_by_user_id.clone());

    let mut anchor = RuntimeSessionExecutionAnchor::new_dispatch(
        input.child_runtime_session_id.clone(),
        child_run.id,
        child_frame.id,
        child_agent.id,
    );
    anchor.created_by_kind = "agent_run_fork".to_string();

    child_agent.bind_current_delivery_from_anchor(
        &anchor,
        DeliveryBindingStatus::Ready,
        chrono::Utc::now(),
    );

    let lineage = AgentRunLineage::new_fork(
        input.parent_run.id,
        input.parent_agent.id,
        child_run.id,
        child_agent.id,
        input.fork_point_event_seq,
        input.fork_point_ref_json,
        input.parent_runtime_session_id,
        input.child_runtime_session_id,
        input.forked_by_user_id,
        input.metadata_json,
    );

    let mut tx = pool.begin().await.map_err(db_err)?;
    insert_lifecycle_run_tx(&mut tx, &child_run).await?;
    insert_lifecycle_agent_tx(&mut tx, &child_agent).await?;
    insert_agent_frame_tx(&mut tx, &child_frame).await?;
    upsert_anchor_tx(&mut tx, &anchor).await?;
    insert_agent_run_lineage_tx(&mut tx, &lineage).await?;
    tx.commit().await.map_err(db_err)?;

    Ok(AgentRunForkMaterializationResult {
        child_run,
        child_agent,
        child_frame,
        lineage,
    })
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
        .bind(option_u64_to_i64(lineage.fork_point_event_seq)?)
        .bind(opt_json_str(&lineage.fork_point_ref_json)?)
        .bind(&lineage.parent_runtime_session_id)
        .bind(&lineage.child_runtime_session_id)
        .bind(&lineage.forked_by_user_id)
        .bind(opt_json_str(&lineage.metadata_json)?)
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
        .bind(option_u64_to_i64(lineage.fork_point_event_seq)?)
        .bind(opt_json_str(&lineage.fork_point_ref_json)?)
        .bind(&lineage.parent_runtime_session_id)
        .bind(&lineage.child_runtime_session_id)
        .bind(&lineage.forked_by_user_id)
        .bind(opt_json_str(&lineage.metadata_json)?)
        .bind(lineage.created_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| sql_err_for("agent_run_lineages", error))?;
    Ok(())
}

fn agent_run_lineage_insert_sql() -> &'static str {
    r#"INSERT INTO agent_run_lineages
        (id,parent_run_id,parent_agent_id,child_run_id,child_agent_id,relation_kind,
         fork_point_event_seq,fork_point_ref,parent_runtime_session_id,child_runtime_session_id,
         forked_by_user_id,metadata,created_at)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#
}

async fn insert_lifecycle_run_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run: &LifecycleRun,
) -> Result<(), DomainError> {
    sqlx::query(
        r#"INSERT INTO lifecycle_runs
            (id,project_id,created_by_user_id,topology,context,orchestrations,tasks,
             view_projection,status,execution_log,created_at,updated_at,last_activity_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
    )
    .bind(run.id.to_string())
    .bind(run.project_id.to_string())
    .bind(&run.created_by_user_id)
    .bind(match run.topology {
        agentdash_domain::workflow::LifecycleRunTopology::Plain => "plain",
        agentdash_domain::workflow::LifecycleRunTopology::WorkflowGraph => "workflow_graph",
    })
    .bind(serde_json::to_string(&run.context)?)
    .bind(serde_json::to_string(&run.orchestrations)?)
    .bind(serde_json::to_string(&run.tasks)?)
    .bind(opt_json_str(&run.view_projection)?)
    .bind(serde_json::to_string(&run.status)?)
    .bind(serde_json::to_string(&run.execution_log)?)
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
    let delivery = agent.current_delivery.as_ref();
    sqlx::query(
        r#"INSERT INTO lifecycle_agents
            (id,run_id,project_id,created_by_user_id,source,project_agent_id,status,bootstrap_status,
             current_delivery_runtime_session_id,current_delivery_launch_frame_id,
             current_delivery_orchestration_id,current_delivery_node_path,current_delivery_node_attempt,
             current_delivery_status,current_delivery_observed_at,created_at,updated_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)"#,
    )
    .bind(agent.id.to_string())
    .bind(agent.run_id.to_string())
    .bind(agent.project_id.to_string())
    .bind(&agent.created_by_user_id)
    .bind(agent.source.as_str())
    .bind(agent.project_agent_id.map(|id| id.to_string()))
    .bind(&agent.status)
    .bind(&agent.bootstrap_status)
    .bind(delivery.map(|binding| binding.runtime_session_id.clone()))
    .bind(delivery.map(|binding| binding.launch_frame_id.to_string()))
    .bind(delivery.and_then(|binding| binding.orchestration_id.map(|id| id.to_string())))
    .bind(delivery.and_then(|binding| binding.node_path.clone()))
    .bind(delivery.and_then(|binding| binding.node_attempt.map(|value| value as i32)))
    .bind(delivery.map(|binding| binding.status.as_str().to_string()))
    .bind(delivery.map(|binding| binding.observed_at))
    .bind(agent.created_at)
    .bind(agent.updated_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| sql_err_for("lifecycle_agents", error))?;
    Ok(())
}

async fn insert_agent_frame_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    frame: &AgentFrame,
) -> Result<(), DomainError> {
    sqlx::query(
        r#"INSERT INTO agent_frames
            (id,agent_id,revision,effective_capability_json,context_slice_json,vfs_surface_json,
             mcp_surface_json,visible_canvas_mount_ids_json,visible_workspace_module_refs_json,
             execution_profile_json,created_by_kind,created_by_id,created_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
    )
    .bind(frame.id.to_string())
    .bind(frame.agent_id.to_string())
    .bind(frame.revision)
    .bind(opt_json_str(&frame.effective_capability_json)?)
    .bind(opt_json_str(&frame.context_slice_json)?)
    .bind(opt_json_str(&frame.vfs_surface_json)?)
    .bind(opt_json_str(&frame.mcp_surface_json)?)
    .bind(opt_json_str(&frame.visible_canvas_mount_ids_json)?)
    .bind(opt_json_str(&frame.visible_workspace_module_refs_json)?)
    .bind(opt_json_str(&frame.execution_profile_json)?)
    .bind(&frame.created_by_kind)
    .bind(&frame.created_by_id)
    .bind(frame.created_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| sql_err_for("agent_frames", error))?;
    Ok(())
}

async fn upsert_anchor_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    anchor: &RuntimeSessionExecutionAnchor,
) -> Result<(), DomainError> {
    sqlx::query(
        r#"INSERT INTO runtime_session_execution_anchors
            (runtime_session_id,run_id,launch_frame_id,agent_id,orchestration_id,node_path,
             node_attempt,created_by_kind,created_at,updated_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
           ON CONFLICT (runtime_session_id) DO UPDATE SET
             run_id=EXCLUDED.run_id,
             launch_frame_id=EXCLUDED.launch_frame_id,
             agent_id=EXCLUDED.agent_id,
             orchestration_id=EXCLUDED.orchestration_id,
             node_path=EXCLUDED.node_path,
             node_attempt=EXCLUDED.node_attempt,
             created_by_kind=EXCLUDED.created_by_kind,
             updated_at=EXCLUDED.updated_at"#,
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
    Ok(())
}

const AGENT_RUN_LINEAGE_COLS: &str = "id,parent_run_id,parent_agent_id,child_run_id,child_agent_id,relation_kind,fork_point_event_seq,fork_point_ref,parent_runtime_session_id,child_runtime_session_id,forked_by_user_id,metadata,created_at";

#[derive(sqlx::FromRow)]
struct AgentRunLineageRow {
    id: String,
    parent_run_id: String,
    parent_agent_id: String,
    child_run_id: String,
    child_agent_id: String,
    relation_kind: String,
    fork_point_event_seq: Option<i64>,
    fork_point_ref: Option<String>,
    parent_runtime_session_id: String,
    child_runtime_session_id: String,
    forked_by_user_id: String,
    metadata: Option<String>,
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
            fork_point_event_seq: option_i64_to_u64(row.fork_point_event_seq)?,
            fork_point_ref_json: parse_optional_json(row.fork_point_ref, "fork_point_ref")?,
            parent_runtime_session_id: row.parent_runtime_session_id,
            child_runtime_session_id: row.child_runtime_session_id,
            forked_by_user_id: row.forked_by_user_id,
            metadata_json: parse_optional_json(row.metadata, "metadata")?,
            created_at: row.created_at,
        })
    }
}

fn parse_uuid(raw: &str, ctx: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{ctx} invalid uuid `{raw}`: {error}")))
}

fn opt_json_str(value: &Option<Value>) -> Result<Option<String>, DomainError> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn parse_optional_json(raw: Option<String>, column: &str) -> Result<Option<Value>, DomainError> {
    raw.map(|value| {
        serde_json::from_str(&value).map_err(|error| {
            DomainError::InvalidConfig(format!("agent_run_lineages.{column} JSON 无效: {error}"))
        })
    })
    .transpose()
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
        let child_run_id = Uuid::new_v4();
        let child_agent_id = Uuid::new_v4();
        let created_at = Utc::now();

        let lineage = AgentRunLineage::try_from(AgentRunLineageRow {
            id: Uuid::new_v4().to_string(),
            parent_run_id: parent_run_id.to_string(),
            parent_agent_id: parent_agent_id.to_string(),
            child_run_id: child_run_id.to_string(),
            child_agent_id: child_agent_id.to_string(),
            relation_kind: "fork".to_string(),
            fork_point_event_seq: Some(42),
            fork_point_ref: Some(json!({ "turn_id": "turn-1", "entry_index": 3 }).to_string()),
            parent_runtime_session_id: "runtime-parent".to_string(),
            child_runtime_session_id: "runtime-child".to_string(),
            forked_by_user_id: "user-child".to_string(),
            metadata: Some(json!({ "reason": "explore" }).to_string()),
            created_at,
        })
        .expect("lineage row should map");

        assert_eq!(lineage.parent_run_id, parent_run_id);
        assert_eq!(lineage.parent_agent_id, parent_agent_id);
        assert_eq!(lineage.child_run_id, child_run_id);
        assert_eq!(lineage.child_agent_id, child_agent_id);
        assert_eq!(lineage.relation_kind, "fork");
        assert_eq!(lineage.fork_point_event_seq, Some(42));
        assert_eq!(
            lineage.fork_point_ref_json,
            Some(json!({ "turn_id": "turn-1", "entry_index": 3 }))
        );
        assert_eq!(lineage.parent_runtime_session_id, "runtime-parent");
        assert_eq!(lineage.child_runtime_session_id, "runtime-child");
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
            fork_point_event_seq: Some(-1),
            fork_point_ref: None,
            parent_runtime_session_id: "runtime-parent".to_string(),
            child_runtime_session_id: "runtime-child".to_string(),
            forked_by_user_id: "user-child".to_string(),
            metadata: None,
            created_at: Utc::now(),
        };

        let error = AgentRunLineage::try_from(row).expect_err("negative seq should fail");
        assert!(error.to_string().contains("fork_point_event_seq"));
    }
}
