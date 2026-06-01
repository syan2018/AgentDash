use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Run-scoped Agent runtime identity.
///
/// Agent 只属于一个 LifecycleRun；可以有多个 frame revision 和 runtime session refs。
/// `current_frame_id` 指向当前生效 AgentFrame。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleAgent {
    pub id: Uuid,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub agent_kind: String,
    pub agent_role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_agent_id: Option<Uuid>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_frame_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LifecycleAgent {
    pub fn new_root(run_id: Uuid, project_id: Uuid, agent_kind: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            run_id,
            project_id,
            agent_kind: agent_kind.into(),
            agent_role: "primary".to_string(),
            project_agent_id: None,
            status: "active".to_string(),
            current_frame_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_project_agent(mut self, project_agent_id: Uuid) -> Self {
        self.project_agent_id = Some(project_agent_id);
        self
    }

    pub fn set_current_frame(&mut self, frame_id: Uuid) {
        self.current_frame_id = Some(frame_id);
        self.updated_at = Utc::now();
    }
}
