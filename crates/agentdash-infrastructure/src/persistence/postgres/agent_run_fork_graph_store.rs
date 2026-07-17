use agentdash_application_ports::agent_run_fork::{AgentRunForkGraph, AgentRunForkGraphStore};
use agentdash_domain::workflow::{LifecycleRunStatus, LifecycleRunTopology};
use async_trait::async_trait;
use sqlx::PgPool;

pub struct PostgresAgentRunForkGraphStore {
    pool: PgPool,
}

impl PostgresAgentRunForkGraphStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentRunForkGraphStore for PostgresAgentRunForkGraphStore {
    async fn create_graph(&self, graph: &AgentRunForkGraph) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;
        let run = &graph.child_run;
        sqlx::query(
            "INSERT INTO lifecycle_runs \
             (id,project_id,created_by_user_id,topology,orchestrations,tasks,status,execution_log,channel_registry,created_at,updated_at,last_activity_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(run.id.to_string())
        .bind(run.project_id.to_string())
        .bind(&run.created_by_user_id)
        .bind(topology(run.topology))
        .bind(serde_json::to_value(&run.orchestrations).map_err(|error| error.to_string())?)
        .bind(serde_json::to_value(&run.tasks).map_err(|error| error.to_string())?)
        .bind(status(run.status))
        .bind(serde_json::to_value(&run.execution_log).map_err(|error| error.to_string())?)
        .bind(serde_json::to_value(&run.channel_registry).map_err(|error| error.to_string())?)
        .bind(run.created_at)
        .bind(run.updated_at)
        .bind(run.last_activity_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

        let agent = &graph.child_agent;
        sqlx::query(
            "INSERT INTO lifecycle_agents \
             (id,run_id,project_id,created_by_user_id,source,project_agent_id,status,bootstrap_status,workspace_title,workspace_title_source,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(agent.id.to_string())
        .bind(agent.run_id.to_string())
        .bind(agent.project_id.to_string())
        .bind(&agent.created_by_user_id)
        .bind(agent.source.as_str())
        .bind(agent.project_agent_id.map(|id| id.to_string()))
        .bind(&agent.status)
        .bind(&agent.bootstrap_status)
        .bind(&agent.workspace_title)
        .bind(&agent.workspace_title_source)
        .bind(agent.created_at)
        .bind(agent.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

        let frame = &graph.child_frame;
        let surface = frame.surface_document();
        sqlx::query(
            "INSERT INTO agent_frames \
             (id,agent_id,revision,surface,effective_capability_json,context_slice_json,vfs_surface_json,mcp_surface_json,execution_profile_json,hook_plan,created_by_kind,created_by_id,created_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
        )
        .bind(frame.id.to_string())
        .bind(frame.agent_id.to_string())
        .bind(frame.revision)
        .bind(serde_json::to_value(&surface).map_err(|error| error.to_string())?)
        .bind(surface.capability_state)
        .bind(surface.context_slice)
        .bind(surface.vfs_surface)
        .bind(surface.mcp_surface)
        .bind(surface.execution_profile)
        .bind(surface.hook_plan)
        .bind(&frame.created_by_kind)
        .bind(&frame.created_by_id)
        .bind(frame.created_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

        let lineage = &graph.lineage;
        sqlx::query(
            "INSERT INTO agent_run_lineages \
             (id,parent_run_id,parent_agent_id,child_run_id,child_agent_id,relation_kind,parent_frame_id,parent_frame_revision,child_frame_id,child_frame_revision,fork_point_event_seq,fork_point_ref,forked_by_user_id,metadata,created_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
        )
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
        .bind(lineage.fork_point_event_seq.map(|value| value as i64))
        .bind(&lineage.fork_point_ref_json)
        .bind(&lineage.forked_by_user_id)
        .bind(&lineage.metadata_json)
        .bind(lineage.created_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        tx.commit().await.map_err(|error| error.to_string())
    }

    async fn delete_graph(&self, graph: &AgentRunForkGraph) -> Result<(), String> {
        sqlx::query("DELETE FROM lifecycle_runs WHERE id=$1")
            .bind(graph.child_run.id.to_string())
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn topology(value: LifecycleRunTopology) -> &'static str {
    match value {
        LifecycleRunTopology::Plain => "plain",
        LifecycleRunTopology::WorkflowGraph => "workflow_graph",
    }
}

fn status(value: LifecycleRunStatus) -> &'static str {
    match value {
        LifecycleRunStatus::Draft => "draft",
        LifecycleRunStatus::Ready => "ready",
        LifecycleRunStatus::Running => "running",
        LifecycleRunStatus::Blocked => "blocked",
        LifecycleRunStatus::Completed => "completed",
        LifecycleRunStatus::Failed => "failed",
        LifecycleRunStatus::Cancelled => "cancelled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{
        AgentFrame, AgentRunLineage, AgentSource, LifecycleAgent, LifecycleRun,
    };
    use uuid::Uuid;

    #[tokio::test]
    async fn fork_graph_transaction_rolls_back_partial_rows_and_delete_cascades() {
        let (pool, _runtime) = test_pool().await;
        let store = PostgresAgentRunForkGraphStore::new(pool.clone());
        let parent = seed_parent(&pool).await;
        let valid = graph(parent.0, parent.1);

        store.create_graph(&valid).await.expect("create graph");
        assert_graph_counts(&pool, &valid, [1, 1, 1, 1]).await;
        store.delete_graph(&valid).await.expect("delete graph");
        assert_graph_counts(&pool, &valid, [0, 0, 0, 0]).await;

        let invalid = graph(Uuid::new_v4(), Uuid::new_v4());
        store
            .create_graph(&invalid)
            .await
            .expect_err("lineage foreign key must fail");
        assert_graph_counts(&pool, &invalid, [0, 0, 0, 0]).await;
    }

    async fn test_pool() -> (PgPool, Option<crate::postgres_runtime::PostgresRuntime>) {
        if crate::persistence::postgres::test_database_url().is_some() {
            return (
                crate::persistence::postgres::test_pg_pool("agent run fork graph uow")
                    .await
                    .expect("configured PostgreSQL test pool"),
                None,
            );
        }
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/fork-graph-postgres-tests");
        let runtime = crate::postgres_runtime::PostgresRuntime::resolve_embedded_at_data_root(
            "fork-graph-tests",
            58,
            data_root,
        )
        .await
        .expect("start embedded PostgreSQL for fork graph tests");
        let database_name = format!("fork_graph_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {database_name}"))
            .execute(&runtime.pool)
            .await
            .expect("create isolated fork graph database");
        let options = runtime
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .database(&database_name);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .expect("connect isolated fork graph database");
        crate::migration::run_postgres_migrations(&pool)
            .await
            .expect("migrate isolated fork graph database");
        crate::migration::assert_postgres_schema_ready(&pool)
            .await
            .expect("fork graph schema readiness");
        (pool, Some(runtime))
    }

    async fn seed_parent(pool: &PgPool) -> (Uuid, Uuid) {
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain_for_user(project_id, "parent-user".to_string());
        let agent = LifecycleAgent::new_root_for_user(
            run.id,
            project_id,
            AgentSource::ProjectAgent,
            "parent-user".to_string(),
        );
        let graph = graph_with_entities(run, agent, Uuid::new_v4(), Uuid::new_v4());
        sqlx::query(
            "INSERT INTO lifecycle_runs (id,project_id,created_by_user_id,topology,orchestrations,tasks,status,execution_log,channel_registry,created_at,updated_at,last_activity_at) VALUES ($1,$2,$3,'plain',$4,$5,'ready',$6,$7,$8,$9,$10)",
        )
        .bind(graph.child_run.id.to_string())
        .bind(graph.child_run.project_id.to_string())
        .bind(&graph.child_run.created_by_user_id)
        .bind(serde_json::json!([])).bind(serde_json::json!([])).bind(serde_json::json!([]))
        .bind(serde_json::to_value(&graph.child_run.channel_registry).expect("channel registry"))
        .bind(graph.child_run.created_at).bind(graph.child_run.updated_at).bind(graph.child_run.last_activity_at)
        .execute(pool).await.expect("parent run");
        sqlx::query("INSERT INTO lifecycle_agents (id,run_id,project_id,created_by_user_id,source,status,bootstrap_status,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(graph.child_agent.id.to_string()).bind(graph.child_agent.run_id.to_string())
            .bind(graph.child_agent.project_id.to_string()).bind(&graph.child_agent.created_by_user_id)
            .bind(graph.child_agent.source.as_str()).bind(&graph.child_agent.status)
            .bind(&graph.child_agent.bootstrap_status).bind(graph.child_agent.created_at).bind(graph.child_agent.updated_at)
            .execute(pool).await.expect("parent agent");
        (graph.child_run.id, graph.child_agent.id)
    }

    fn graph(parent_run_id: Uuid, parent_agent_id: Uuid) -> AgentRunForkGraph {
        let project_id = Uuid::new_v4();
        let run = LifecycleRun::new_plain_for_user(project_id, "child-user".to_string());
        let agent = LifecycleAgent::new_root_for_user(
            run.id,
            project_id,
            AgentSource::ProjectAgent,
            "child-user".to_string(),
        );
        graph_with_entities(run, agent, parent_run_id, parent_agent_id)
    }

    fn graph_with_entities(
        run: LifecycleRun,
        agent: LifecycleAgent,
        parent_run_id: Uuid,
        parent_agent_id: Uuid,
    ) -> AgentRunForkGraph {
        let frame = AgentFrame::new_initial(agent.id);
        let lineage = AgentRunLineage::new_fork(
            parent_run_id,
            parent_agent_id,
            run.id,
            agent.id,
            None,
            None,
            "child-user".to_string(),
            None,
        );
        AgentRunForkGraph {
            child_run: run,
            child_agent: agent,
            child_frame: frame,
            lineage,
        }
    }

    async fn assert_graph_counts(pool: &PgPool, graph: &AgentRunForkGraph, expected: [i64; 4]) {
        let run: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lifecycle_runs WHERE id=$1")
            .bind(graph.child_run.id.to_string())
            .fetch_one(pool)
            .await
            .expect("run count");
        let agent: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lifecycle_agents WHERE id=$1")
            .bind(graph.child_agent.id.to_string())
            .fetch_one(pool)
            .await
            .expect("agent count");
        let frame: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_frames WHERE id=$1")
            .bind(graph.child_frame.id.to_string())
            .fetch_one(pool)
            .await
            .expect("frame count");
        let lineage: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM agent_run_lineages WHERE id=$1")
                .bind(graph.lineage.id.to_string())
                .fetch_one(pool)
                .await
                .expect("lineage count");
        assert_eq!([run, agent, frame, lineage], expected);
    }
}
