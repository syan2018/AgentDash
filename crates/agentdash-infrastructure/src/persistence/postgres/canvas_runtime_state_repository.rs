use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::canvas::{
    CanvasInteractionSnapshot, CanvasRuntimeObservation, CanvasRuntimeStateRepository,
};
use agentdash_domain::common::error::DomainError;

use super::json_document::{from_jsonb, to_jsonb};
use super::{db_err, sql_err_for};

pub struct PostgresCanvasRuntimeStateRepository {
    pool: PgPool,
}

impl PostgresCanvasRuntimeStateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(
            &self.pool,
            &[
                "agent_run_canvas_runtime_observations",
                "agent_run_canvas_interaction_snapshots",
            ],
        )
        .await
    }
}

#[async_trait::async_trait]
impl CanvasRuntimeStateRepository for PostgresCanvasRuntimeStateRepository {
    async fn upsert_runtime_observation(
        &self,
        observation: CanvasRuntimeObservation,
    ) -> Result<CanvasRuntimeObservation, DomainError> {
        let payload = to_jsonb(
            &observation,
            "agent_run_canvas_runtime_observations.payload",
        )?;
        let now = Utc::now();
        sqlx::query_as::<_, CanvasRuntimeStateRow>(
            "INSERT INTO agent_run_canvas_runtime_observations \
             (id,run_id,agent_id,canvas_id,canvas_mount_id,agent_run_canvas_ref,\
              delivery_trace_ref,current_agent_frame_id,frame_id,generation,status,payload,captured_at,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$14) \
             ON CONFLICT (run_id,agent_id,canvas_mount_id) DO UPDATE SET \
               id=EXCLUDED.id,canvas_id=EXCLUDED.canvas_id,agent_run_canvas_ref=EXCLUDED.agent_run_canvas_ref,\
               delivery_trace_ref=EXCLUDED.delivery_trace_ref,current_agent_frame_id=EXCLUDED.current_agent_frame_id,\
               frame_id=EXCLUDED.frame_id,generation=EXCLUDED.generation,status=EXCLUDED.status,\
               payload=EXCLUDED.payload,captured_at=EXCLUDED.captured_at,updated_at=EXCLUDED.updated_at \
             RETURNING id, payload, created_at, updated_at",
        )
        .bind(observation.observation_id.to_string())
        .bind(observation.run_id.to_string())
        .bind(observation.agent_id.to_string())
        .bind(observation.canvas_id.to_string())
        .bind(&observation.canvas_mount_id)
        .bind(&observation.agent_run_canvas_ref)
        .bind(&observation.delivery_trace_ref)
        .bind(observation.current_agent_frame_id.map(|id| id.to_string()))
        .bind(&observation.frame_id)
        .bind(observation.generation)
        .bind(observation.status.as_str())
        .bind(payload)
        .bind(observation.captured_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_canvas_runtime_observations", error))?
        .into_runtime_observation()
    }

    async fn latest_runtime_observation(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        canvas_mount_id: &str,
    ) -> Result<Option<CanvasRuntimeObservation>, DomainError> {
        sqlx::query_as::<_, CanvasRuntimeStateRow>(
            "SELECT id, payload, created_at, updated_at \
             FROM agent_run_canvas_runtime_observations \
             WHERE run_id=$1 AND agent_id=$2 AND canvas_mount_id=$3",
        )
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(canvas_mount_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(CanvasRuntimeStateRow::into_runtime_observation)
        .transpose()
    }

    async fn upsert_interaction_snapshot(
        &self,
        snapshot: CanvasInteractionSnapshot,
    ) -> Result<CanvasInteractionSnapshot, DomainError> {
        let payload = to_jsonb(&snapshot, "agent_run_canvas_interaction_snapshots.payload")?;
        let now = Utc::now();
        sqlx::query_as::<_, CanvasRuntimeStateRow>(
            "INSERT INTO agent_run_canvas_interaction_snapshots \
             (id,run_id,agent_id,canvas_id,canvas_mount_id,agent_run_canvas_ref,\
              delivery_trace_ref,current_agent_frame_id,frame_id,payload,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$11) \
             ON CONFLICT (run_id,agent_id,canvas_mount_id) DO UPDATE SET \
               id=EXCLUDED.id,canvas_id=EXCLUDED.canvas_id,agent_run_canvas_ref=EXCLUDED.agent_run_canvas_ref,\
               delivery_trace_ref=EXCLUDED.delivery_trace_ref,current_agent_frame_id=EXCLUDED.current_agent_frame_id,\
               frame_id=EXCLUDED.frame_id,payload=EXCLUDED.payload,updated_at=EXCLUDED.updated_at \
             RETURNING id, payload, created_at, updated_at",
        )
        .bind(snapshot.snapshot_id.to_string())
        .bind(snapshot.run_id.to_string())
        .bind(snapshot.agent_id.to_string())
        .bind(snapshot.canvas_id.to_string())
        .bind(&snapshot.canvas_mount_id)
        .bind(&snapshot.agent_run_canvas_ref)
        .bind(&snapshot.delivery_trace_ref)
        .bind(snapshot.current_agent_frame_id.map(|id| id.to_string()))
        .bind(&snapshot.frame_id)
        .bind(payload)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| sql_err_for("agent_run_canvas_interaction_snapshots", error))?
        .into_interaction_snapshot()
    }

    async fn latest_interaction_snapshot(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        canvas_mount_id: &str,
    ) -> Result<Option<CanvasInteractionSnapshot>, DomainError> {
        sqlx::query_as::<_, CanvasRuntimeStateRow>(
            "SELECT id, payload, created_at, updated_at \
             FROM agent_run_canvas_interaction_snapshots \
             WHERE run_id=$1 AND agent_id=$2 AND canvas_mount_id=$3",
        )
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(canvas_mount_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(CanvasRuntimeStateRow::into_interaction_snapshot)
        .transpose()
    }
}

#[derive(sqlx::FromRow)]
struct CanvasRuntimeStateRow {
    #[allow(dead_code)]
    id: String,
    payload: serde_json::Value,
    #[allow(dead_code)]
    created_at: DateTime<Utc>,
    #[allow(dead_code)]
    updated_at: DateTime<Utc>,
}

impl CanvasRuntimeStateRow {
    fn into_runtime_observation(self) -> Result<CanvasRuntimeObservation, DomainError> {
        from_jsonb(
            self.payload,
            "agent_run_canvas_runtime_observations.payload",
        )
    }

    fn into_interaction_snapshot(self) -> Result<CanvasInteractionSnapshot, DomainError> {
        from_jsonb(
            self.payload,
            "agent_run_canvas_interaction_snapshots.payload",
        )
    }
}
