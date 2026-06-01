use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityAttemptStatus {
    Pending,
    Ready,
    Claiming,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl ActivityAttemptStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityAttemptState {
    pub activity_key: String,
    pub attempt: u32,
    pub status: ActivityAttemptStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_run: Option<ExecutorRunRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityPortValue {
    pub port_key: String,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityOutputArtifact {
    pub activity_key: String,
    pub attempt: u32,
    pub port_key: String,
    pub value: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityInputArtifact {
    pub activity_key: String,
    pub attempt: u32,
    pub port_key: String,
    pub source_activity_key: String,
    pub source_attempt: u32,
    pub source_port_key: String,
    pub value: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityLifecycleRunState {
    /// 当前 run state 所属的 graph instance，用于保证同一 LifecycleRun
    /// 内不同 WorkflowGraphInstance 的同名 activity key 互不干扰。
    #[serde(default = "Uuid::nil")]
    pub graph_instance_id: Uuid,
    pub status: ActivityRunStatus,
    pub attempts: Vec<ActivityAttemptState>,
    pub outputs: Vec<ActivityOutputArtifact>,
    pub inputs: Vec<ActivityInputArtifact>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityRunStatus {
    Ready,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutorRunRef {
    AgentSession { session_id: String },
    FunctionRun { run_id: String },
    HumanDecision { decision_id: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityExecutionClaimStatus {
    Claiming,
    Running,
    Succeeded,
    Failed,
    Abandoned,
}

impl ActivityExecutionClaimStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claiming => "claiming",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Claiming | Self::Running)
    }
}

impl std::str::FromStr for ActivityExecutionClaimStatus {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "claiming" => Ok(Self::Claiming),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "abandoned" => Ok(Self::Abandoned),
            _ => Err(format!("activity execution claim status 无效: {raw}")),
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRunStatus {
    Draft,
    Ready,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleExecutionEventKind {
    StepActivated,
    StepCompleted,
    ConstraintBlocked,
    CompletionEvaluated,
    ArtifactAppended,
    ContextInjected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleExecutionEntry {
    pub timestamp: DateTime<Utc>,
    pub activity_key: String,
    pub event_kind: LifecycleExecutionEventKind,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}
