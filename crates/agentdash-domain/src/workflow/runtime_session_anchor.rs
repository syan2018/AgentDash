use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Anchor-first runtime delivery selection.
///
/// Selection is resolved from `RuntimeSessionExecutionAnchorRepository`, so
/// delivery commands never consult AgentFrame persistence for runtime refs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDeliverySelectionPolicy {
    /// The runtime session currently handled by the runtime adapter.
    Specific { runtime_session_id: String },
    /// The earliest runtime session recorded for the agent.
    LaunchPrimary,
    /// The latest runtime session recorded for the agent.
    LatestAttached,
}

/// RuntimeSession -> 控制面实体的 launch evidence 锚点。
///
/// 在 dispatch / orchestrator launch 创建 RuntimeSession 时同步写入，
/// 让 runtime trace 能稳定反查到 lifecycle 控制面。
///
/// 这是 **launch evidence**——记录创建时刻的 frame/agent/assignment 关联，
/// 不被后续 frame revision 覆盖。查询最新 frame 仍需按 `agent_id` 取 current。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSessionExecutionAnchor {
    pub runtime_session_id: String,
    pub run_id: Uuid,
    pub launch_frame_id: Uuid,
    pub agent_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_instance_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt: Option<i32>,
    pub created_by_kind: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RuntimeSessionExecutionAnchor {
    /// 第一段写入：runtime_session + frame 创建后，assignment 尚未创建。
    pub fn new_dispatch(
        runtime_session_id: impl Into<String>,
        run_id: Uuid,
        launch_frame_id: Uuid,
        agent_id: Uuid,
        graph_instance_id: Option<Uuid>,
        activity_key: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            runtime_session_id: runtime_session_id.into(),
            run_id,
            launch_frame_id,
            agent_id,
            assignment_id: None,
            graph_instance_id,
            activity_key,
            attempt: None,
            created_by_kind: "dispatch".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 第二段补写：assignment 创建后回填。
    pub fn fill_assignment(&mut self, assignment_id: Uuid, attempt: i32) {
        self.assignment_id = Some(assignment_id);
        self.attempt = Some(attempt);
        self.updated_at = Utc::now();
    }
}
