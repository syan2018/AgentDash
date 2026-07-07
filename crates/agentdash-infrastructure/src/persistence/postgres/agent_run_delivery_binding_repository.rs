use std::str::FromStr;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentRunDeliveryBinding, AgentRunDeliveryBindingRepository, DeliveryBindingStatus,
};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::db_err;

pub struct PostgresAgentRunDeliveryBindingRepository {
    pool: PgPool,
}

impl PostgresAgentRunDeliveryBindingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct DeliveryBindingRow {
    run_id: String,
    agent_id: String,
    runtime_session_id: String,
    launch_frame_id: String,
    orchestration_id: Option<String>,
    node_path: Option<String>,
    node_attempt: Option<i32>,
    status: String,
    active_turn_id: Option<String>,
    last_turn_id: Option<String>,
    terminal_state: Option<String>,
    terminal_message: Option<String>,
    terminal_diagnostic: Option<serde_json::Value>,
    observed_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn parse_uuid(s: &str, ctx: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(s)
        .map_err(|e| DomainError::InvalidConfig(format!("{ctx}: invalid uuid `{s}`: {e}")))
}

fn opt_uuid(s: Option<&String>, ctx: &str) -> Result<Option<Uuid>, DomainError> {
    match s {
        Some(value) => Ok(Some(parse_uuid(value, ctx)?)),
        None => Ok(None),
    }
}

impl TryFrom<DeliveryBindingRow> for AgentRunDeliveryBinding {
    type Error = DomainError;

    fn try_from(row: DeliveryBindingRow) -> Result<Self, Self::Error> {
        let status = DeliveryBindingStatus::from_str(&row.status).map_err(|_| {
            DomainError::InvalidConfig(format!(
                "agent_run_delivery_bindings.status invalid slug `{}`",
                row.status
            ))
        })?;
        let node_attempt = match row.node_attempt {
            Some(value) => Some(u32::try_from(value).map_err(|_| {
                DomainError::InvalidConfig(format!(
                    "agent_run_delivery_bindings.node_attempt invalid value `{value}`"
                ))
            })?),
            None => None,
        };
        Ok(AgentRunDeliveryBinding {
            run_id: parse_uuid(&row.run_id, "agent_run_delivery_bindings.run_id")?,
            agent_id: parse_uuid(&row.agent_id, "agent_run_delivery_bindings.agent_id")?,
            runtime_session_id: row.runtime_session_id,
            launch_frame_id: parse_uuid(
                &row.launch_frame_id,
                "agent_run_delivery_bindings.launch_frame_id",
            )?,
            orchestration_id: opt_uuid(
                row.orchestration_id.as_ref(),
                "agent_run_delivery_bindings.orchestration_id",
            )?,
            node_path: row.node_path,
            node_attempt,
            status,
            active_turn_id: row.active_turn_id,
            last_turn_id: row.last_turn_id,
            terminal_state: row.terminal_state,
            terminal_message: row.terminal_message,
            terminal_diagnostic: row.terminal_diagnostic,
            observed_at: row.observed_at,
            updated_at: row.updated_at,
        })
    }
}

#[async_trait::async_trait]
impl AgentRunDeliveryBindingRepository for PostgresAgentRunDeliveryBindingRepository {
    async fn upsert(&self, binding: &AgentRunDeliveryBinding) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO agent_run_delivery_bindings
                (run_id, agent_id, runtime_session_id, launch_frame_id,
                 orchestration_id, node_path, node_attempt, status,
                 active_turn_id, last_turn_id, terminal_state, terminal_message,
                 terminal_diagnostic, observed_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
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
                   terminal_diagnostic = EXCLUDED.terminal_diagnostic,
                   observed_at = EXCLUDED.observed_at,
                   updated_at = EXCLUDED.updated_at"#,
        )
        .bind(binding.run_id.to_string())
        .bind(binding.agent_id.to_string())
        .bind(&binding.runtime_session_id)
        .bind(binding.launch_frame_id.to_string())
        .bind(binding.orchestration_id.map(|id| id.to_string()))
        .bind(&binding.node_path)
        .bind(binding.node_attempt.map(|attempt| attempt as i32))
        .bind(binding.status.as_str())
        .bind(&binding.active_turn_id)
        .bind(&binding.last_turn_id)
        .bind(&binding.terminal_state)
        .bind(&binding.terminal_message)
        .bind(&binding.terminal_diagnostic)
        .bind(binding.observed_at)
        .bind(binding.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn upsert_if_current_runtime_session(
        &self,
        binding: &AgentRunDeliveryBinding,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            r#"INSERT INTO agent_run_delivery_bindings
                (run_id, agent_id, runtime_session_id, launch_frame_id,
                 orchestration_id, node_path, node_attempt, status,
                 active_turn_id, last_turn_id, terminal_state, terminal_message,
                 terminal_diagnostic, observed_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
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
                   terminal_diagnostic = EXCLUDED.terminal_diagnostic,
                   observed_at = EXCLUDED.observed_at,
                   updated_at = EXCLUDED.updated_at
               WHERE agent_run_delivery_bindings.runtime_session_id = EXCLUDED.runtime_session_id"#,
        )
        .bind(binding.run_id.to_string())
        .bind(binding.agent_id.to_string())
        .bind(&binding.runtime_session_id)
        .bind(binding.launch_frame_id.to_string())
        .bind(binding.orchestration_id.map(|id| id.to_string()))
        .bind(&binding.node_path)
        .bind(binding.node_attempt.map(|attempt| attempt as i32))
        .bind(binding.status.as_str())
        .bind(&binding.active_turn_id)
        .bind(&binding.last_turn_id)
        .bind(&binding.terminal_state)
        .bind(&binding.terminal_message)
        .bind(&binding.terminal_diagnostic)
        .bind(binding.observed_at)
        .bind(binding.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_current(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<AgentRunDeliveryBinding>, DomainError> {
        sqlx::query_as::<_, DeliveryBindingRow>(
            r#"SELECT run_id, agent_id, runtime_session_id, launch_frame_id,
                      orchestration_id, node_path, node_attempt, status,
                      active_turn_id, last_turn_id, terminal_state, terminal_message,
                      terminal_diagnostic,
                      observed_at, updated_at
               FROM agent_run_delivery_bindings
               WHERE run_id = $1 AND agent_id = $2"#,
        )
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentRunDeliveryBinding>, DomainError> {
        sqlx::query_as::<_, DeliveryBindingRow>(
            r#"SELECT run_id, agent_id, runtime_session_id, launch_frame_id,
                      orchestration_id, node_path, node_attempt, status,
                      active_turn_id, last_turn_id, terminal_state, terminal_message,
                      terminal_diagnostic,
                      observed_at, updated_at
               FROM agent_run_delivery_bindings
               WHERE run_id = $1
               ORDER BY updated_at DESC, runtime_session_id DESC"#,
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM agent_run_delivery_bindings WHERE runtime_session_id = $1")
            .bind(runtime_session_id)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::postgres::test_pg_pool;
    use agentdash_domain::workflow::{
        AgentFrame, AgentSource, LifecycleAgent, LifecycleRun, RuntimeSessionExecutionAnchor,
        RuntimeSessionExecutionAnchorRepository,
    };

    async fn seed_binding_prerequisites(
        pool: &PgPool,
        run: &LifecycleRun,
        agent: &LifecycleAgent,
        frame: &AgentFrame,
        runtime_session_id: &str,
    ) {
        sqlx::query(
            r#"INSERT INTO lifecycle_runs
                (id, project_id, created_by_user_id, topology, orchestrations, tasks,
                 status, execution_log, created_at, updated_at, last_activity_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
        )
        .bind(run.id.to_string())
        .bind(run.project_id.to_string())
        .bind(&run.created_by_user_id)
        .bind("plain")
        .bind("[]")
        .bind("[]")
        .bind("\"ready\"")
        .bind("[]")
        .bind(run.created_at)
        .bind(run.updated_at)
        .bind(run.last_activity_at)
        .execute(pool)
        .await
        .expect("insert lifecycle run");

        sqlx::query(
            r#"INSERT INTO lifecycle_agents
                (id, run_id, project_id, created_by_user_id, source, status,
                 bootstrap_status, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(agent.id.to_string())
        .bind(agent.run_id.to_string())
        .bind(agent.project_id.to_string())
        .bind(&agent.created_by_user_id)
        .bind(agent.source.as_str())
        .bind(&agent.status)
        .bind(&agent.bootstrap_status)
        .bind(agent.created_at)
        .bind(agent.updated_at)
        .execute(pool)
        .await
        .expect("insert lifecycle agent");

        sqlx::query(
            r#"INSERT INTO agent_frames
                (id, agent_id, revision, created_by_kind, created_by_id, created_at)
               VALUES ($1,$2,$3,$4,$5,$6)"#,
        )
        .bind(frame.id.to_string())
        .bind(frame.agent_id.to_string())
        .bind(frame.revision)
        .bind(&frame.created_by_kind)
        .bind(&frame.created_by_id)
        .bind(frame.created_at)
        .execute(pool)
        .await
        .expect("insert agent frame");

        sqlx::query(
            r#"INSERT INTO runtime_sessions (id, created_at, updated_at, last_delivery_status)
               VALUES ($1,$2,$3,$4)
               ON CONFLICT (id) DO NOTHING"#,
        )
        .bind(runtime_session_id)
        .bind(Utc::now().timestamp_millis())
        .bind(Utc::now().timestamp_millis())
        .bind("running")
        .execute(pool)
        .await
        .expect("insert runtime session");
    }

    #[tokio::test]
    async fn upsert_get_list_and_delete_by_session_round_trip() {
        let Some(pool) = test_pg_pool("agent_run_delivery_binding").await else {
            return;
        };
        let repo = PostgresAgentRunDeliveryBindingRepository::new(pool.clone());
        let anchor_repo =
            crate::persistence::postgres::PostgresRuntimeSessionExecutionAnchorRepository::new(
                pool.clone(),
            );

        let run = LifecycleRun::new_plain(Uuid::new_v4());
        let agent = LifecycleAgent::new_root(run.id, run.project_id, AgentSource::ProjectAgent);
        let frame = AgentFrame::new_revision(agent.id, 1, "test");
        let runtime_session_id = format!("runtime-{}", Uuid::new_v4());
        seed_binding_prerequisites(&pool, &run, &agent, &frame, &runtime_session_id).await;

        let anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            runtime_session_id.clone(),
            run.id,
            frame.id,
            agent.id,
        );
        anchor_repo
            .create_once(&anchor)
            .await
            .expect("create anchor");

        let observed_at = Utc::now();
        let binding = AgentRunDeliveryBinding::from_anchor(
            &anchor,
            DeliveryBindingStatus::Running,
            observed_at,
        );
        repo.upsert(&binding).await.expect("upsert binding");

        let current = repo
            .get_current(run.id, agent.id)
            .await
            .expect("get binding")
            .expect("binding exists");
        assert_eq!(current.runtime_session_id, runtime_session_id);
        assert_eq!(current.launch_frame_id, frame.id);
        assert_eq!(current.status, DeliveryBindingStatus::Running);

        let listed = repo.list_by_run(run.id).await.expect("list bindings");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].agent_id, agent.id);

        let replacement_session_id = format!("runtime-{}", Uuid::new_v4());
        let replacement_frame = AgentFrame::new_revision(agent.id, 2, "test");
        sqlx::query(
            r#"INSERT INTO agent_frames
                (id, agent_id, revision, created_by_kind, created_by_id, created_at)
               VALUES ($1,$2,$3,$4,$5,$6)"#,
        )
        .bind(replacement_frame.id.to_string())
        .bind(replacement_frame.agent_id.to_string())
        .bind(replacement_frame.revision)
        .bind(&replacement_frame.created_by_kind)
        .bind(&replacement_frame.created_by_id)
        .bind(replacement_frame.created_at)
        .execute(&pool)
        .await
        .expect("insert replacement frame");
        sqlx::query(
            r#"INSERT INTO runtime_sessions (id, created_at, updated_at, last_delivery_status)
               VALUES ($1,$2,$3,$4)"#,
        )
        .bind(&replacement_session_id)
        .bind(Utc::now().timestamp_millis())
        .bind(Utc::now().timestamp_millis())
        .bind("idle")
        .execute(&pool)
        .await
        .expect("insert replacement session");
        let replacement_anchor = RuntimeSessionExecutionAnchor::new_dispatch(
            replacement_session_id.clone(),
            run.id,
            replacement_frame.id,
            agent.id,
        );
        anchor_repo
            .create_once(&replacement_anchor)
            .await
            .expect("create replacement anchor");
        let replacement = AgentRunDeliveryBinding::from_anchor(
            &replacement_anchor,
            DeliveryBindingStatus::Ready,
            Utc::now(),
        );
        repo.upsert(&replacement).await.expect("replace binding");

        let current = repo
            .get_current(run.id, agent.id)
            .await
            .expect("get replacement binding")
            .expect("replacement binding exists");
        assert_eq!(current.runtime_session_id, replacement_session_id);
        assert_eq!(current.status, DeliveryBindingStatus::Ready);

        let stale_terminal = AgentRunDeliveryBinding::from_anchor(
            &anchor,
            DeliveryBindingStatus::Terminal,
            Utc::now(),
        )
        .mark_terminal("turn-stale", "failed", None, None, Utc::now());
        let wrote_stale = repo
            .upsert_if_current_runtime_session(&stale_terminal)
            .await
            .expect("conditional stale upsert");
        assert!(!wrote_stale);
        let current = repo
            .get_current(run.id, agent.id)
            .await
            .expect("get after stale write")
            .expect("binding still exists");
        assert_eq!(current.runtime_session_id, replacement_session_id);
        assert_eq!(current.status, DeliveryBindingStatus::Ready);

        let replacement_running = AgentRunDeliveryBinding::from_anchor(
            &replacement_anchor,
            DeliveryBindingStatus::Running,
            Utc::now(),
        )
        .mark_running("turn-current", Utc::now());
        let wrote_current = repo
            .upsert_if_current_runtime_session(&replacement_running)
            .await
            .expect("conditional current upsert");
        assert!(wrote_current);
        let current = repo
            .get_current(run.id, agent.id)
            .await
            .expect("get after current write")
            .expect("binding still exists");
        assert_eq!(current.runtime_session_id, replacement_session_id);
        assert_eq!(current.status, DeliveryBindingStatus::Running);
        assert_eq!(current.active_turn_id.as_deref(), Some("turn-current"));

        repo.delete_by_session(&replacement_session_id)
            .await
            .expect("delete current binding");
        assert!(
            repo.get_current(run.id, agent.id)
                .await
                .expect("get after delete")
                .is_none()
        );
    }
}
