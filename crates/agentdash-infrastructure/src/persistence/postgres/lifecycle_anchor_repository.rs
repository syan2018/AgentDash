use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentLineage, AgentLineageRepository,
    ClaimGateResultParentContinuationRequest, ClaimGateResultWaiterRequest,
    CompleteGateResultParentContinuationRequest, GateResultDeliveryClaim, GateResultDeliveryMarker,
    GateResultDeliveryMarkerRepository, GateResultDeliveryState, GateWaitPolicyEnvelope,
    LifecycleAgent, LifecycleAgentRepository, LifecycleGate, LifecycleGateRepository,
    LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository,
    RegisterGateResultWaiterRequest, SubjectRef, WaitProducerRef,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::db_err;
use super::json_document::{from_jsonb, to_jsonb, to_optional_jsonb};

fn parse_uuid(s: &str, ctx: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(s)
        .map_err(|e| DomainError::InvalidConfig(format!("{ctx}: invalid uuid `{s}`: {e}")))
}

fn opt_uuid(s: Option<&String>, ctx: &str) -> Result<Option<Uuid>, DomainError> {
    match s {
        Some(val) => Ok(Some(parse_uuid(val, ctx)?)),
        None => Ok(None),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// LifecycleAgentRepository
// ═══════════════════════════════════════════════════════════════════════════════

pub struct PostgresLifecycleAgentRepository {
    pool: PgPool,
}

impl PostgresLifecycleAgentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct AgentRow {
    id: String,
    run_id: String,
    project_id: String,
    created_by_user_id: String,
    source: String,
    project_agent_id: Option<String>,
    status: String,
    bootstrap_status: String,
    workspace_title: Option<String>,
    workspace_title_source: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<AgentRow> for LifecycleAgent {
    type Error = DomainError;
    fn try_from(row: AgentRow) -> Result<Self, Self::Error> {
        Ok(LifecycleAgent {
            id: parse_uuid(&row.id, "lifecycle_agents.id")?,
            run_id: parse_uuid(&row.run_id, "lifecycle_agents.run_id")?,
            project_id: parse_uuid(&row.project_id, "lifecycle_agents.project_id")?,
            created_by_user_id: row.created_by_user_id,
            source: row.source.parse().unwrap_or_default(),
            project_agent_id: opt_uuid(
                row.project_agent_id.as_ref(),
                "lifecycle_agents.project_agent_id",
            )?,
            status: row.status,
            bootstrap_status: row.bootstrap_status,
            workspace_title: row.workspace_title,
            workspace_title_source: row.workspace_title_source,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[async_trait::async_trait]
impl LifecycleAgentRepository for PostgresLifecycleAgentRepository {
    async fn create(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO lifecycle_agents
                (id, run_id, project_id, created_by_user_id, source, project_agent_id,
                 status, bootstrap_status, workspace_title, workspace_title_source,
                 created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
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
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
        sqlx::query_as::<_, AgentRow>(
            r#"SELECT id,run_id,project_id,source,project_agent_id,status,bootstrap_status,
                      created_by_user_id,workspace_title,workspace_title_source,
                      created_at,updated_at
               FROM lifecycle_agents WHERE id=$1"#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleAgent>, DomainError> {
        sqlx::query_as::<_, AgentRow>(
            r#"SELECT id,run_id,project_id,source,project_agent_id,status,bootstrap_status,
                      created_by_user_id,workspace_title,workspace_title_source,
                      created_at,updated_at
               FROM lifecycle_agents WHERE run_id=$1 ORDER BY created_at"#,
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn update(&self, agent: &LifecycleAgent) -> Result<(), DomainError> {
        sqlx::query(
            r#"UPDATE lifecycle_agents
               SET status=$1, bootstrap_status=$2, project_agent_id=$3,
                   created_by_user_id=$4,
                   workspace_title=$5, workspace_title_source=$6,
                   updated_at=$7
               WHERE id=$8"#,
        )
        .bind(&agent.status)
        .bind(&agent.bootstrap_status)
        .bind(agent.project_agent_id.map(|id| id.to_string()))
        .bind(&agent.created_by_user_id)
        .bind(&agent.workspace_title)
        .bind(&agent.workspace_title_source)
        .bind(agent.updated_at)
        .bind(agent.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn initialize_title_from_agent(
        &self,
        target: &agentdash_domain::agent_run_target::AgentRunTarget,
        title: &str,
    ) -> Result<bool, DomainError> {
        let title = title.trim();
        if title.is_empty() {
            return Ok(false);
        }
        let result = sqlx::query(
            r#"UPDATE lifecycle_agents
               SET workspace_title=$1, workspace_title_source='agent', updated_at=$2
               WHERE id=$3 AND run_id=$4
                 AND NULLIF(BTRIM(workspace_title), '') IS NULL"#,
        )
        .bind(title)
        .bind(Utc::now())
        .bind(target.agent_id.to_string())
        .bind(target.run_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        if result.rows_affected() > 0 {
            return Ok(true);
        }
        match self.get(target.agent_id).await? {
            Some(agent) if agent.run_id == target.run_id => Ok(false),
            _ => Err(DomainError::NotFound {
                entity: "lifecycle_agent",
                id: target.agent_id.to_string(),
            }),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// AgentFrameRepository
// ═══════════════════════════════════════════════════════════════════════════════

pub struct PostgresAgentFrameRepository {
    pool: PgPool,
}

impl PostgresAgentFrameRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAgentFrame {
    id: Uuid,
    agent_id: Uuid,
    revision: i32,
    surface: agentdash_domain::workflow::AgentFrameSurfaceDocument,
    created_by_kind: String,
    created_by_id: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<&AgentFrame> for StoredAgentFrame {
    fn from(frame: &AgentFrame) -> Self {
        Self {
            id: frame.id,
            agent_id: frame.agent_id,
            revision: frame.revision,
            surface: frame.surface_document(),
            created_by_kind: frame.created_by_kind.clone(),
            created_by_id: frame.created_by_id.clone(),
            created_at: frame.created_at,
        }
    }
}

impl From<StoredAgentFrame> for AgentFrame {
    fn from(stored: StoredAgentFrame) -> Self {
        let mut frame = Self {
            id: stored.id,
            agent_id: stored.agent_id,
            revision: stored.revision,
            surface: Some(stored.surface),
            effective_capability_json: None,
            context_slice_json: None,
            vfs_surface_json: None,
            mcp_surface_json: None,
            execution_profile_json: None,
            hook_plan: None,
            created_by_kind: stored.created_by_kind,
            created_by_id: stored.created_by_id,
            created_at: stored.created_at,
        };
        frame.apply_surface_projection();
        frame
    }
}

#[async_trait::async_trait]
impl AgentFrameRepository for PostgresAgentFrameRepository {
    async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
        let document = to_jsonb(&StoredAgentFrame::from(frame), "lifecycle_agents.frames")?;
        let result = sqlx::query(
            r#"UPDATE lifecycle_agents
               SET frames = frames || jsonb_build_array($2::JSONB),
                   updated_at = GREATEST(updated_at, $3)
               WHERE id=$1
                 AND NOT EXISTS (
                     SELECT 1
                     FROM jsonb_array_elements(frames) AS existing
                     WHERE existing ->> 'id' = $4
                        OR (existing ->> 'revision')::INTEGER = $5
                 )"#,
        )
        .bind(frame.agent_id.to_string())
        .bind(document)
        .bind(frame.created_at)
        .bind(frame.id.to_string())
        .bind(frame.revision)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        if result.rows_affected() != 1 {
            return Err(DomainError::Conflict {
                entity: "LifecycleAgent",
                constraint: "frame_history",
                message: format!(
                    "agent {} does not exist or frame id/revision is already present",
                    frame.agent_id
                ),
            });
        }
        Ok(())
    }

    async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        sqlx::query_scalar::<_, Value>(
            r#"SELECT frame
               FROM lifecycle_agents AS agent
               CROSS JOIN LATERAL jsonb_array_elements(agent.frames) AS frame
               WHERE frame ->> 'id' = $1"#,
        )
        .bind(frame_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(decode_stored_frame)
        .transpose()
    }

    async fn get_latest(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        sqlx::query_scalar::<_, Value>(
            r#"SELECT frame
               FROM lifecycle_agents AS agent
               CROSS JOIN LATERAL jsonb_array_elements(agent.frames) AS frame
               WHERE agent.id=$1
               ORDER BY (frame ->> 'revision')::INTEGER DESC,
                        (frame ->> 'created_at')::TIMESTAMPTZ DESC
               LIMIT 1"#,
        )
        .bind(agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(decode_stored_frame)
        .transpose()
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
        sqlx::query_scalar::<_, Value>(
            r#"SELECT frame
               FROM lifecycle_agents AS agent
               CROSS JOIN LATERAL jsonb_array_elements(agent.frames) AS frame
               WHERE agent.id=$1
               ORDER BY (frame ->> 'revision')::INTEGER ASC"#,
        )
        .bind(agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(decode_stored_frame)
        .collect()
    }
}

fn decode_stored_frame(value: Value) -> Result<AgentFrame, DomainError> {
    serde_json::from_value::<StoredAgentFrame>(value)
        .map(AgentFrame::from)
        .map_err(DomainError::Serialization)
}

// ═══════════════════════════════════════════════════════════════════════════════
// LifecycleSubjectAssociationRepository
// ═══════════════════════════════════════════════════════════════════════════════

pub struct PostgresLifecycleSubjectAssociationRepository {
    pool: PgPool,
}

impl PostgresLifecycleSubjectAssociationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct AssocRow {
    id: String,
    anchor_run_id: String,
    anchor_agent_id: Option<String>,
    subject_kind: String,
    subject_id: String,
    role: String,
    metadata_json: Option<Value>,
    created_at: DateTime<Utc>,
}

impl TryFrom<AssocRow> for LifecycleSubjectAssociation {
    type Error = DomainError;
    fn try_from(row: AssocRow) -> Result<Self, Self::Error> {
        Ok(LifecycleSubjectAssociation {
            id: parse_uuid(&row.id, "lifecycle_subject_associations.id")?,
            anchor_run_id: parse_uuid(
                &row.anchor_run_id,
                "lifecycle_subject_associations.anchor_run_id",
            )?,
            anchor_agent_id: opt_uuid(
                row.anchor_agent_id.as_ref(),
                "lifecycle_subject_associations.anchor_agent_id",
            )?,
            subject_kind: row.subject_kind,
            subject_id: parse_uuid(&row.subject_id, "lifecycle_subject_associations.subject_id")?,
            role: row.role,
            metadata_json: row.metadata_json,
            created_at: row.created_at,
        })
    }
}

#[async_trait::async_trait]
impl LifecycleSubjectAssociationRepository for PostgresLifecycleSubjectAssociationRepository {
    async fn create(&self, assoc: &LifecycleSubjectAssociation) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO lifecycle_subject_associations
                (id, anchor_run_id, anchor_agent_id, subject_kind, subject_id, role, metadata_json, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
        )
        .bind(assoc.id.to_string())
        .bind(assoc.anchor_run_id.to_string())
        .bind(assoc.anchor_agent_id.map(|id| id.to_string()))
        .bind(&assoc.subject_kind)
        .bind(assoc.subject_id.to_string())
        .bind(&assoc.role)
        .bind(to_optional_jsonb(
            assoc.metadata_json.as_ref(),
            "lifecycle_subject_associations.metadata_json",
        )?)
        .bind(assoc.created_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn list_by_subject(
        &self,
        subject: &SubjectRef,
    ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
        sqlx::query_as::<_, AssocRow>(
            r#"SELECT id,anchor_run_id,anchor_agent_id,subject_kind,subject_id,role,metadata_json,created_at
               FROM lifecycle_subject_associations WHERE subject_kind=$1 AND subject_id=$2 ORDER BY created_at DESC"#,
        )
        .bind(&subject.kind)
        .bind(subject.id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_by_anchor(
        &self,
        run_id: Uuid,
        agent_id: Option<Uuid>,
    ) -> Result<Vec<LifecycleSubjectAssociation>, DomainError> {
        let rows = match agent_id {
            Some(aid) => {
                sqlx::query_as::<_, AssocRow>(
                    r#"SELECT id,anchor_run_id,anchor_agent_id,subject_kind,subject_id,role,metadata_json,created_at
                       FROM lifecycle_subject_associations WHERE anchor_run_id=$1 AND anchor_agent_id=$2 ORDER BY created_at"#,
                )
                .bind(run_id.to_string())
                .bind(aid.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?
            }
            None => {
                sqlx::query_as::<_, AssocRow>(
                    r#"SELECT id,anchor_run_id,anchor_agent_id,subject_kind,subject_id,role,metadata_json,created_at
                       FROM lifecycle_subject_associations WHERE anchor_run_id=$1 AND anchor_agent_id IS NULL ORDER BY created_at"#,
                )
                .bind(run_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(db_err)?
            }
        };
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM lifecycle_subject_associations WHERE id=$1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// LifecycleGateRepository
// ═══════════════════════════════════════════════════════════════════════════════

pub struct PostgresLifecycleGateRepository {
    pool: PgPool,
}

impl PostgresLifecycleGateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct GateRow {
    id: String,
    run_id: String,
    agent_id: Option<String>,
    frame_id: Option<String>,
    gate_kind: String,
    correlation_id: String,
    status: String,
    payload_json: Option<Value>,
    resolved_by: Option<String>,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    delivery: Value,
}

impl TryFrom<GateRow> for LifecycleGate {
    type Error = DomainError;
    fn try_from(row: GateRow) -> Result<Self, Self::Error> {
        Ok(LifecycleGate {
            id: parse_uuid(&row.id, "lifecycle_gates.id")?,
            run_id: parse_uuid(&row.run_id, "lifecycle_gates.run_id")?,
            agent_id: opt_uuid(row.agent_id.as_ref(), "lifecycle_gates.agent_id")?,
            frame_id: opt_uuid(row.frame_id.as_ref(), "lifecycle_gates.frame_id")?,
            gate_kind: row.gate_kind,
            correlation_id: row.correlation_id,
            status: row.status,
            payload_json: row.payload_json,
            resolved_by: row.resolved_by,
            created_at: row.created_at,
            resolved_at: row.resolved_at,
            delivery: from_jsonb(row.delivery, "lifecycle_gates.delivery")?,
        })
    }
}

#[async_trait::async_trait]
impl LifecycleGateRepository for PostgresLifecycleGateRepository {
    async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO lifecycle_gates
                (id, run_id, agent_id, frame_id, gate_kind, correlation_id, status, payload_json, resolved_by, created_at, resolved_at, delivery)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        )
        .bind(gate.id.to_string())
        .bind(gate.run_id.to_string())
        .bind(gate.agent_id.map(|id| id.to_string()))
        .bind(gate.frame_id.map(|id| id.to_string()))
        .bind(&gate.gate_kind)
        .bind(&gate.correlation_id)
        .bind(&gate.status)
        .bind(to_optional_jsonb(
            gate.payload_json.as_ref(),
            "lifecycle_gates.payload_json",
        )?)
        .bind(&gate.resolved_by)
        .bind(gate.created_at)
        .bind(gate.resolved_at)
        .bind(to_jsonb(&gate.delivery, "lifecycle_gates.delivery")?)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
        sqlx::query_as::<_, GateRow>(
            r#"SELECT id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,resolved_by,created_at,resolved_at,delivery
               FROM lifecycle_gates WHERE id=$1"#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_open_for_agent(&self, agent_id: Uuid) -> Result<Vec<LifecycleGate>, DomainError> {
        sqlx::query_as::<_, GateRow>(
            r#"SELECT id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,resolved_by,created_at,resolved_at,delivery
               FROM lifecycle_gates WHERE agent_id=$1 AND status='open' ORDER BY created_at"#,
        )
        .bind(agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_open_gate_wait_policies(
        &self,
        limit: usize,
    ) -> Result<Vec<LifecycleGate>, DomainError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let paths = GateWaitPolicyEnvelope::json_paths();
        let query = format!(
            r#"SELECT id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,resolved_by,created_at,resolved_at,delivery
               FROM lifecycle_gates
               WHERE status='open'
                 AND payload_json IS NOT NULL
                 AND payload_json ->> '{schema_version}' = '1'
                 AND payload_json ? '{wait_policy}'
                 AND payload_json -> '{wait_policy}' ? '{source}'
                 AND payload_json -> '{wait_policy}' ? '{expected_result}'
                 AND payload_json -> '{wait_policy}' ? '{terminal_policy}'
                 AND payload_json -> '{wait_policy}' ? '{wake_target}'
               ORDER BY created_at
               LIMIT $1"#,
            schema_version = paths.schema_version,
            wait_policy = paths.wait_policy,
            source = paths.source,
            expected_result = paths.expected_result,
            terminal_policy = paths.terminal_policy,
            wake_target = paths.wake_target,
        );
        sqlx::query_as::<_, GateRow>(&query)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?
            .into_iter()
            .map(TryInto::try_into)
            .collect()
    }

    async fn list_by_wait_producer(
        &self,
        producer: &WaitProducerRef,
    ) -> Result<Vec<LifecycleGate>, DomainError> {
        match producer {
            WaitProducerRef::AgentRunDelivery {
                run_id,
                agent_id,
                frame_id,
            } => {
                let paths = GateWaitPolicyEnvelope::json_paths();
                let query = format!(
                    r#"SELECT id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,resolved_by,created_at,resolved_at,delivery
                       FROM lifecycle_gates
                       WHERE payload_json IS NOT NULL
                         AND payload_json ->> '{schema_version}' = '1'
                         AND payload_json -> '{wait_policy}' -> '{source}' ->> '{kind}' = $4
                         AND payload_json -> '{wait_policy}' -> '{source}' ->> '{run_id}' = $1
                         AND payload_json -> '{wait_policy}' -> '{source}' ->> '{agent_id}' = $2
                         AND (
                            $3::text IS NULL
                            OR payload_json -> '{wait_policy}' -> '{source}' ->> '{frame_id}' = $3
                         )
                       ORDER BY created_at"#,
                    schema_version = paths.schema_version,
                    wait_policy = paths.wait_policy,
                    source = paths.source,
                    kind = paths.kind,
                    run_id = paths.run_id,
                    agent_id = paths.agent_id,
                    frame_id = paths.frame_id,
                );
                let rows = sqlx::query_as::<_, GateRow>(&query)
                    .bind(run_id.to_string())
                    .bind(agent_id.to_string())
                    .bind(frame_id.map(|id| id.to_string()))
                    .bind(producer.kind())
                    .fetch_all(&self.pool)
                    .await
                    .map_err(db_err)?;
                rows.into_iter().map(TryInto::try_into).collect()
            }
        }
    }

    async fn find_by_agent_and_correlation(
        &self,
        agent_id: Uuid,
        correlation_id: &str,
    ) -> Result<Option<LifecycleGate>, DomainError> {
        sqlx::query_as::<_, GateRow>(
            r#"SELECT id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,resolved_by,created_at,resolved_at,delivery
               FROM lifecycle_gates WHERE agent_id=$1 AND correlation_id=$2 ORDER BY created_at DESC LIMIT 1"#,
        )
        .bind(agent_id.to_string())
        .bind(correlation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        sqlx::query(
            r#"UPDATE lifecycle_gates SET status=$1, payload_json=$2, resolved_by=$3, resolved_at=$4 WHERE id=$5"#,
        )
        .bind(&gate.status)
        .bind(to_optional_jsonb(
            gate.payload_json.as_ref(),
            "lifecycle_gates.payload_json",
        )?)
        .bind(&gate.resolved_by)
        .bind(gate.resolved_at)
        .bind(gate.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// GateResultDeliveryMarkerRepository
// ═══════════════════════════════════════════════════════════════════════════════

impl PostgresLifecycleGateRepository {
    async fn load_delivery_for_update(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        gate_id: Uuid,
    ) -> Result<GateResultDeliveryState, DomainError> {
        let delivery = sqlx::query_scalar::<_, Value>(
            r#"SELECT delivery FROM lifecycle_gates WHERE id=$1 FOR UPDATE"#,
        )
        .bind(gate_id.to_string())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(db_err)?
        .ok_or_else(|| DomainError::NotFound {
            entity: "lifecycle_gate",
            id: gate_id.to_string(),
        })?;
        from_jsonb(delivery, "lifecycle_gates.delivery")
    }

    async fn save_delivery(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        gate_id: Uuid,
        delivery: &GateResultDeliveryState,
        now: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"UPDATE lifecycle_gates
               SET delivery=$2, updated_at=$3
               WHERE id=$1"#,
        )
        .bind(gate_id.to_string())
        .bind(to_jsonb(delivery, "lifecycle_gates.delivery")?)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(db_err)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl GateResultDeliveryMarkerRepository for PostgresLifecycleGateRepository {
    async fn register_waiter(
        &self,
        request: RegisterGateResultWaiterRequest,
    ) -> Result<GateResultDeliveryMarker, DomainError> {
        let now = Utc::now();
        let gate_id = request.gate_id;
        let mut transaction = self.pool.begin().await.map_err(db_err)?;
        let mut delivery = self
            .load_delivery_for_update(&mut transaction, gate_id)
            .await?;
        let marker = delivery.register_waiter(request, now);
        self.save_delivery(&mut transaction, gate_id, &delivery, now)
            .await?;
        transaction.commit().await.map_err(db_err)?;
        Ok(marker)
    }

    async fn claim_waiter_delivery(
        &self,
        request: ClaimGateResultWaiterRequest,
    ) -> Result<GateResultDeliveryClaim, DomainError> {
        let now = Utc::now();
        let gate_id = request.gate_id;
        let mut transaction = self.pool.begin().await.map_err(db_err)?;
        let mut delivery = self
            .load_delivery_for_update(&mut transaction, gate_id)
            .await?;
        let claim = delivery.claim_waiter_delivery(request, now);
        self.save_delivery(&mut transaction, gate_id, &delivery, now)
            .await?;
        transaction.commit().await.map_err(db_err)?;
        Ok(claim)
    }

    async fn claim_parent_continuation(
        &self,
        request: ClaimGateResultParentContinuationRequest,
    ) -> Result<GateResultDeliveryClaim, DomainError> {
        let now = Utc::now();
        let gate_id = request.gate_id;
        let mut transaction = self.pool.begin().await.map_err(db_err)?;
        let mut delivery = self
            .load_delivery_for_update(&mut transaction, gate_id)
            .await?;
        let claim = delivery.claim_parent_continuation(request, now);
        self.save_delivery(&mut transaction, gate_id, &delivery, now)
            .await?;
        transaction.commit().await.map_err(db_err)?;
        Ok(claim)
    }

    async fn complete_parent_continuation(
        &self,
        request: CompleteGateResultParentContinuationRequest,
    ) -> Result<GateResultDeliveryMarker, DomainError> {
        let now = Utc::now();
        let gate_id = request.gate_id;
        let result_attempt = request.result_attempt;
        let mut transaction = self.pool.begin().await.map_err(db_err)?;
        let mut delivery = self
            .load_delivery_for_update(&mut transaction, gate_id)
            .await?;
        let marker = delivery
            .complete_parent_continuation(request, now)
            .ok_or_else(|| DomainError::Database {
                operation: "complete_gate_result_parent_continuation",
                message: format!(
                    "marker disappeared for gate_id={} result_attempt={}",
                    gate_id, result_attempt
                ),
            })?;
        self.save_delivery(&mut transaction, gate_id, &delivery, now)
            .await?;
        transaction.commit().await.map_err(db_err)?;
        Ok(marker)
    }

    async fn get(
        &self,
        gate_id: Uuid,
        result_attempt: i32,
    ) -> Result<Option<GateResultDeliveryMarker>, DomainError> {
        let delivery =
            sqlx::query_scalar::<_, Value>(r#"SELECT delivery FROM lifecycle_gates WHERE id=$1"#)
                .bind(gate_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(db_err)?;
        delivery
            .map(|delivery| {
                from_jsonb::<GateResultDeliveryState>(delivery, "lifecycle_gates.delivery")
                    .map(|delivery| delivery.marker(result_attempt).cloned())
            })
            .transpose()
            .map(Option::flatten)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// AgentLineageRepository
// ═══════════════════════════════════════════════════════════════════════════════

pub struct PostgresAgentLineageRepository {
    pool: PgPool,
}

impl PostgresAgentLineageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct LineageRow {
    id: String,
    run_id: String,
    parent_agent_id: Option<String>,
    child_agent_id: String,
    relation_kind: String,
    source_frame_id: Option<String>,
    metadata_json: Option<Value>,
    created_at: DateTime<Utc>,
}

impl TryFrom<LineageRow> for AgentLineage {
    type Error = DomainError;
    fn try_from(row: LineageRow) -> Result<Self, Self::Error> {
        Ok(AgentLineage {
            id: parse_uuid(&row.id, "agent_lineages.id")?,
            run_id: parse_uuid(&row.run_id, "agent_lineages.run_id")?,
            parent_agent_id: opt_uuid(
                row.parent_agent_id.as_ref(),
                "agent_lineages.parent_agent_id",
            )?,
            child_agent_id: parse_uuid(&row.child_agent_id, "agent_lineages.child_agent_id")?,
            relation_kind: row.relation_kind,
            source_frame_id: opt_uuid(
                row.source_frame_id.as_ref(),
                "agent_lineages.source_frame_id",
            )?,
            metadata_json: row.metadata_json,
            created_at: row.created_at,
        })
    }
}

#[async_trait::async_trait]
impl AgentLineageRepository for PostgresAgentLineageRepository {
    async fn create(&self, lineage: &AgentLineage) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO agent_lineages
                (id, run_id, parent_agent_id, child_agent_id, relation_kind, source_frame_id, metadata_json, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
        )
        .bind(lineage.id.to_string())
        .bind(lineage.run_id.to_string())
        .bind(lineage.parent_agent_id.map(|id| id.to_string()))
        .bind(lineage.child_agent_id.to_string())
        .bind(&lineage.relation_kind)
        .bind(lineage.source_frame_id.map(|id| id.to_string()))
        .bind(to_optional_jsonb(
            lineage.metadata_json.as_ref(),
            "agent_lineages.metadata_json",
        )?)
        .bind(lineage.created_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn list_children(&self, agent_id: Uuid) -> Result<Vec<AgentLineage>, DomainError> {
        sqlx::query_as::<_, LineageRow>(
            r#"SELECT id,run_id,parent_agent_id,child_agent_id,relation_kind,source_frame_id,metadata_json,created_at
               FROM agent_lineages WHERE parent_agent_id=$1 ORDER BY created_at"#,
        )
        .bind(agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn find_parent(&self, child_agent_id: Uuid) -> Result<Option<AgentLineage>, DomainError> {
        sqlx::query_as::<_, LineageRow>(
            r#"SELECT id,run_id,parent_agent_id,child_agent_id,relation_kind,source_frame_id,metadata_json,created_at
               FROM agent_lineages WHERE child_agent_id=$1 LIMIT 1"#,
        )
        .bind(child_agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentLineage>, DomainError> {
        sqlx::query_as::<_, LineageRow>(
            r#"SELECT id,run_id,parent_agent_id,child_agent_id,relation_kind,source_frame_id,metadata_json,created_at
               FROM agent_lineages WHERE run_id=$1 ORDER BY created_at"#,
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::postgres::test_pg_pool;
    use agentdash_domain::workflow::GateResultDeliveryStatus;
    use serde_json::json;

    fn stored_frame() -> StoredAgentFrame {
        StoredAgentFrame {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            revision: 3,
            surface: agentdash_domain::workflow::AgentFrameSurfaceDocument {
                capability_state: Some(json!({"canonical": true})),
                vfs_surface: Some(json!({"mounts": ["canonical"]})),
                ..Default::default()
            },
            created_by_kind: "test".to_string(),
            created_by_id: Some("tester".to_string()),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn stored_frame_projects_its_canonical_surface() {
        let frame = AgentFrame::from(stored_frame());

        assert_eq!(
            frame.effective_capability_json,
            Some(json!({"canonical": true}))
        );
        assert_eq!(
            frame.vfs_surface_json,
            Some(json!({"mounts": ["canonical"]}))
        );
    }

    async fn seed_marker_gate(repo: &PostgresLifecycleGateRepository) -> LifecycleGate {
        let run_id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            r#"INSERT INTO lifecycle_runs
               (id,project_id,created_by_user_id,topology,orchestrations,tasks,status,execution_log,created_at,updated_at,last_activity_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$9)"#,
        )
        .bind(run_id.to_string())
        .bind(Uuid::new_v4().to_string())
        .bind("fixture-user")
        .bind("plain")
        .bind(json!([]))
        .bind(json!([]))
        .bind("ready")
        .bind(json!([]))
        .bind(now)
        .execute(&repo.pool)
        .await
        .expect("seed lifecycle run");

        let gate = LifecycleGate::open(
            run_id,
            None,
            None,
            "companion_wait_follow_up",
            "dispatch-marker",
            Some(json!({"status": "completed"})),
        );
        repo.create(&gate).await.expect("seed gate");
        gate
    }

    #[tokio::test]
    async fn gate_result_delivery_marker_claims_waiter_or_parent_once() {
        let Some(pool) = test_pg_pool("gate_result_delivery_marker_claims").await else {
            return;
        };
        let repo = PostgresLifecycleGateRepository::new(pool);
        let gate = seed_marker_gate(&repo).await;
        let target_run_id = Uuid::new_v4();
        let target_agent_id = Uuid::new_v4();

        repo.register_waiter(RegisterGateResultWaiterRequest {
            gate_id: gate.id,
            result_attempt: 1,
            waiter_ref: "waiter-live".to_string(),
            target_run_id,
            target_agent_id,
            claim_expires_at: Utc::now() + chrono::Duration::seconds(60),
        })
        .await
        .expect("register waiter");

        let parent_attempt = repo
            .claim_parent_continuation(ClaimGateResultParentContinuationRequest {
                gate_id: gate.id,
                result_attempt: 1,
                target_run_id,
                target_agent_id,
                claim_token: Uuid::new_v4(),
                claim_expires_at: Utc::now() + chrono::Duration::seconds(60),
            })
            .await
            .expect("parent claim should replay pending waiter");
        assert!(!parent_attempt.claimed());
        assert_eq!(
            parent_attempt.marker().status,
            GateResultDeliveryStatus::Pending
        );

        let waiter_claim = repo
            .claim_waiter_delivery(ClaimGateResultWaiterRequest {
                gate_id: gate.id,
                result_attempt: 1,
                waiter_ref: "waiter-live".to_string(),
                target_run_id,
                target_agent_id,
            })
            .await
            .expect("waiter claim");
        assert!(waiter_claim.claimed());
        assert_eq!(
            waiter_claim.marker().status,
            GateResultDeliveryStatus::DeliveredToWaiter
        );

        let replay = repo
            .claim_parent_continuation(ClaimGateResultParentContinuationRequest {
                gate_id: gate.id,
                result_attempt: 1,
                target_run_id,
                target_agent_id,
                claim_token: Uuid::new_v4(),
                claim_expires_at: Utc::now() + chrono::Duration::seconds(60),
            })
            .await
            .expect("parent replay");
        assert!(!replay.claimed());
        assert_eq!(
            replay.marker().status,
            GateResultDeliveryStatus::DeliveredToWaiter
        );
    }

    #[tokio::test]
    async fn gate_result_delivery_marker_expired_waiter_can_queue_parent_once() {
        let Some(pool) = test_pg_pool("gate_result_delivery_marker_expired").await else {
            return;
        };
        let repo = PostgresLifecycleGateRepository::new(pool);
        let gate = seed_marker_gate(&repo).await;
        let target_run_id = Uuid::new_v4();
        let target_agent_id = Uuid::new_v4();

        repo.register_waiter(RegisterGateResultWaiterRequest {
            gate_id: gate.id,
            result_attempt: 1,
            waiter_ref: "waiter-expired".to_string(),
            target_run_id,
            target_agent_id,
            claim_expires_at: Utc::now() - chrono::Duration::seconds(1),
        })
        .await
        .expect("register expired waiter");

        let claim_token = Uuid::new_v4();
        let parent_claim = repo
            .claim_parent_continuation(ClaimGateResultParentContinuationRequest {
                gate_id: gate.id,
                result_attempt: 1,
                target_run_id,
                target_agent_id,
                claim_token,
                claim_expires_at: Utc::now() + chrono::Duration::seconds(60),
            })
            .await
            .expect("parent claim");
        assert!(parent_claim.claimed());
        assert_eq!(
            parent_claim.marker().status,
            GateResultDeliveryStatus::QueuedForParentContinuation
        );

        let input_handoff_id = Uuid::new_v4();
        let operation_id = "operation-gate-continuation".to_string();
        let completed = repo
            .complete_parent_continuation(CompleteGateResultParentContinuationRequest {
                gate_id: gate.id,
                result_attempt: 1,
                claim_token,
                input_handoff_id: Some(input_handoff_id),
                accepted_operation_id: Some(operation_id.clone()),
                dispatched_to_parent: false,
            })
            .await
            .expect("complete parent continuation");
        assert_eq!(
            completed.status,
            GateResultDeliveryStatus::QueuedForParentContinuation
        );
        assert_eq!(completed.input_handoff_id, Some(input_handoff_id));
        assert_eq!(completed.accepted_operation_id, Some(operation_id));

        let replay = repo
            .claim_parent_continuation(ClaimGateResultParentContinuationRequest {
                gate_id: gate.id,
                result_attempt: 1,
                target_run_id,
                target_agent_id,
                claim_token: Uuid::new_v4(),
                claim_expires_at: Utc::now() + chrono::Duration::seconds(60),
            })
            .await
            .expect("duplicate parent replay");
        assert!(!replay.claimed());
        assert_eq!(replay.marker().input_handoff_id, Some(input_handoff_id));
    }
}
