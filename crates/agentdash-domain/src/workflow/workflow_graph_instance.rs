use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// graph 在某个 LifecycleRun 内的一次生效实例。
///
/// root graph 以 `(run_id, role="root")` 唯一；同一 run 可以有多个 graph instance
/// （task execution, companion review 等）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowGraphInstance {
    pub id: Uuid,
    pub run_id: Uuid,
    pub graph_id: Uuid,
    pub role: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_state_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowGraphInstance {
    pub fn new_root(run_id: Uuid, graph_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            run_id,
            graph_id,
            role: "root".to_string(),
            status: "active".to_string(),
            activity_state_json: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn new(run_id: Uuid, graph_id: Uuid, role: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            run_id,
            graph_id,
            role: role.into(),
            status: "active".to_string(),
            activity_state_json: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_root(&self) -> bool {
        self.role == "root"
    }
}
