//! Product-owned AgentRun read-model values.
//!
//! These values contain Product coordinates only. Canonical conversation and Runtime lifecycle
//! are joined by the workspace query through `AgentRunProductProjectionQueryPort`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeThreadRefView {
    pub runtime_thread_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRefView {
    pub run_id: String,
    pub agent_id: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectRefView {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleSubjectAssociationView {
    pub id: String,
    pub anchor_run_id: String,
    pub anchor_agent_id: Option<String>,
    pub subject_ref: SubjectRefView,
    pub role: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}
