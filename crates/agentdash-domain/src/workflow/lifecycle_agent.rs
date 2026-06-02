use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent bootstrap 状态 — 控制首轮 owner context 注入是否完成。
pub mod bootstrap_status {
    pub const PENDING: &str = "pending";
    pub const BOOTSTRAPPED: &str = "bootstrapped";
    /// 无需 owner bootstrap 的 agent（如 companion child、reuse 场景）。
    pub const NOT_APPLICABLE: &str = "not_applicable";
}

/// Run-scoped Agent runtime identity.
///
/// Agent 只属于一个 LifecycleRun；可以有多个 frame revision 和 runtime session refs。
/// `current_frame_id` 指向当前生效 AgentFrame。
/// `bootstrap_status` 取代原 SessionMeta.bootstrap_state，表达首轮初始化是否完成。
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
    /// 首轮 owner context bootstrap 状态。
    /// "pending" = 等待首次 bootstrap；"bootstrapped" = 已完成；"not_applicable" = 不需要。
    #[serde(default = "default_bootstrap_status")]
    pub bootstrap_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_frame_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_bootstrap_status() -> String {
    bootstrap_status::NOT_APPLICABLE.to_string()
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
            bootstrap_status: bootstrap_status::PENDING.to_string(),
            current_frame_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_project_agent(mut self, project_agent_id: Uuid) -> Self {
        self.project_agent_id = Some(project_agent_id);
        self
    }

    pub fn with_bootstrap_status(mut self, status: &str) -> Self {
        self.bootstrap_status = status.to_string();
        self
    }

    pub fn set_current_frame(&mut self, frame_id: Uuid) {
        self.current_frame_id = Some(frame_id);
        self.updated_at = Utc::now();
    }

    pub fn mark_bootstrapped(&mut self) {
        self.bootstrap_status = bootstrap_status::BOOTSTRAPPED.to_string();
        self.updated_at = Utc::now();
    }

    pub fn needs_bootstrap(&self) -> bool {
        self.bootstrap_status == bootstrap_status::PENDING
    }
}
