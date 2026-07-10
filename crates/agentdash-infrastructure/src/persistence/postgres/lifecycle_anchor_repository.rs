use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    AgentFrame, AgentFrameRepository, AgentFrameSurfaceDocument, AgentLineage,
    AgentLineageRepository, ClaimGateResultParentContinuationRequest, ClaimGateResultWaiterRequest,
    CompleteGateResultParentContinuationRequest, GateResultDeliveryClaim, GateResultDeliveryMarker,
    GateResultDeliveryMarkerRepository, GateResultDeliveryStatus, GateWaitPolicyEnvelope,
    LifecycleAgent, LifecycleAgentRepository, LifecycleGate, LifecycleGateRepository,
    LifecycleSubjectAssociation, LifecycleSubjectAssociationRepository,
    RegisterGateResultWaiterRequest, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository, SubjectRef, WaitProducerRef,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::db_err;
use super::json_document::{from_optional_jsonb, to_jsonb, to_optional_jsonb};

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

#[derive(sqlx::FromRow)]
struct FrameRow {
    id: String,
    agent_id: String,
    revision: i32,
    surface: Option<Value>,
    effective_capability_json: Option<Value>,
    context_slice_json: Option<Value>,
    vfs_surface_json: Option<Value>,
    mcp_surface_json: Option<Value>,
    execution_profile_json: Option<Value>,
    visible_workspace_module_refs_json: Option<Value>,
    created_by_kind: String,
    created_by_id: Option<String>,
    created_at: DateTime<Utc>,
}

fn parse_opt_surface(
    s: Option<Value>,
    ctx: &str,
) -> Result<Option<AgentFrameSurfaceDocument>, DomainError> {
    from_optional_jsonb(s, ctx)
}

impl TryFrom<FrameRow> for AgentFrame {
    type Error = DomainError;
    fn try_from(row: FrameRow) -> Result<Self, Self::Error> {
        let mut frame = AgentFrame {
            id: parse_uuid(&row.id, "agent_frames.id")?,
            agent_id: parse_uuid(&row.agent_id, "agent_frames.agent_id")?,
            revision: row.revision,
            surface: parse_opt_surface(row.surface, "agent_frames.surface")?,
            effective_capability_json: row.effective_capability_json,
            context_slice_json: row.context_slice_json,
            vfs_surface_json: row.vfs_surface_json,
            mcp_surface_json: row.mcp_surface_json,
            execution_profile_json: row.execution_profile_json,
            visible_workspace_module_refs_json: row.visible_workspace_module_refs_json,
            created_by_kind: row.created_by_kind,
            created_by_id: row.created_by_id,
            created_at: row.created_at,
        };
        frame.apply_surface_projection();
        Ok(frame)
    }
}

#[async_trait::async_trait]
impl AgentFrameRepository for PostgresAgentFrameRepository {
    async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
        let surface = frame.surface_document();
        sqlx::query(
            r#"INSERT INTO agent_frames
                (id, agent_id, revision,
                 surface,
                 effective_capability_json, context_slice_json, vfs_surface_json, mcp_surface_json,
                 visible_workspace_module_refs_json,
                 execution_profile_json,
                 created_by_kind, created_by_id, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
        )
        .bind(frame.id.to_string())
        .bind(frame.agent_id.to_string())
        .bind(frame.revision)
        .bind(to_jsonb(&surface, "agent_frames.surface")?)
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
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, frame_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        sqlx::query_as::<_, FrameRow>(
            r#"SELECT id,agent_id,revision,
                      surface,
                      effective_capability_json,context_slice_json,vfs_surface_json,mcp_surface_json,
                      visible_workspace_module_refs_json,
                      execution_profile_json,
                      created_by_kind,created_by_id,created_at
               FROM agent_frames WHERE id=$1"#,
        )
        .bind(frame_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn get_current(&self, agent_id: Uuid) -> Result<Option<AgentFrame>, DomainError> {
        sqlx::query_as::<_, FrameRow>(
            r#"SELECT id,agent_id,revision,
                      surface,
                      effective_capability_json,context_slice_json,vfs_surface_json,mcp_surface_json,
                      visible_workspace_module_refs_json,
                      execution_profile_json,
                      created_by_kind,created_by_id,created_at
               FROM agent_frames WHERE agent_id=$1 ORDER BY revision DESC, created_at DESC LIMIT 1"#,
        )
        .bind(agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentFrame>, DomainError> {
        sqlx::query_as::<_, FrameRow>(
            r#"SELECT id,agent_id,revision,
                      surface,
                      effective_capability_json,context_slice_json,vfs_surface_json,mcp_surface_json,
                      visible_workspace_module_refs_json,
                      execution_profile_json,
                      created_by_kind,created_by_id,created_at
               FROM agent_frames WHERE agent_id=$1 ORDER BY revision ASC"#,
        )
        .bind(agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }
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
        })
    }
}

#[async_trait::async_trait]
impl LifecycleGateRepository for PostgresLifecycleGateRepository {
    async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO lifecycle_gates
                (id, run_id, agent_id, frame_id, gate_kind, correlation_id, status, payload_json, resolved_by, created_at, resolved_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
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
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LifecycleGate>, DomainError> {
        sqlx::query_as::<_, GateRow>(
            r#"SELECT id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,resolved_by,created_at,resolved_at
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
            r#"SELECT id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,resolved_by,created_at,resolved_at
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
            r#"SELECT id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,resolved_by,created_at,resolved_at
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
                    r#"SELECT id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,resolved_by,created_at,resolved_at
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
            r#"SELECT id,run_id,agent_id,frame_id,gate_kind,correlation_id,status,payload_json,resolved_by,created_at,resolved_at
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

#[derive(sqlx::FromRow)]
struct GateResultDeliveryMarkerRow {
    gate_id: String,
    result_attempt: i32,
    status: String,
    target_run_id: Option<String>,
    target_agent_id: Option<String>,
    target_waiter_ref: Option<String>,
    mailbox_message_id: Option<String>,
    command_receipt_id: Option<String>,
    claim_token: Option<String>,
    claim_expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

const GATE_RESULT_DELIVERY_MARKER_COLS: &str = "gate_id,result_attempt,status,target_run_id,target_agent_id,target_waiter_ref,mailbox_message_id,command_receipt_id,claim_token,claim_expires_at,created_at,updated_at";

impl TryFrom<GateResultDeliveryMarkerRow> for GateResultDeliveryMarker {
    type Error = DomainError;

    fn try_from(row: GateResultDeliveryMarkerRow) -> Result<Self, Self::Error> {
        Ok(Self {
            gate_id: parse_uuid(&row.gate_id, "gate_result_delivery_markers.gate_id")?,
            result_attempt: row.result_attempt,
            status: GateResultDeliveryStatus::parse(&row.status)?,
            target_run_id: opt_uuid(
                row.target_run_id.as_ref(),
                "gate_result_delivery_markers.target_run_id",
            )?,
            target_agent_id: opt_uuid(
                row.target_agent_id.as_ref(),
                "gate_result_delivery_markers.target_agent_id",
            )?,
            target_waiter_ref: row.target_waiter_ref,
            mailbox_message_id: opt_uuid(
                row.mailbox_message_id.as_ref(),
                "gate_result_delivery_markers.mailbox_message_id",
            )?,
            command_receipt_id: opt_uuid(
                row.command_receipt_id.as_ref(),
                "gate_result_delivery_markers.command_receipt_id",
            )?,
            claim_token: opt_uuid(
                row.claim_token.as_ref(),
                "gate_result_delivery_markers.claim_token",
            )?,
            claim_expires_at: row.claim_expires_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[async_trait::async_trait]
impl GateResultDeliveryMarkerRepository for PostgresLifecycleGateRepository {
    async fn register_waiter(
        &self,
        request: RegisterGateResultWaiterRequest,
    ) -> Result<GateResultDeliveryMarker, DomainError> {
        let now = Utc::now();
        sqlx::query_as::<_, GateResultDeliveryMarkerRow>(&format!(
            r#"INSERT INTO gate_result_delivery_markers
                (gate_id,result_attempt,status,target_run_id,target_agent_id,target_waiter_ref,claim_expires_at,created_at,updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8)
               ON CONFLICT (gate_id,result_attempt) DO UPDATE SET
                 target_run_id = CASE
                    WHEN gate_result_delivery_markers.status = 'pending' THEN EXCLUDED.target_run_id
                    ELSE gate_result_delivery_markers.target_run_id
                 END,
                 target_agent_id = CASE
                    WHEN gate_result_delivery_markers.status = 'pending' THEN EXCLUDED.target_agent_id
                    ELSE gate_result_delivery_markers.target_agent_id
                 END,
                 target_waiter_ref = CASE
                    WHEN gate_result_delivery_markers.status = 'pending' THEN EXCLUDED.target_waiter_ref
                    ELSE gate_result_delivery_markers.target_waiter_ref
                 END,
                 claim_expires_at = CASE
                    WHEN gate_result_delivery_markers.status = 'pending' THEN EXCLUDED.claim_expires_at
                    ELSE gate_result_delivery_markers.claim_expires_at
                 END,
                 updated_at = CASE
                    WHEN gate_result_delivery_markers.status = 'pending' THEN EXCLUDED.updated_at
                    ELSE gate_result_delivery_markers.updated_at
                 END
               RETURNING {GATE_RESULT_DELIVERY_MARKER_COLS}"#
        ))
        .bind(request.gate_id.to_string())
        .bind(request.result_attempt)
        .bind(GateResultDeliveryStatus::Pending.as_str())
        .bind(request.target_run_id.to_string())
        .bind(request.target_agent_id.to_string())
        .bind(request.waiter_ref)
        .bind(request.claim_expires_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?
        .try_into()
    }

    async fn claim_waiter_delivery(
        &self,
        request: ClaimGateResultWaiterRequest,
    ) -> Result<GateResultDeliveryClaim, DomainError> {
        let now = Utc::now();
        let updated = sqlx::query_as::<_, GateResultDeliveryMarkerRow>(&format!(
            r#"UPDATE gate_result_delivery_markers
               SET status=$4, claim_token=NULL, claim_expires_at=NULL, updated_at=$5
               WHERE gate_id=$1 AND result_attempt=$2
                 AND status='pending'
                 AND target_waiter_ref=$3
                 AND (claim_expires_at IS NULL OR claim_expires_at >= $5)
               RETURNING {GATE_RESULT_DELIVERY_MARKER_COLS}"#
        ))
        .bind(request.gate_id.to_string())
        .bind(request.result_attempt)
        .bind(&request.waiter_ref)
        .bind(GateResultDeliveryStatus::DeliveredToWaiter.as_str())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        if let Some(row) = updated {
            return row.try_into().map(GateResultDeliveryClaim::Claimed);
        }

        let inserted = sqlx::query_as::<_, GateResultDeliveryMarkerRow>(&format!(
            r#"INSERT INTO gate_result_delivery_markers
                (gate_id,result_attempt,status,target_run_id,target_agent_id,target_waiter_ref,created_at,updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$7)
               ON CONFLICT (gate_id,result_attempt) DO NOTHING
               RETURNING {GATE_RESULT_DELIVERY_MARKER_COLS}"#
        ))
        .bind(request.gate_id.to_string())
        .bind(request.result_attempt)
        .bind(GateResultDeliveryStatus::DeliveredToWaiter.as_str())
        .bind(request.target_run_id.to_string())
        .bind(request.target_agent_id.to_string())
        .bind(&request.waiter_ref)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        if let Some(row) = inserted {
            return row.try_into().map(GateResultDeliveryClaim::Claimed);
        }

        GateResultDeliveryMarkerRepository::get(self, request.gate_id, request.result_attempt)
            .await?
            .ok_or_else(|| DomainError::Database {
                operation: "claim_gate_result_waiter_delivery",
                message: format!(
                    "marker disappeared for gate_id={} result_attempt={}",
                    request.gate_id, request.result_attempt
                ),
            })
            .map(GateResultDeliveryClaim::Existing)
    }

    async fn claim_parent_continuation(
        &self,
        request: ClaimGateResultParentContinuationRequest,
    ) -> Result<GateResultDeliveryClaim, DomainError> {
        let now = Utc::now();
        let inserted = sqlx::query_as::<_, GateResultDeliveryMarkerRow>(&format!(
            r#"INSERT INTO gate_result_delivery_markers
                (gate_id,result_attempt,status,target_run_id,target_agent_id,claim_token,claim_expires_at,created_at,updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8)
               ON CONFLICT (gate_id,result_attempt) DO NOTHING
               RETURNING {GATE_RESULT_DELIVERY_MARKER_COLS}"#
        ))
        .bind(request.gate_id.to_string())
        .bind(request.result_attempt)
        .bind(GateResultDeliveryStatus::QueuedForParentContinuation.as_str())
        .bind(request.target_run_id.to_string())
        .bind(request.target_agent_id.to_string())
        .bind(request.claim_token.to_string())
        .bind(request.claim_expires_at)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        if let Some(row) = inserted {
            return row.try_into().map(GateResultDeliveryClaim::Claimed);
        }

        let updated = sqlx::query_as::<_, GateResultDeliveryMarkerRow>(&format!(
            r#"UPDATE gate_result_delivery_markers
               SET status=$3,
                   target_run_id=$4,
                   target_agent_id=$5,
                   claim_token=$6,
                   claim_expires_at=$7,
                   updated_at=$8
               WHERE gate_id=$1 AND result_attempt=$2
                 AND (
                    (status='pending' AND (claim_expires_at IS NULL OR claim_expires_at < $8))
                    OR (
                        status='queued_for_parent_continuation'
                        AND mailbox_message_id IS NULL
                        AND (claim_expires_at IS NULL OR claim_expires_at < $8)
                    )
                 )
               RETURNING {GATE_RESULT_DELIVERY_MARKER_COLS}"#
        ))
        .bind(request.gate_id.to_string())
        .bind(request.result_attempt)
        .bind(GateResultDeliveryStatus::QueuedForParentContinuation.as_str())
        .bind(request.target_run_id.to_string())
        .bind(request.target_agent_id.to_string())
        .bind(request.claim_token.to_string())
        .bind(request.claim_expires_at)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        if let Some(row) = updated {
            return row.try_into().map(GateResultDeliveryClaim::Claimed);
        }

        GateResultDeliveryMarkerRepository::get(self, request.gate_id, request.result_attempt)
            .await?
            .ok_or_else(|| DomainError::Database {
                operation: "claim_gate_result_parent_continuation",
                message: format!(
                    "marker disappeared for gate_id={} result_attempt={}",
                    request.gate_id, request.result_attempt
                ),
            })
            .map(GateResultDeliveryClaim::Existing)
    }

    async fn complete_parent_continuation(
        &self,
        request: CompleteGateResultParentContinuationRequest,
    ) -> Result<GateResultDeliveryMarker, DomainError> {
        let status = if request.dispatched_to_parent {
            GateResultDeliveryStatus::DispatchedToParent
        } else {
            GateResultDeliveryStatus::QueuedForParentContinuation
        };
        let row = sqlx::query_as::<_, GateResultDeliveryMarkerRow>(&format!(
            r#"UPDATE gate_result_delivery_markers
               SET status=$4,
                   mailbox_message_id=$5,
                   command_receipt_id=$6,
                   claim_token=NULL,
                   claim_expires_at=NULL,
                   updated_at=$7
               WHERE gate_id=$1 AND result_attempt=$2 AND claim_token=$3
               RETURNING {GATE_RESULT_DELIVERY_MARKER_COLS}"#
        ))
        .bind(request.gate_id.to_string())
        .bind(request.result_attempt)
        .bind(request.claim_token.to_string())
        .bind(status.as_str())
        .bind(request.mailbox_message_id.map(|id| id.to_string()))
        .bind(request.command_receipt_id.map(|id| id.to_string()))
        .bind(Utc::now())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        match row {
            Some(row) => row.try_into(),
            None => GateResultDeliveryMarkerRepository::get(
                self,
                request.gate_id,
                request.result_attempt,
            )
            .await?
            .ok_or_else(|| DomainError::Database {
                operation: "complete_gate_result_parent_continuation",
                message: format!(
                    "marker disappeared for gate_id={} result_attempt={}",
                    request.gate_id, request.result_attempt
                ),
            }),
        }
    }

    async fn get(
        &self,
        gate_id: Uuid,
        result_attempt: i32,
    ) -> Result<Option<GateResultDeliveryMarker>, DomainError> {
        sqlx::query_as::<_, GateResultDeliveryMarkerRow>(&format!(
            r#"SELECT {GATE_RESULT_DELIVERY_MARKER_COLS}
               FROM gate_result_delivery_markers
               WHERE gate_id=$1 AND result_attempt=$2"#
        ))
        .bind(gate_id.to_string())
        .bind(result_attempt)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
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

// ═══════════════════════════════════════════════════════════════════════════════
// RuntimeSessionExecutionAnchorRepository
// ═══════════════════════════════════════════════════════════════════════════════

pub struct PostgresRuntimeSessionExecutionAnchorRepository {
    pool: PgPool,
}

impl PostgresRuntimeSessionExecutionAnchorRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct AnchorRow {
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

impl TryFrom<AnchorRow> for RuntimeSessionExecutionAnchor {
    type Error = DomainError;
    fn try_from(row: AnchorRow) -> Result<Self, Self::Error> {
        Ok(RuntimeSessionExecutionAnchor {
            runtime_session_id: row.runtime_session_id,
            run_id: parse_uuid(&row.run_id, "rsea.run_id")?,
            launch_frame_id: parse_uuid(&row.launch_frame_id, "rsea.launch_frame_id")?,
            agent_id: parse_uuid(&row.agent_id, "rsea.agent_id")?,
            orchestration_id: opt_uuid(row.orchestration_id.as_ref(), "rsea.orchestration_id")?,
            node_path: row.node_path,
            node_attempt: row.node_attempt.map(|attempt| attempt as u32),
            created_by_kind: row.created_by_kind,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[async_trait::async_trait]
impl RuntimeSessionExecutionAnchorRepository for PostgresRuntimeSessionExecutionAnchorRepository {
    async fn create_once(&self, a: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
        let result = sqlx::query(
            r#"INSERT INTO runtime_session_execution_anchors
                (runtime_session_id, run_id, launch_frame_id, agent_id,
                 orchestration_id, node_path, node_attempt,
                 created_by_kind, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
               ON CONFLICT (runtime_session_id) DO NOTHING"#,
        )
        .bind(&a.runtime_session_id)
        .bind(a.run_id.to_string())
        .bind(a.launch_frame_id.to_string())
        .bind(a.agent_id.to_string())
        .bind(a.orchestration_id.map(|id| id.to_string()))
        .bind(&a.node_path)
        .bind(a.node_attempt.map(|attempt| attempt as i32))
        .bind(&a.created_by_kind)
        .bind(a.created_at)
        .bind(a.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        if result.rows_affected() > 0 {
            return Ok(());
        }

        let existing = self
            .find_by_session(&a.runtime_session_id)
            .await?
            .ok_or_else(|| DomainError::Database {
                operation: "create_runtime_session_execution_anchor",
                message: format!(
                    "runtime_session_id={} conflicted but existing anchor was not visible",
                    a.runtime_session_id
                ),
            })?;
        if existing.has_same_launch_coordinates_as(a) {
            return Ok(());
        }
        Err(existing.immutable_conflict(a))
    }

    async fn delete_by_session(&self, runtime_session_id: &str) -> Result<(), DomainError> {
        sqlx::query(
            r#"DELETE FROM runtime_session_execution_anchors
               WHERE runtime_session_id = $1"#,
        )
        .bind(runtime_session_id)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn find_by_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
        sqlx::query_as::<_, AnchorRow>(
            r#"SELECT runtime_session_id, run_id, launch_frame_id, agent_id,
                      orchestration_id, node_path, node_attempt,
                      created_by_kind, created_at, updated_at
               FROM runtime_session_execution_anchors
               WHERE runtime_session_id = $1"#,
        )
        .bind(runtime_session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
        sqlx::query_as::<_, AnchorRow>(
            r#"SELECT runtime_session_id, run_id, launch_frame_id, agent_id,
                      orchestration_id, node_path, node_attempt,
                      created_by_kind, created_at, updated_at
               FROM runtime_session_execution_anchors
               WHERE run_id = $1
               ORDER BY updated_at DESC"#,
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_by_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
        sqlx::query_as::<_, AnchorRow>(
            r#"SELECT runtime_session_id, run_id, launch_frame_id, agent_id,
                      orchestration_id, node_path, node_attempt,
                      created_by_kind, created_at, updated_at
               FROM runtime_session_execution_anchors
               WHERE agent_id = $1
               ORDER BY updated_at DESC"#,
        )
        .bind(agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_by_project_session_ids(
        &self,
        runtime_session_ids: &[String],
    ) -> Result<Vec<RuntimeSessionExecutionAnchor>, DomainError> {
        if runtime_session_ids.is_empty() {
            return Ok(Vec::new());
        }
        sqlx::query_as::<_, AnchorRow>(
            r#"SELECT runtime_session_id, run_id, launch_frame_id, agent_id,
                      orchestration_id, node_path, node_attempt,
                      created_by_kind, created_at, updated_at
               FROM runtime_session_execution_anchors
               WHERE runtime_session_id = ANY($1)
               ORDER BY updated_at DESC"#,
        )
        .bind(runtime_session_ids)
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
    use serde_json::json;

    fn frame_row_with_surface(surface: Option<serde_json::Value>) -> FrameRow {
        FrameRow {
            id: Uuid::new_v4().to_string(),
            agent_id: Uuid::new_v4().to_string(),
            revision: 3,
            surface,
            effective_capability_json: None,
            context_slice_json: None,
            vfs_surface_json: None,
            mcp_surface_json: None,
            execution_profile_json: None,
            visible_workspace_module_refs_json: None,
            created_by_kind: "test".to_string(),
            created_by_id: Some("tester".to_string()),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn frame_row_projects_surface_document_as_canonical_source() {
        let mut row = frame_row_with_surface(Some(json!({
            "capability_state": {"canonical": true},
            "vfs_surface": {"mounts": ["canonical"]},
            "visible_workspace_module_refs": ["canvas:canonical"]
        })));
        row.effective_capability_json = Some(json!({"stale": true}));
        row.vfs_surface_json = Some(json!({"mounts": ["stale"]}));
        row.visible_workspace_module_refs_json = Some(json!(["canvas:stale"]));

        let frame = AgentFrame::try_from(row).expect("frame row should map");

        assert_eq!(
            frame.effective_capability_json,
            Some(json!({"canonical": true}))
        );
        assert_eq!(
            frame.vfs_surface_json,
            Some(json!({"mounts": ["canonical"]}))
        );
        assert_eq!(
            frame.visible_workspace_module_refs_json,
            Some(json!(["canvas:canonical"]))
        );
    }

    #[test]
    fn frame_row_without_surface_derives_document_from_split_projection() {
        let mut row = frame_row_with_surface(None);
        row.effective_capability_json = Some(json!({"from_split": true}));
        row.context_slice_json = Some(json!({"slice": "launch"}));

        let frame = AgentFrame::try_from(row).expect("frame row should map");
        let surface = frame
            .surface
            .expect("surface projection should be materialized");

        assert_eq!(surface.capability_state, Some(json!({"from_split": true})));
        assert_eq!(surface.context_slice, Some(json!({"slice": "launch"})));
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

        let mailbox_message_id = Uuid::new_v4();
        let command_receipt_id = Uuid::new_v4();
        let completed = repo
            .complete_parent_continuation(CompleteGateResultParentContinuationRequest {
                gate_id: gate.id,
                result_attempt: 1,
                claim_token,
                mailbox_message_id: Some(mailbox_message_id),
                command_receipt_id: Some(command_receipt_id),
                dispatched_to_parent: false,
            })
            .await
            .expect("complete parent continuation");
        assert_eq!(
            completed.status,
            GateResultDeliveryStatus::QueuedForParentContinuation
        );
        assert_eq!(completed.mailbox_message_id, Some(mailbox_message_id));
        assert_eq!(completed.command_receipt_id, Some(command_receipt_id));

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
        assert_eq!(replay.marker().mailbox_message_id, Some(mailbox_message_id));
    }
}
