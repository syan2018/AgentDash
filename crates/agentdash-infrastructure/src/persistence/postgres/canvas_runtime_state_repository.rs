use sqlx::PgPool;
use uuid::Uuid;

use agentdash_domain::canvas::{
    CanvasInteractionSnapshot, CanvasRuntimeObservation, CanvasRuntimeStateRepository,
};
use agentdash_domain::common::error::DomainError;

use super::json_document::{from_jsonb, to_jsonb};
use super::{db_err, sql_err_for};

const CANVAS_STATE_TABLE: &str = "agent_run_canvas_state";
const RUNTIME_OBSERVATION_COLUMN: &str = "agent_run_canvas_state.runtime_observation";
const INTERACTION_SNAPSHOT_COLUMN: &str = "agent_run_canvas_state.interaction_snapshot";

pub struct PostgresCanvasRuntimeStateRepository {
    pool: PgPool,
}

impl PostgresCanvasRuntimeStateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        crate::migration::assert_postgres_tables_ready(&self.pool, &[CANVAS_STATE_TABLE]).await
    }
}

#[async_trait::async_trait]
impl CanvasRuntimeStateRepository for PostgresCanvasRuntimeStateRepository {
    async fn upsert_runtime_observation(
        &self,
        observation: CanvasRuntimeObservation,
    ) -> Result<CanvasRuntimeObservation, DomainError> {
        let document = to_jsonb(&observation, RUNTIME_OBSERVATION_COLUMN)?;
        let stored_document: serde_json::Value = sqlx::query_scalar(
            "INSERT INTO agent_run_canvas_state \
             (run_id,agent_id,canvas_id,canvas_mount_id,agent_run_canvas_ref,\
              delivery_trace_ref,current_agent_frame_id,frame_id,runtime_observation,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,NOW(),NOW()) \
             ON CONFLICT (run_id,agent_id,canvas_mount_id) DO UPDATE SET \
               canvas_id=EXCLUDED.canvas_id,\
               agent_run_canvas_ref=EXCLUDED.agent_run_canvas_ref,\
               delivery_trace_ref=EXCLUDED.delivery_trace_ref,\
               current_agent_frame_id=EXCLUDED.current_agent_frame_id,\
               frame_id=EXCLUDED.frame_id,\
               runtime_observation=EXCLUDED.runtime_observation,\
               updated_at=EXCLUDED.updated_at \
             RETURNING runtime_observation",
        )
        .bind(observation.run_id.to_string())
        .bind(observation.agent_id.to_string())
        .bind(observation.canvas_id.to_string())
        .bind(&observation.canvas_mount_id)
        .bind(&observation.agent_run_canvas_ref)
        .bind(&observation.delivery_trace_ref)
        .bind(observation.current_agent_frame_id.map(|id| id.to_string()))
        .bind(&observation.frame_id)
        .bind(document)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| sql_err_for(CANVAS_STATE_TABLE, error))?;
        from_jsonb(stored_document, RUNTIME_OBSERVATION_COLUMN)
    }

    async fn latest_runtime_observation(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        canvas_mount_id: &str,
    ) -> Result<Option<CanvasRuntimeObservation>, DomainError> {
        sqlx::query_scalar::<_, Option<serde_json::Value>>(
            "SELECT runtime_observation \
             FROM agent_run_canvas_state \
             WHERE run_id=$1 AND agent_id=$2 AND canvas_mount_id=$3",
        )
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(canvas_mount_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .flatten()
        .map(|document| from_jsonb(document, RUNTIME_OBSERVATION_COLUMN))
        .transpose()
    }

    async fn upsert_interaction_snapshot(
        &self,
        snapshot: CanvasInteractionSnapshot,
    ) -> Result<CanvasInteractionSnapshot, DomainError> {
        let document = to_jsonb(&snapshot, INTERACTION_SNAPSHOT_COLUMN)?;
        let stored_document: serde_json::Value = sqlx::query_scalar(
            "INSERT INTO agent_run_canvas_state \
             (run_id,agent_id,canvas_id,canvas_mount_id,agent_run_canvas_ref,\
              delivery_trace_ref,current_agent_frame_id,frame_id,interaction_snapshot,created_at,updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,NOW(),NOW()) \
             ON CONFLICT (run_id,agent_id,canvas_mount_id) DO UPDATE SET \
               canvas_id=EXCLUDED.canvas_id,\
               agent_run_canvas_ref=EXCLUDED.agent_run_canvas_ref,\
               delivery_trace_ref=EXCLUDED.delivery_trace_ref,\
               current_agent_frame_id=EXCLUDED.current_agent_frame_id,\
               frame_id=EXCLUDED.frame_id,\
               interaction_snapshot=EXCLUDED.interaction_snapshot,\
               updated_at=EXCLUDED.updated_at \
             RETURNING interaction_snapshot",
        )
        .bind(snapshot.run_id.to_string())
        .bind(snapshot.agent_id.to_string())
        .bind(snapshot.canvas_id.to_string())
        .bind(&snapshot.canvas_mount_id)
        .bind(&snapshot.agent_run_canvas_ref)
        .bind(&snapshot.delivery_trace_ref)
        .bind(snapshot.current_agent_frame_id.map(|id| id.to_string()))
        .bind(&snapshot.frame_id)
        .bind(document)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| sql_err_for(CANVAS_STATE_TABLE, error))?;
        from_jsonb(stored_document, INTERACTION_SNAPSHOT_COLUMN)
    }

    async fn latest_interaction_snapshot(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        canvas_mount_id: &str,
    ) -> Result<Option<CanvasInteractionSnapshot>, DomainError> {
        sqlx::query_scalar::<_, Option<serde_json::Value>>(
            "SELECT interaction_snapshot \
             FROM agent_run_canvas_state \
             WHERE run_id=$1 AND agent_id=$2 AND canvas_mount_id=$3",
        )
        .bind(run_id.to_string())
        .bind(agent_id.to_string())
        .bind(canvas_mount_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .flatten()
        .map(|document| from_jsonb(document, INTERACTION_SNAPSHOT_COLUMN))
        .transpose()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use agentdash_domain::canvas::{
        CanvasInteractionEvent, CanvasRuntimeDocumentState, CanvasRuntimeObservationStatus,
        CanvasRuntimeViewport,
    };

    use super::*;
    use crate::persistence::postgres::test_pg_pool;

    #[tokio::test]
    async fn observation_and_snapshot_roundtrip_without_overwriting_each_other() {
        let Some(pool) = test_pg_pool("canvas runtime state").await else {
            return;
        };
        let (run_id, agent_id, canvas_id) = seed_owners(&pool).await;
        let repo = PostgresCanvasRuntimeStateRepository::new(pool);
        repo.initialize()
            .await
            .expect("canvas state schema readiness");

        let snapshot = interaction_snapshot(run_id, agent_id, canvas_id);
        repo.upsert_interaction_snapshot(snapshot.clone())
            .await
            .expect("upsert interaction snapshot");
        assert_eq!(
            repo.latest_runtime_observation(run_id, agent_id, "mount-fixture")
                .await
                .expect("read absent observation"),
            None
        );

        let observation = runtime_observation(run_id, agent_id, canvas_id);
        repo.upsert_runtime_observation(observation.clone())
            .await
            .expect("upsert runtime observation");
        assert_eq!(
            repo.latest_runtime_observation(run_id, agent_id, "mount-fixture")
                .await
                .expect("read observation"),
            Some(observation.clone())
        );
        assert_eq!(
            repo.latest_interaction_snapshot(run_id, agent_id, "mount-fixture")
                .await
                .expect("read preserved snapshot"),
            Some(snapshot.clone())
        );

        let mut updated_snapshot = snapshot;
        updated_snapshot.state = json!({"selected": "node-2"});
        updated_snapshot.updated_at = Utc::now();
        repo.upsert_interaction_snapshot(updated_snapshot.clone())
            .await
            .expect("update interaction snapshot");
        assert_eq!(
            repo.latest_interaction_snapshot(run_id, agent_id, "mount-fixture")
                .await
                .expect("read updated snapshot"),
            Some(updated_snapshot)
        );
        assert_eq!(
            repo.latest_runtime_observation(run_id, agent_id, "mount-fixture")
                .await
                .expect("read preserved observation"),
            Some(observation)
        );
    }

    async fn seed_owners(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
        let project_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let canvas_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO projects(id,name,created_at,updated_at) VALUES ($1,$2,NOW(),NOW())",
        )
        .bind(project_id.to_string())
        .bind("canvas state fixture")
        .execute(pool)
        .await
        .expect("seed project");
        sqlx::query(
            "INSERT INTO lifecycle_runs(
                 id,project_id,topology,status,created_at,updated_at,last_activity_at
             ) VALUES ($1,$2,'single','active',NOW(),NOW(),NOW())",
        )
        .bind(run_id.to_string())
        .bind(project_id.to_string())
        .execute(pool)
        .await
        .expect("seed lifecycle run");
        sqlx::query(
            "INSERT INTO lifecycle_agents(
                 id,run_id,project_id,source,status,created_at,updated_at
             ) VALUES ($1,$2,$3,'unknown','idle',NOW(),NOW())",
        )
        .bind(agent_id.to_string())
        .bind(run_id.to_string())
        .bind(project_id.to_string())
        .execute(pool)
        .await
        .expect("seed lifecycle agent");
        sqlx::query(
            "INSERT INTO canvases(
                 id,project_id,owner_user_id,scope,mount_id,title,description,entry_file,
                 sandbox_config,created_at,updated_at
             ) VALUES ($1,$2,$3,'personal',$4,$5,'','index.html',$6,NOW(),NOW())",
        )
        .bind(canvas_id.to_string())
        .bind(project_id.to_string())
        .bind("canvas-state-user")
        .bind("mount-fixture")
        .bind("Canvas state fixture")
        .bind(json!({}))
        .execute(pool)
        .await
        .expect("seed canvas");
        (run_id, agent_id, canvas_id)
    }

    fn runtime_observation(
        run_id: Uuid,
        agent_id: Uuid,
        canvas_id: Uuid,
    ) -> CanvasRuntimeObservation {
        CanvasRuntimeObservation {
            observation_id: Uuid::new_v4(),
            run_id,
            agent_id,
            agent_run_canvas_ref: "canvas-ref".to_string(),
            canvas_id,
            canvas_mount_id: "mount-fixture".to_string(),
            delivery_trace_ref: Some("delivery-1".to_string()),
            current_agent_frame_id: None,
            frame_id: "frame-1".to_string(),
            generation: 3,
            captured_at: Utc::now(),
            status: CanvasRuntimeObservationStatus::Ready,
            message: Some("ready".to_string()),
            viewport: CanvasRuntimeViewport {
                width: 1280,
                height: 720,
                device_pixel_ratio: 2.0,
            },
            document: CanvasRuntimeDocumentState {
                root_empty: false,
                body_text_preview: "fixture".to_string(),
                element_count: 4,
                focused_element: Some("button".to_string()),
            },
            diagnostics: Vec::new(),
            screenshot_ref: None,
        }
    }

    fn interaction_snapshot(
        run_id: Uuid,
        agent_id: Uuid,
        canvas_id: Uuid,
    ) -> CanvasInteractionSnapshot {
        CanvasInteractionSnapshot {
            snapshot_id: Uuid::new_v4(),
            run_id,
            agent_id,
            agent_run_canvas_ref: "canvas-ref".to_string(),
            canvas_id,
            canvas_mount_id: "mount-fixture".to_string(),
            delivery_trace_ref: Some("delivery-1".to_string()),
            current_agent_frame_id: None,
            frame_id: "frame-1".to_string(),
            updated_at: Utc::now(),
            state: json!({"selected": "node-1"}),
            recent_events: vec![CanvasInteractionEvent {
                kind: "select".to_string(),
                payload: json!({"node": "node-1"}),
                occurred_at: Utc::now(),
            }],
        }
    }
}
