use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::common::error::DomainError;

pub const MAILBOX_DELIVERY_RESULT_UNKNOWN: &str = "delivery_result_unknown";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailboxMessageOrigin {
    User,
    System,
    Hook,
    Companion,
    Workflow,
}

impl MailboxMessageOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
            Self::Hook => "hook",
            Self::Companion => "companion",
            Self::Workflow => "workflow",
        }
    }
}

impl TryFrom<&str> for MailboxMessageOrigin {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "user" => Ok(Self::User),
            "system" => Ok(Self::System),
            "hook" => Ok(Self::Hook),
            "companion" => Ok(Self::Companion),
            "workflow" => Ok(Self::Workflow),
            other => Err(DomainError::InvalidConfig(format!(
                "agent_run_mailbox_messages.origin 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MailboxSourceIdentity {
    pub namespace: String,
    pub kind: String,
    pub source_ref: Option<String>,
    pub correlation_ref: Option<String>,
    pub actor: String,
    pub route: Option<String>,
    pub display_label_key: String,
    pub metadata: Option<Value>,
}

impl MailboxSourceIdentity {
    pub fn new(
        namespace: impl Into<String>,
        kind: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let namespace = namespace.into();
        let kind = kind.into();
        Self {
            display_label_key: format!("mailbox.source.{namespace}.{kind}"),
            namespace,
            kind,
            source_ref: None,
            correlation_ref: None,
            actor: actor.into(),
            route: None,
            metadata: None,
        }
    }

    pub fn with_source_ref(mut self, source_ref: impl Into<String>) -> Self {
        self.source_ref = Some(source_ref.into());
        self
    }

    pub fn with_correlation_ref(mut self, correlation_ref: impl Into<String>) -> Self {
        self.correlation_ref = Some(correlation_ref.into());
        self
    }

    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }

    pub fn with_display_label_key(mut self, display_label_key: impl Into<String>) -> Self {
        self.display_label_key = display_label_key.into();
        self
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn dedup_fragment(&self) -> String {
        format!("{}:{}", self.namespace, self.kind)
    }

    pub fn composer() -> Self {
        Self::new("core", "composer", "user")
    }

    pub fn draft_start() -> Self {
        Self::new("core", "draft_start", "user")
    }

    pub fn hook_after_turn() -> Self {
        Self::new("core", "hook_after_turn", "system")
    }

    pub fn hook_before_stop() -> Self {
        Self::new("core", "hook_before_stop", "system")
    }

    pub fn hook_auto_resume() -> Self {
        Self::new("core", "hook_auto_resume", "system")
    }

    pub fn companion_parent_resume() -> Self {
        Self::new("companion", "parent_resume", "agent").with_route("parent")
    }

    pub fn workflow_orchestrator() -> Self {
        Self::new("workflow", "orchestrator", "system")
    }

    pub fn routine_trigger() -> Self {
        Self::new("routine", "trigger", "routine")
    }

    pub fn local_relay_prompt() -> Self {
        Self::new("core", "local_relay_prompt", "user")
    }

    pub fn canvas_action() -> Self {
        Self::new("core", "canvas_action", "user")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteeringStopEffect {
    None,
    ContinueOnStop,
}

impl SteeringStopEffect {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ContinueOnStop => "continue_on_stop",
        }
    }
}

impl TryFrom<&str> for SteeringStopEffect {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "none" => Ok(Self::None),
            "continue_on_stop" => Ok(Self::ContinueOnStop),
            other => Err(DomainError::InvalidConfig(format!(
                "agent_run_mailbox_messages.delivery.stop_effect 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailboxDelivery {
    LaunchOrContinueTurn,
    SteerActiveTurn { stop_effect: SteeringStopEffect },
    ResumeLaunchSource { launch_source: String },
}

impl MailboxDelivery {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::LaunchOrContinueTurn => "launch_or_continue_turn",
            Self::SteerActiveTurn { .. } => "steer_active_turn",
            Self::ResumeLaunchSource { .. } => "resume_launch_source",
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::LaunchOrContinueTurn => serde_json::json!({}),
            Self::SteerActiveTurn { stop_effect } => {
                serde_json::json!({ "stop_effect": stop_effect.as_str() })
            }
            Self::ResumeLaunchSource { launch_source } => {
                serde_json::json!({ "launch_source": launch_source })
            }
        }
    }

    pub fn from_parts(kind: &str, json: Value) -> Result<Self, DomainError> {
        match kind {
            "launch_or_continue_turn" => Ok(Self::LaunchOrContinueTurn),
            "steer_active_turn" => {
                let stop_effect = json
                    .get("stop_effect")
                    .and_then(Value::as_str)
                    .unwrap_or("none");
                Ok(Self::SteerActiveTurn {
                    stop_effect: SteeringStopEffect::try_from(stop_effect)?,
                })
            }
            "resume_launch_source" => {
                let Some(launch_source) = json.get("launch_source").and_then(Value::as_str) else {
                    return Err(DomainError::InvalidConfig(
                        "agent_run_mailbox_messages.delivery_json 缺少 launch_source".to_string(),
                    ));
                };
                Ok(Self::ResumeLaunchSource {
                    launch_source: launch_source.to_string(),
                })
            }
            other => Err(DomainError::InvalidConfig(format!(
                "agent_run_mailbox_messages.delivery 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumptionBarrier {
    ImmediateIfIdle,
    AgentLoopTurnBoundary,
    AgentRunTurnBoundary,
    ManualResume,
}

impl ConsumptionBarrier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ImmediateIfIdle => "immediate_if_idle",
            Self::AgentLoopTurnBoundary => "agent_loop_turn_boundary",
            Self::AgentRunTurnBoundary => "agent_run_turn_boundary",
            Self::ManualResume => "manual_resume",
        }
    }
}

impl TryFrom<&str> for ConsumptionBarrier {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "immediate_if_idle" => Ok(Self::ImmediateIfIdle),
            "agent_loop_turn_boundary" => Ok(Self::AgentLoopTurnBoundary),
            "agent_run_turn_boundary" => Ok(Self::AgentRunTurnBoundary),
            "manual_resume" => Ok(Self::ManualResume),
            other => Err(DomainError::InvalidConfig(format!(
                "agent_run_mailbox_messages.barrier 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailboxDrainMode {
    One,
    All,
}

impl MailboxDrainMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::One => "one",
            Self::All => "all",
        }
    }
}

impl TryFrom<&str> for MailboxDrainMode {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "one" => Ok(Self::One),
            "all" => Ok(Self::All),
            other => Err(DomainError::InvalidConfig(format!(
                "agent_run_mailbox_messages.drain_mode 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailboxMessageStatus {
    Accepted,
    Queued,
    ReadyToConsume,
    Consuming,
    Dispatched,
    Steered,
    Paused,
    Blocked,
    Failed,
    Deleted,
}

impl MailboxMessageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Queued => "queued",
            Self::ReadyToConsume => "ready_to_consume",
            Self::Consuming => "consuming",
            Self::Dispatched => "dispatched",
            Self::Steered => "steered",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Deleted => "deleted",
        }
    }
}

impl TryFrom<&str> for MailboxMessageStatus {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "accepted" => Ok(Self::Accepted),
            "queued" => Ok(Self::Queued),
            "ready_to_consume" => Ok(Self::ReadyToConsume),
            "consuming" => Ok(Self::Consuming),
            "dispatched" => Ok(Self::Dispatched),
            "steered" => Ok(Self::Steered),
            "paused" => Ok(Self::Paused),
            "blocked" => Ok(Self::Blocked),
            "failed" => Ok(Self::Failed),
            "deleted" => Ok(Self::Deleted),
            other => Err(DomainError::InvalidConfig(format!(
                "agent_run_mailbox_messages.status 无效: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunMailboxMessage {
    pub id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: Option<String>,
    pub origin: MailboxMessageOrigin,
    pub source: MailboxSourceIdentity,
    pub delivery: MailboxDelivery,
    pub barrier: ConsumptionBarrier,
    pub drain_mode: MailboxDrainMode,
    pub status: MailboxMessageStatus,
    pub priority: i32,
    pub order_key: i64,
    pub source_dedup_key: Option<String>,
    pub queued_agent_run_turn_id: Option<String>,
    pub consuming_agent_run_turn_id: Option<String>,
    pub expected_active_agent_run_turn_id: Option<String>,
    pub accepted_agent_run_turn_id: Option<String>,
    pub accepted_protocol_turn_id: Option<String>,
    pub claim_token: Option<Uuid>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    pub command_receipt_id: Option<Uuid>,
    pub payload_json: Option<Value>,
    pub executor_config_json: Option<Value>,
    pub launch_planning_input: Option<Value>,
    pub preview: String,
    pub has_images: bool,
    pub retain_payload: bool,
    pub attempt_count: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAgentRunMailboxMessage {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: Option<String>,
    pub origin: MailboxMessageOrigin,
    pub source: MailboxSourceIdentity,
    pub delivery: MailboxDelivery,
    pub barrier: ConsumptionBarrier,
    pub drain_mode: MailboxDrainMode,
    pub priority: i32,
    pub source_dedup_key: Option<String>,
    pub queued_agent_run_turn_id: Option<String>,
    pub expected_active_agent_run_turn_id: Option<String>,
    pub command_receipt_id: Option<Uuid>,
    pub payload_json: Option<Value>,
    pub executor_config_json: Option<Value>,
    pub launch_planning_input: Option<Value>,
    pub preview: String,
    pub has_images: bool,
    pub retain_payload: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunMailboxState {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: Option<String>,
    pub paused: bool,
    pub pause_reason: Option<String>,
    pub pause_message: Option<String>,
    pub backend_selection_preference: Option<Value>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunMailboxClaimRequest {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: Option<String>,
    pub barriers: Vec<ConsumptionBarrier>,
    pub drain_mode: Option<MailboxDrainMode>,
    pub limit: i64,
    pub claim_token: Uuid,
    pub claim_expires_at: DateTime<Utc>,
}

#[async_trait::async_trait]
pub trait AgentRunMailboxRepository: Send + Sync {
    async fn create_message(
        &self,
        message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError>;

    async fn create_message_idempotent(
        &self,
        message: NewAgentRunMailboxMessage,
    ) -> Result<AgentRunMailboxMessage, DomainError>;

    async fn get_message(&self, id: Uuid) -> Result<Option<AgentRunMailboxMessage>, DomainError>;

    async fn list_messages(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError>;

    async fn claim_next(
        &self,
        request: AgentRunMailboxClaimRequest,
    ) -> Result<Vec<AgentRunMailboxMessage>, DomainError>;

    async fn recover_expired_consuming(&self, now: DateTime<Utc>) -> Result<u64, DomainError>;

    async fn mark_message_status(
        &self,
        id: Uuid,
        claim_token: Option<Uuid>,
        status: MailboxMessageStatus,
        accepted_agent_run_turn_id: Option<String>,
        accepted_protocol_turn_id: Option<String>,
        last_error: Option<String>,
    ) -> Result<AgentRunMailboxMessage, DomainError>;

    async fn update_message_policy(
        &self,
        id: Uuid,
        delivery: MailboxDelivery,
        barrier: ConsumptionBarrier,
        drain_mode: MailboxDrainMode,
        priority: i32,
    ) -> Result<AgentRunMailboxMessage, DomainError>;

    async fn delete_message(&self, id: Uuid)
    -> Result<Option<AgentRunMailboxMessage>, DomainError>;

    async fn cleanup_user_payload(&self, id: Uuid) -> Result<(), DomainError>;

    async fn pause_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: Option<String>,
        reason: String,
        message: Option<String>,
    ) -> Result<AgentRunMailboxState, DomainError>;

    async fn resume_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: Option<String>,
    ) -> Result<AgentRunMailboxState, DomainError>;

    async fn get_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<AgentRunMailboxState>, DomainError>;

    async fn set_backend_selection_preference(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: Option<String>,
        preference: Value,
    ) -> Result<AgentRunMailboxState, DomainError>;

    /// Move a message after the anchor (or to the front if `after_id` is None).
    /// Returns the updated message with new order_key.
    async fn move_message_after(
        &self,
        id: Uuid,
        after_id: Option<Uuid>,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<AgentRunMailboxMessage, DomainError>;
}
