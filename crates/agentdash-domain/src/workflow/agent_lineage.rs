use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent spawn/delegation/companion relation (控制树)。
///
/// UI 控制树使用 AgentLineage；RuntimeSessionLineage 只保留 trace/debug 语义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLineage {
    pub id: Uuid,
    pub run_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_agent_id: Option<Uuid>,
    pub child_agent_id: Uuid,
    pub relation_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_frame_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl AgentLineage {
    pub fn new(
        run_id: Uuid,
        parent_agent_id: Option<Uuid>,
        child_agent_id: Uuid,
        relation_kind: impl Into<String>,
        source_frame_id: Option<Uuid>,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            run_id,
            parent_agent_id,
            child_agent_id,
            relation_kind: relation_kind.into(),
            source_frame_id,
            metadata_json: metadata,
            created_at: Utc::now(),
        }
    }
}
