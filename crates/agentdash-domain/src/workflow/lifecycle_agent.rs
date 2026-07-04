use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// Agent 的创建/启动来源 —— 在 agent **出生时**确定一次，此后不再变更。
///
/// 这是 agent 的内在身份属性，**不是** [`super::dispatch::ExecutionSource`]
/// （那是每次 dispatch / 状态更新的触发来源，会反复变化）。二者只在 agent
/// 出生那一刻做一次映射（见 dispatch_service 的 `agent_source_from_execution_source`）；
/// 任何状态/交互更新都不得回写本字段。
///
/// 变体集合 = 生产中真实存在的出生路径全集（已穷举：dispatch 出生 + orchestration
/// activity 出生）。不收录任何仅出现在测试夹具里的伪来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSource {
    /// 由用户 / API / Project Agent 触发而出生（ExecutionSource::User|Api|ProjectAgent）。
    ProjectAgent,
    /// 由 routine 触发而出生。
    Routine,
    /// 由 parent agent spawn 而出生（ExecutionSource::ParentAgent）。
    Subagent,
    /// orchestration workflow activity 节点 agent。
    WorkflowAgent,
    /// 通用来源：未识别或不再具备具体分类的来源。唯一的非具体来源变体。
    #[default]
    Unknown,
}

impl AgentSource {
    /// 持久化 / 契约用的 snake_case slug。
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentSource::ProjectAgent => "project_agent",
            AgentSource::Routine => "routine",
            AgentSource::Subagent => "subagent",
            AgentSource::WorkflowAgent => "workflow_agent",
            AgentSource::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for AgentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AgentSource {
    type Err = std::convert::Infallible;

    /// 只解析当前 canonical slug；未识别值落 [`AgentSource::Unknown`]。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let source = match s {
            "project_agent" => AgentSource::ProjectAgent,
            "routine" => AgentSource::Routine,
            "subagent" => AgentSource::Subagent,
            "workflow_agent" => AgentSource::WorkflowAgent,
            _ => AgentSource::Unknown,
        };
        Ok(source)
    }
}

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
/// `bootstrap_status` 取代原 SessionMeta.bootstrap_state，表达首轮初始化是否完成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleAgent {
    pub id: Uuid,
    pub run_id: Uuid,
    pub project_id: Uuid,
    #[serde(default = "default_created_by_user_id")]
    pub created_by_user_id: String,
    /// Agent 创建/启动来源（取代原 `agent_kind` 自由字符串）。
    pub source: AgentSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_agent_id: Option<Uuid>,
    pub status: String,
    /// 首轮 owner context bootstrap 状态。
    /// "pending" = 等待首次 bootstrap；"bootstrapped" = 已完成；"not_applicable" = 不需要。
    #[serde(default = "default_bootstrap_status")]
    pub bootstrap_status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_bootstrap_status() -> String {
    bootstrap_status::NOT_APPLICABLE.to_string()
}

impl LifecycleAgent {
    pub const SYSTEM_CREATED_BY_USER_ID: &'static str = "system";

    pub fn new_root(run_id: Uuid, project_id: Uuid, source: AgentSource) -> Self {
        Self::new_root_for_user(run_id, project_id, source, Self::SYSTEM_CREATED_BY_USER_ID)
    }

    pub fn new_root_for_user(
        run_id: Uuid,
        project_id: Uuid,
        source: AgentSource,
        created_by_user_id: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            run_id,
            project_id,
            created_by_user_id: normalize_created_by_user_id(created_by_user_id),
            source,
            project_agent_id: None,
            status: "active".to_string(),
            bootstrap_status: bootstrap_status::PENDING.to_string(),
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

    pub fn mark_bootstrapped(&mut self) {
        self.bootstrap_status = bootstrap_status::BOOTSTRAPPED.to_string();
        self.updated_at = Utc::now();
    }

    pub fn needs_bootstrap(&self) -> bool {
        self.bootstrap_status == bootstrap_status::PENDING
    }
}

fn default_created_by_user_id() -> String {
    LifecycleAgent::SYSTEM_CREATED_BY_USER_ID.to_string()
}

fn normalize_created_by_user_id(value: impl Into<String>) -> String {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        LifecycleAgent::SYSTEM_CREATED_BY_USER_ID.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod agent_source_tests {
    use super::*;

    #[test]
    fn slug_round_trips() {
        for source in [
            AgentSource::ProjectAgent,
            AgentSource::Routine,
            AgentSource::Subagent,
            AgentSource::WorkflowAgent,
            AgentSource::Unknown,
        ] {
            assert_eq!(AgentSource::from_str(source.as_str()).unwrap(), source);
        }
    }

    #[test]
    fn unknown_source_slugs_normalize_to_unknown() {
        assert_eq!(
            AgentSource::from_str("routine_agent").unwrap(),
            AgentSource::Unknown
        );
        assert_eq!(
            AgentSource::from_str("child_agent").unwrap(),
            AgentSource::Unknown
        );
        assert_eq!(
            AgentSource::from_str("migration_agent").unwrap(),
            AgentSource::Unknown
        );
        assert_eq!(
            AgentSource::from_str("task_agent").unwrap(),
            AgentSource::Unknown
        );
        assert_eq!(
            AgentSource::from_str("PI_AGENT").unwrap(),
            AgentSource::Unknown
        );
    }

    #[test]
    fn lifecycle_agent_owner_defaults_to_system_and_preserves_actor() {
        let run_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let default_agent = LifecycleAgent::new_root(run_id, project_id, AgentSource::ProjectAgent);
        assert_eq!(
            default_agent.created_by_user_id,
            LifecycleAgent::SYSTEM_CREATED_BY_USER_ID
        );

        let user_agent = LifecycleAgent::new_root_for_user(
            run_id,
            project_id,
            AgentSource::ProjectAgent,
            "  user-a  ",
        );
        assert_eq!(user_agent.created_by_user_id, "user-a");

        let blank_agent =
            LifecycleAgent::new_root_for_user(run_id, project_id, AgentSource::ProjectAgent, "   ");
        assert_eq!(
            blank_agent.created_by_user_id,
            LifecycleAgent::SYSTEM_CREATED_BY_USER_ID
        );
    }
}
