use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use agentdash_domain::DomainError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectRefView {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleRunRefView {
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRefView {
    pub run_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionRefView {
    pub runtime_session_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleSubjectAssociationView {
    pub id: String,
    pub anchor_run_id: String,
    pub anchor_agent_id: Option<String>,
    pub subject_ref: SubjectRefView,
    pub role: String,
    pub metadata: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleRunStatusView {
    Draft,
    Ready,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleRunTopologyView {
    Plain,
    WorkflowGraph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleExecutionEventKindView {
    ActivityActivated,
    ActivityCompleted,
    ConstraintBlocked,
    CompletionEvaluated,
    ArtifactAppended,
    ContextInjected,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutorRunRefView {
    RuntimeSession { session_id: String },
    FunctionRun { run_id: String },
    HumanDecision { decision_id: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleExecutionEntryView {
    pub timestamp: DateTime<Utc>,
    pub activity_key: String,
    pub event_kind: LifecycleExecutionEventKindView,
    pub summary: String,
    pub detail: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeNodeView {
    pub node_id: String,
    pub node_path: String,
    pub kind: String,
    pub status: String,
    pub attempt: u32,
    pub executor_run_ref: Option<ExecutorRunRefView>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub children: Vec<RuntimeNodeView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRuntimeNodeRefView {
    pub run_id: String,
    pub orchestration_id: String,
    pub node_path: String,
    pub attempt: u32,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestrationInstanceView {
    pub orchestration_id: String,
    pub role: String,
    pub status: String,
    pub plan_digest: String,
    pub source_ref: Value,
    pub ready_node_ids: Vec<String>,
    pub nodes: Vec<RuntimeNodeView>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunView {
    pub agent_ref: AgentRunRefView,
    pub project_id: String,
    pub source: String,
    pub project_agent_id: Option<String>,
    pub status: String,
    pub last_delivery_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleRunView {
    pub run_ref: LifecycleRunRefView,
    pub project_id: String,
    pub topology: LifecycleRunTopologyView,
    pub status: LifecycleRunStatusView,
    pub orchestrations: Vec<OrchestrationInstanceView>,
    pub active_runtime_node_refs: Vec<ActiveRuntimeNodeRefView>,
    pub agents: Vec<AgentRunView>,
    pub subject_associations: Vec<LifecycleSubjectAssociationView>,
    pub runtime_trace_refs: Vec<RuntimeSessionRefView>,
    pub execution_log: Vec<LifecycleExecutionEntryView>,
    pub created_at: String,
    pub updated_at: String,
    pub last_activity_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubjectExecutionView {
    pub subject_ref: SubjectRefView,
    pub associations: Vec<LifecycleSubjectAssociationView>,
    pub runs: Vec<LifecycleRunView>,
    pub current_agent: Option<AgentRunView>,
    pub runtime_attempts: Vec<SubjectRuntimeAttemptView>,
    pub latest_runtime_node: Option<RuntimeNodeView>,
    pub artifacts: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubjectRuntimeAttemptView {
    pub run_ref: LifecycleRunRefView,
    pub agent_ref: AgentRunRefView,
    pub runtime_session_ref: RuntimeSessionRefView,
    pub launch_frame_id: String,
    pub current_frame_id: Option<String>,
    pub orchestration_id: String,
    pub node_path: String,
    pub attempt: u32,
    pub status: String,
    pub observed_at: String,
    pub runtime_node: RuntimeNodeView,
    pub artifacts: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectActiveAgentsView {
    pub project_id: String,
    pub runs: Vec<LifecycleRunView>,
    pub agents: Vec<AgentRunView>,
}

#[async_trait]
pub trait LifecycleReadModelQueryPort: Send + Sync {
    async fn lifecycle_run_view(&self, run_id: Uuid) -> Result<LifecycleRunView, DomainError>;
}
