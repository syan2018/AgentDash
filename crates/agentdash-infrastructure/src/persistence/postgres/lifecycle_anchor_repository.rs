use agentdash_domain::common::error::DomainError;
use agentdash_domain::workflow::{
    ActivityLifecycleRunState, AgentAssignment, AgentAssignmentRepository, AgentFrame,
    AgentFrameRepository, AgentLineage, AgentLineageRepository, LifecycleAgent,
    LifecycleAgentRepository, LifecycleGate, LifecycleGateRepository, LifecycleSubjectAssociation,
    LifecycleSubjectAssociationRepository, RuntimeSessionExecutionAnchor,
    RuntimeSessionExecutionAnchorRepository, SubjectRef, WorkflowGraphInstance,
    WorkflowGraphInstanceRepository,
};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::db_err;

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
// WorkflowGraphInstanceRepository
// ═══════════════════════════════════════════════════════════════════════════════

pub struct PostgresWorkflowGraphInstanceRepository {
    pool: PgPool,
}

impl PostgresWorkflowGraphInstanceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct GraphInstanceRow {
    id: String,
    run_id: String,
    graph_id: String,
    role: String,
    status: String,
    activity_state_json: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<GraphInstanceRow> for WorkflowGraphInstance {
    type Error = DomainError;
    fn try_from(row: GraphInstanceRow) -> Result<Self, Self::Error> {
        Ok(WorkflowGraphInstance {
            id: parse_uuid(&row.id, "lifecycle_workflow_instances.id")?,
            run_id: parse_uuid(&row.run_id, "lifecycle_workflow_instances.run_id")?,
            graph_id: parse_uuid(&row.graph_id, "lifecycle_workflow_instances.graph_id")?,
            role: row.role,
            status: row.status,
            activity_state: row
                .activity_state_json
                .map(|s| serde_json::from_str::<ActivityLifecycleRunState>(&s))
                .transpose()
                .map_err(|e| DomainError::InvalidConfig(format!("activity_state_json: {e}")))?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[async_trait::async_trait]
impl WorkflowGraphInstanceRepository for PostgresWorkflowGraphInstanceRepository {
    async fn create(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError> {
        let activity_state = instance
            .activity_state
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| DomainError::InvalidConfig(format!("activity_state_json: {e}")))?;
        sqlx::query(
            r#"INSERT INTO lifecycle_workflow_instances
                (id, run_id, graph_id, role, status, activity_state_json, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
        )
        .bind(instance.id.to_string())
        .bind(instance.run_id.to_string())
        .bind(instance.graph_id.to_string())
        .bind(&instance.role)
        .bind(&instance.status)
        .bind(activity_state)
        .bind(instance.created_at)
        .bind(instance.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<WorkflowGraphInstance>, DomainError> {
        sqlx::query_as::<_, GraphInstanceRow>(
            "SELECT id,run_id,graph_id,role,status,activity_state_json,created_at,updated_at FROM lifecycle_workflow_instances WHERE id=$1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn get_by_run_and_id(
        &self,
        run_id: Uuid,
        id: Uuid,
    ) -> Result<Option<WorkflowGraphInstance>, DomainError> {
        sqlx::query_as::<_, GraphInstanceRow>(
            "SELECT id,run_id,graph_id,role,status,activity_state_json,created_at,updated_at FROM lifecycle_workflow_instances WHERE run_id=$1 AND id=$2",
        )
        .bind(run_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<WorkflowGraphInstance>, DomainError> {
        sqlx::query_as::<_, GraphInstanceRow>(
            "SELECT id,run_id,graph_id,role,status,activity_state_json,created_at,updated_at FROM lifecycle_workflow_instances WHERE run_id=$1 ORDER BY created_at",
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn update(&self, instance: &WorkflowGraphInstance) -> Result<(), DomainError> {
        let activity_state = instance
            .activity_state
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| DomainError::InvalidConfig(format!("activity_state_json: {e}")))?;
        sqlx::query(
            r#"UPDATE lifecycle_workflow_instances
               SET status=$1, activity_state_json=$2, updated_at=$3
               WHERE id=$4"#,
        )
        .bind(&instance.status)
        .bind(activity_state)
        .bind(instance.updated_at)
        .bind(instance.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
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
    agent_kind: String,
    agent_role: String,
    project_agent_id: Option<String>,
    status: String,
    bootstrap_status: String,
    current_frame_id: Option<String>,
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
            agent_kind: row.agent_kind,
            agent_role: row.agent_role,
            project_agent_id: opt_uuid(
                row.project_agent_id.as_ref(),
                "lifecycle_agents.project_agent_id",
            )?,
            status: row.status,
            bootstrap_status: row.bootstrap_status,
            current_frame_id: opt_uuid(
                row.current_frame_id.as_ref(),
                "lifecycle_agents.current_frame_id",
            )?,
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
                (id, run_id, project_id, agent_kind, agent_role, project_agent_id, status, bootstrap_status, current_frame_id, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
        )
        .bind(agent.id.to_string())
        .bind(agent.run_id.to_string())
        .bind(agent.project_id.to_string())
        .bind(&agent.agent_kind)
        .bind(&agent.agent_role)
        .bind(agent.project_agent_id.map(|id| id.to_string()))
        .bind(&agent.status)
        .bind(&agent.bootstrap_status)
        .bind(agent.current_frame_id.map(|id| id.to_string()))
        .bind(agent.created_at)
        .bind(agent.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LifecycleAgent>, DomainError> {
        sqlx::query_as::<_, AgentRow>(
            "SELECT id,run_id,project_id,agent_kind,agent_role,project_agent_id,status,bootstrap_status,current_frame_id,created_at,updated_at FROM lifecycle_agents WHERE id=$1",
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
            "SELECT id,run_id,project_id,agent_kind,agent_role,project_agent_id,status,bootstrap_status,current_frame_id,created_at,updated_at FROM lifecycle_agents WHERE run_id=$1 ORDER BY created_at",
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
               SET status=$1, bootstrap_status=$2, current_frame_id=$3, project_agent_id=$4, updated_at=$5
               WHERE id=$6"#,
        )
        .bind(&agent.status)
        .bind(&agent.bootstrap_status)
        .bind(agent.current_frame_id.map(|id| id.to_string()))
        .bind(agent.project_agent_id.map(|id| id.to_string()))
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
    procedure_id: Option<String>,
    graph_instance_id: Option<String>,
    activity_key: Option<String>,
    effective_capability_json: Option<String>,
    context_slice_json: Option<String>,
    vfs_surface_json: Option<String>,
    mcp_surface_json: Option<String>,
    runtime_session_refs_json: Option<String>,
    execution_profile_json: Option<String>,
    visible_canvas_mount_ids_json: Option<String>,
    created_by_kind: String,
    created_by_id: Option<String>,
    created_at: DateTime<Utc>,
}

fn parse_opt_json(s: Option<String>, ctx: &str) -> Result<Option<serde_json::Value>, DomainError> {
    match s {
        Some(val) => serde_json::from_str(&val)
            .map(Some)
            .map_err(|e| DomainError::InvalidConfig(format!("{ctx}: {e}"))),
        None => Ok(None),
    }
}

impl TryFrom<FrameRow> for AgentFrame {
    type Error = DomainError;
    fn try_from(row: FrameRow) -> Result<Self, Self::Error> {
        Ok(AgentFrame {
            id: parse_uuid(&row.id, "agent_frames.id")?,
            agent_id: parse_uuid(&row.agent_id, "agent_frames.agent_id")?,
            revision: row.revision,
            procedure_id: opt_uuid(row.procedure_id.as_ref(), "agent_frames.procedure_id")?,
            graph_instance_id: opt_uuid(
                row.graph_instance_id.as_ref(),
                "agent_frames.graph_instance_id",
            )?,
            activity_key: row.activity_key,
            effective_capability_json: parse_opt_json(
                row.effective_capability_json,
                "effective_capability_json",
            )?,
            context_slice_json: parse_opt_json(row.context_slice_json, "context_slice_json")?,
            vfs_surface_json: parse_opt_json(row.vfs_surface_json, "vfs_surface_json")?,
            mcp_surface_json: parse_opt_json(row.mcp_surface_json, "mcp_surface_json")?,
            runtime_session_refs_json: parse_opt_json(
                row.runtime_session_refs_json,
                "runtime_session_refs_json",
            )?,
            execution_profile_json: parse_opt_json(
                row.execution_profile_json,
                "execution_profile_json",
            )?,
            visible_canvas_mount_ids_json: parse_opt_json(
                row.visible_canvas_mount_ids_json,
                "visible_canvas_mount_ids_json",
            )?,
            created_by_kind: row.created_by_kind,
            created_by_id: row.created_by_id,
            created_at: row.created_at,
        })
    }
}

fn opt_json_str(v: &Option<serde_json::Value>) -> Result<Option<String>, DomainError> {
    match v {
        Some(val) => serde_json::to_string(val)
            .map(Some)
            .map_err(|e| DomainError::InvalidConfig(format!("json serialize: {e}"))),
        None => Ok(None),
    }
}

#[async_trait::async_trait]
impl AgentFrameRepository for PostgresAgentFrameRepository {
    async fn create(&self, frame: &AgentFrame) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO agent_frames
                (id, agent_id, revision, procedure_id, graph_instance_id, activity_key,
                 effective_capability_json, context_slice_json, vfs_surface_json, mcp_surface_json,
                 runtime_session_refs_json, execution_profile_json,
                 visible_canvas_mount_ids_json,
                 created_by_kind, created_by_id, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)"#,
        )
        .bind(frame.id.to_string())
        .bind(frame.agent_id.to_string())
        .bind(frame.revision)
        .bind(frame.procedure_id.map(|id| id.to_string()))
        .bind(frame.graph_instance_id.map(|id| id.to_string()))
        .bind(&frame.activity_key)
        .bind(opt_json_str(&frame.effective_capability_json)?)
        .bind(opt_json_str(&frame.context_slice_json)?)
        .bind(opt_json_str(&frame.vfs_surface_json)?)
        .bind(opt_json_str(&frame.mcp_surface_json)?)
        .bind(opt_json_str(&frame.runtime_session_refs_json)?)
        .bind(opt_json_str(&frame.execution_profile_json)?)
        .bind(opt_json_str(&frame.visible_canvas_mount_ids_json)?)
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
            r#"SELECT id,agent_id,revision,procedure_id,graph_instance_id,activity_key,
                      effective_capability_json,context_slice_json,vfs_surface_json,mcp_surface_json,
                      runtime_session_refs_json,execution_profile_json,
                      visible_canvas_mount_ids_json,
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
            r#"SELECT id,agent_id,revision,procedure_id,graph_instance_id,activity_key,
                      effective_capability_json,context_slice_json,vfs_surface_json,mcp_surface_json,
                      runtime_session_refs_json,execution_profile_json,
                      visible_canvas_mount_ids_json,
                      created_by_kind,created_by_id,created_at
               FROM agent_frames WHERE agent_id=$1 ORDER BY revision DESC LIMIT 1"#,
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
            r#"SELECT id,agent_id,revision,procedure_id,graph_instance_id,activity_key,
                      effective_capability_json,context_slice_json,vfs_surface_json,mcp_surface_json,
                      runtime_session_refs_json,execution_profile_json,
                      visible_canvas_mount_ids_json,
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

    async fn attach_runtime_session_ref(
        &self,
        frame_id: Uuid,
        runtime_session_id: &str,
    ) -> Result<(), DomainError> {
        let mut frame = self
            .get(frame_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_frame",
                id: frame_id.to_string(),
            })?;
        frame.attach_runtime_session_ref(runtime_session_id);
        sqlx::query("UPDATE agent_frames SET runtime_session_refs_json=$1 WHERE id=$2")
            .bind(opt_json_str(&frame.runtime_session_refs_json)?)
            .bind(frame_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn append_visible_canvas_mount(
        &self,
        frame_id: Uuid,
        mount_id: &str,
    ) -> Result<(), DomainError> {
        let mut frame = self
            .get(frame_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity: "agent_frame",
                id: frame_id.to_string(),
            })?;
        frame.append_visible_canvas_mount(mount_id);
        sqlx::query("UPDATE agent_frames SET visible_canvas_mount_ids_json=$1 WHERE id=$2")
            .bind(opt_json_str(&frame.visible_canvas_mount_ids_json)?)
            .bind(frame_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn find_by_runtime_session(
        &self,
        runtime_session_id: &str,
    ) -> Result<Option<AgentFrame>, DomainError> {
        // 优先通过 execution anchor 表查询（O(1) 精确匹配）
        let anchor_row = sqlx::query_as::<_, AnchorRow>(
            r#"SELECT runtime_session_id, run_id, launch_frame_id, agent_id,
                      assignment_id, graph_instance_id, activity_key, attempt,
                      created_by_kind, created_at, updated_at
               FROM runtime_session_execution_anchors
               WHERE runtime_session_id = $1"#,
        )
        .bind(runtime_session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        let Some(anchor) = anchor_row else {
            return Ok(None);
        };

        let agent_id = parse_uuid(&anchor.agent_id, "rsea.agent_id")?;
        sqlx::query_as::<_, FrameRow>(
            r#"SELECT id,agent_id,revision,procedure_id,graph_instance_id,activity_key,
                      effective_capability_json,context_slice_json,vfs_surface_json,mcp_surface_json,
                      runtime_session_refs_json,execution_profile_json,
                      visible_canvas_mount_ids_json,
                      created_by_kind,created_by_id,created_at
               FROM agent_frames WHERE agent_id=$1
               ORDER BY created_at DESC LIMIT 1"#,
        )
        .bind(agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// AgentAssignmentRepository
// ═══════════════════════════════════════════════════════════════════════════════

pub struct PostgresAgentAssignmentRepository {
    pool: PgPool,
}

impl PostgresAgentAssignmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct AssignmentRow {
    id: String,
    run_id: String,
    graph_instance_id: String,
    activity_key: String,
    attempt: i32,
    agent_id: String,
    frame_id: String,
    lease_status: String,
    assigned_at: DateTime<Utc>,
    released_at: Option<DateTime<Utc>>,
}

impl TryFrom<AssignmentRow> for AgentAssignment {
    type Error = DomainError;
    fn try_from(row: AssignmentRow) -> Result<Self, Self::Error> {
        Ok(AgentAssignment {
            id: parse_uuid(&row.id, "agent_assignments.id")?,
            run_id: parse_uuid(&row.run_id, "agent_assignments.run_id")?,
            graph_instance_id: parse_uuid(
                &row.graph_instance_id,
                "agent_assignments.graph_instance_id",
            )?,
            activity_key: row.activity_key,
            attempt: row.attempt,
            agent_id: parse_uuid(&row.agent_id, "agent_assignments.agent_id")?,
            frame_id: parse_uuid(&row.frame_id, "agent_assignments.frame_id")?,
            lease_status: row.lease_status,
            assigned_at: row.assigned_at,
            released_at: row.released_at,
        })
    }
}

#[async_trait::async_trait]
impl AgentAssignmentRepository for PostgresAgentAssignmentRepository {
    async fn create(&self, a: &AgentAssignment) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO agent_assignments
                (id, run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id, lease_status, assigned_at, released_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
        )
        .bind(a.id.to_string())
        .bind(a.run_id.to_string())
        .bind(a.graph_instance_id.to_string())
        .bind(&a.activity_key)
        .bind(a.attempt)
        .bind(a.agent_id.to_string())
        .bind(a.frame_id.to_string())
        .bind(&a.lease_status)
        .bind(a.assigned_at)
        .bind(a.released_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get(&self, assignment_id: Uuid) -> Result<Option<AgentAssignment>, DomainError> {
        sqlx::query_as::<_, AssignmentRow>(
            r#"SELECT id,run_id,graph_instance_id,activity_key,attempt,agent_id,frame_id,lease_status,assigned_at,released_at
               FROM agent_assignments WHERE id=$1"#,
        )
        .bind(assignment_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn find_for_attempt(
        &self,
        graph_instance_id: Uuid,
        activity_key: &str,
        attempt: i32,
    ) -> Result<Option<AgentAssignment>, DomainError> {
        sqlx::query_as::<_, AssignmentRow>(
            r#"SELECT id,run_id,graph_instance_id,activity_key,attempt,agent_id,frame_id,lease_status,assigned_at,released_at
               FROM agent_assignments WHERE graph_instance_id=$1 AND activity_key=$2 AND attempt=$3"#,
        )
        .bind(graph_instance_id.to_string())
        .bind(activity_key)
        .bind(attempt)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }

    async fn find_active_for_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<AgentAssignment>, DomainError> {
        sqlx::query_as::<_, AssignmentRow>(
            r#"SELECT id,run_id,graph_instance_id,activity_key,attempt,agent_id,frame_id,lease_status,assigned_at,released_at
               FROM agent_assignments WHERE agent_id=$1 AND lease_status='active' ORDER BY assigned_at DESC"#,
        )
        .bind(agent_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<AgentAssignment>, DomainError> {
        sqlx::query_as::<_, AssignmentRow>(
            r#"SELECT id,run_id,graph_instance_id,activity_key,attempt,agent_id,frame_id,lease_status,assigned_at,released_at
               FROM agent_assignments WHERE run_id=$1 ORDER BY assigned_at"#,
        )
        .bind(run_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    async fn update(&self, a: &AgentAssignment) -> Result<(), DomainError> {
        sqlx::query("UPDATE agent_assignments SET lease_status=$1, released_at=$2 WHERE id=$3")
            .bind(&a.lease_status)
            .bind(a.released_at)
            .bind(a.id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
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
    metadata_json: Option<String>,
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
            metadata_json: row
                .metadata_json
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e| DomainError::InvalidConfig(format!("metadata_json: {e}")))?,
            created_at: row.created_at,
        })
    }
}

#[async_trait::async_trait]
impl LifecycleSubjectAssociationRepository for PostgresLifecycleSubjectAssociationRepository {
    async fn create(&self, assoc: &LifecycleSubjectAssociation) -> Result<(), DomainError> {
        let metadata = assoc
            .metadata_json
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| DomainError::InvalidConfig(format!("metadata_json: {e}")))?;
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
        .bind(metadata)
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
    payload_json: Option<String>,
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
            payload_json: row
                .payload_json
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e| DomainError::InvalidConfig(format!("payload_json: {e}")))?,
            resolved_by: row.resolved_by,
            created_at: row.created_at,
            resolved_at: row.resolved_at,
        })
    }
}

#[async_trait::async_trait]
impl LifecycleGateRepository for PostgresLifecycleGateRepository {
    async fn create(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        let payload = gate
            .payload_json
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| DomainError::InvalidConfig(format!("payload_json: {e}")))?;
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
        .bind(payload)
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

    async fn update(&self, gate: &LifecycleGate) -> Result<(), DomainError> {
        let payload = gate
            .payload_json
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| DomainError::InvalidConfig(format!("payload_json: {e}")))?;
        sqlx::query(
            r#"UPDATE lifecycle_gates SET status=$1, payload_json=$2, resolved_by=$3, resolved_at=$4 WHERE id=$5"#,
        )
        .bind(&gate.status)
        .bind(payload)
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
    metadata_json: Option<String>,
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
            metadata_json: row
                .metadata_json
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e| DomainError::InvalidConfig(format!("metadata_json: {e}")))?,
            created_at: row.created_at,
        })
    }
}

#[async_trait::async_trait]
impl AgentLineageRepository for PostgresAgentLineageRepository {
    async fn create(&self, lineage: &AgentLineage) -> Result<(), DomainError> {
        let metadata = lineage
            .metadata_json
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| DomainError::InvalidConfig(format!("metadata_json: {e}")))?;
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
        .bind(metadata)
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
    assignment_id: Option<String>,
    graph_instance_id: Option<String>,
    activity_key: Option<String>,
    attempt: Option<i32>,
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
            assignment_id: opt_uuid(row.assignment_id.as_ref(), "rsea.assignment_id")?,
            graph_instance_id: opt_uuid(row.graph_instance_id.as_ref(), "rsea.graph_instance_id")?,
            activity_key: row.activity_key,
            attempt: row.attempt,
            created_by_kind: row.created_by_kind,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[async_trait::async_trait]
impl RuntimeSessionExecutionAnchorRepository for PostgresRuntimeSessionExecutionAnchorRepository {
    async fn upsert(&self, a: &RuntimeSessionExecutionAnchor) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO runtime_session_execution_anchors
                (runtime_session_id, run_id, launch_frame_id, agent_id,
                 assignment_id, graph_instance_id, activity_key, attempt,
                 created_by_kind, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
               ON CONFLICT (runtime_session_id) DO UPDATE SET
                 run_id = EXCLUDED.run_id,
                 launch_frame_id = EXCLUDED.launch_frame_id,
                 agent_id = EXCLUDED.agent_id,
                 assignment_id = COALESCE(EXCLUDED.assignment_id, runtime_session_execution_anchors.assignment_id),
                 graph_instance_id = COALESCE(EXCLUDED.graph_instance_id, runtime_session_execution_anchors.graph_instance_id),
                 activity_key = COALESCE(EXCLUDED.activity_key, runtime_session_execution_anchors.activity_key),
                 attempt = COALESCE(EXCLUDED.attempt, runtime_session_execution_anchors.attempt),
                 updated_at = EXCLUDED.updated_at"#,
        )
        .bind(&a.runtime_session_id)
        .bind(a.run_id.to_string())
        .bind(a.launch_frame_id.to_string())
        .bind(a.agent_id.to_string())
        .bind(a.assignment_id.map(|id| id.to_string()))
        .bind(a.graph_instance_id.map(|id| id.to_string()))
        .bind(&a.activity_key)
        .bind(a.attempt)
        .bind(&a.created_by_kind)
        .bind(a.created_at)
        .bind(a.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn update_assignment(
        &self,
        runtime_session_id: &str,
        assignment_id: Uuid,
        attempt: i32,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"UPDATE runtime_session_execution_anchors
               SET assignment_id = $2, attempt = $3, updated_at = now()
               WHERE runtime_session_id = $1"#,
        )
        .bind(runtime_session_id)
        .bind(assignment_id.to_string())
        .bind(attempt)
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
                      assignment_id, graph_instance_id, activity_key, attempt,
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
                      assignment_id, graph_instance_id, activity_key, attempt,
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
                      assignment_id, graph_instance_id, activity_key, attempt,
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
                      assignment_id, graph_instance_id, activity_key, attempt,
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

    async fn latest_for_agent(
        &self,
        agent_id: Uuid,
    ) -> Result<Option<RuntimeSessionExecutionAnchor>, DomainError> {
        sqlx::query_as::<_, AnchorRow>(
            r#"SELECT runtime_session_id, run_id, launch_frame_id, agent_id,
                      assignment_id, graph_instance_id, activity_key, attempt,
                      created_by_kind, created_at, updated_at
               FROM runtime_session_execution_anchors
               WHERE agent_id = $1
               ORDER BY updated_at DESC
               LIMIT 1"#,
        )
        .bind(agent_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?
        .map(TryInto::try_into)
        .transpose()
    }
}
