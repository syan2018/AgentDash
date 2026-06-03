use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent/Frame → graph activity attempt 的执行桥接。
///
/// 目标 key 必须包含 `graph_instance_id + activity_key + attempt`，
/// 这样 scheduler 可以定位到精确的 attempt，并通过 assignment 关联到 Agent/Frame。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAssignment {
    pub id: Uuid,
    pub run_id: Uuid,
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: i32,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub lease_status: String,
    pub assigned_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released_at: Option<DateTime<Utc>>,
}

impl AgentAssignment {
    pub fn new(
        run_id: Uuid,
        graph_instance_id: Uuid,
        activity_key: impl Into<String>,
        attempt: i32,
        agent_id: Uuid,
        frame_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            run_id,
            graph_instance_id,
            activity_key: activity_key.into(),
            attempt,
            agent_id,
            frame_id,
            lease_status: "active".to_string(),
            assigned_at: Utc::now(),
            released_at: None,
        }
    }

    pub fn release(&mut self) {
        self.lease_status = "released".to_string();
        self.released_at = Some(Utc::now());
    }
}
