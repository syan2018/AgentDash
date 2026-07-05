use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::error::DomainError;

/// RuntimeSession -> 控制面实体的 launch evidence 锚点。
///
/// 在 dispatch / orchestrator launch 创建 RuntimeSession 时同步写入，
/// 让 runtime trace 能稳定反查到 lifecycle 控制面。
///
/// 这是 **launch evidence**——记录创建时刻的 frame/agent 与可选 orchestration
/// node 关联，不被后续 frame revision 覆盖。查询最新 frame 仍需按 `agent_id`
/// 取 current。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSessionExecutionAnchor {
    pub runtime_session_id: String,
    pub run_id: Uuid,
    pub launch_frame_id: Uuid,
    pub agent_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestration_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_attempt: Option<u32>,
    pub created_by_kind: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RuntimeSessionExecutionAnchor {
    /// Plain AgentRun 写入：runtime_session + frame 创建后即可锚定控制面。
    pub fn new_dispatch(
        runtime_session_id: impl Into<String>,
        run_id: Uuid,
        launch_frame_id: Uuid,
        agent_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            runtime_session_id: runtime_session_id.into(),
            run_id,
            launch_frame_id,
            agent_id,
            orchestration_id: None,
            node_path: None,
            node_attempt: None,
            created_by_kind: "dispatch".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn new_orchestration_dispatch(
        runtime_session_id: impl Into<String>,
        run_id: Uuid,
        launch_frame_id: Uuid,
        agent_id: Uuid,
        orchestration_id: Uuid,
        node_path: impl Into<String>,
        node_attempt: u32,
    ) -> Self {
        let now = Utc::now();
        Self {
            runtime_session_id: runtime_session_id.into(),
            run_id,
            launch_frame_id,
            agent_id,
            orchestration_id: Some(orchestration_id),
            node_path: Some(node_path.into()),
            node_attempt: Some(node_attempt),
            created_by_kind: "dispatch".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn has_same_launch_coordinates_as(&self, other: &Self) -> bool {
        self.runtime_session_id == other.runtime_session_id
            && self.run_id == other.run_id
            && self.launch_frame_id == other.launch_frame_id
            && self.agent_id == other.agent_id
            && self.orchestration_id == other.orchestration_id
            && self.node_path == other.node_path
            && self.node_attempt == other.node_attempt
    }

    pub fn immutable_conflict(&self, requested: &Self) -> DomainError {
        DomainError::Conflict {
            entity: "runtime_session_execution_anchor",
            constraint: "runtime_session_id_immutable",
            message: format!(
                "runtime_session_id={} 已锚定到 run_id={}, agent_id={}, launch_frame_id={}, orchestration_id={:?}, node_path={:?}, node_attempt={:?}; requested run_id={}, agent_id={}, launch_frame_id={}, orchestration_id={:?}, node_path={:?}, node_attempt={:?}",
                self.runtime_session_id,
                self.run_id,
                self.agent_id,
                self.launch_frame_id,
                self.orchestration_id,
                self.node_path,
                self.node_attempt,
                requested.run_id,
                requested.agent_id,
                requested.launch_frame_id,
                requested.orchestration_id,
                requested.node_path,
                requested.node_attempt,
            ),
        }
    }
}
