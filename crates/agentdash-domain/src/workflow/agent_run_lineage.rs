use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Cross-run AgentRun provenance relation.
///
/// `AgentLineage` remains the same-run agent control tree. This model links a
/// forked child AgentRun back to the parent AgentRun product fork boundary that
/// produced it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRunLineage {
    pub id: Uuid,
    pub parent_run_id: Uuid,
    pub parent_agent_id: Uuid,
    pub child_run_id: Uuid,
    pub child_agent_id: Uuid,
    pub relation_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_frame_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_frame_revision: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_frame_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_frame_revision: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_point_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_point_ref_json: Option<Value>,
    pub forked_by_user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<Value>,
    pub created_at: DateTime<Utc>,
}

impl AgentRunLineage {
    #[allow(clippy::too_many_arguments)]
    pub fn new_fork(
        parent_run_id: Uuid,
        parent_agent_id: Uuid,
        child_run_id: Uuid,
        child_agent_id: Uuid,
        fork_point_event_seq: Option<u64>,
        fork_point_ref_json: Option<Value>,
        forked_by_user_id: impl Into<String>,
        metadata_json: Option<Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_run_id,
            parent_agent_id,
            child_run_id,
            child_agent_id,
            relation_kind: "fork".to_string(),
            parent_frame_id: None,
            parent_frame_revision: None,
            child_frame_id: None,
            child_frame_revision: None,
            fork_point_event_seq,
            fork_point_ref_json,
            forked_by_user_id: normalize_user_id(forked_by_user_id),
            metadata_json,
            created_at: Utc::now(),
        }
    }

    pub fn with_frame_baseline(
        mut self,
        parent_frame_id: Uuid,
        parent_frame_revision: i32,
        child_frame_id: Uuid,
        child_frame_revision: i32,
    ) -> Self {
        self.parent_frame_id = Some(parent_frame_id);
        self.parent_frame_revision = Some(parent_frame_revision);
        self.child_frame_id = Some(child_frame_id);
        self.child_frame_revision = Some(child_frame_revision);
        self
    }
}

fn normalize_user_id(value: impl Into<String>) -> String {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "system".to_string()
    } else {
        trimmed.to_string()
    }
}
