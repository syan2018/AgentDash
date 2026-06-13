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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailboxMessageSource {
    Composer,
    DraftStart,
    HookAfterTurn,
    HookBeforeStop,
    HookAutoResume,
    CompanionParentResume,
    WorkflowOrchestrator,
    RoutineExecutor,
    LocalRelayPrompt,
}

impl MailboxMessageSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Composer => "composer",
            Self::DraftStart => "draft_start",
            Self::HookAfterTurn => "hook_after_turn",
            Self::HookBeforeStop => "hook_before_stop",
            Self::HookAutoResume => "hook_auto_resume",
            Self::CompanionParentResume => "companion_parent_resume",
            Self::WorkflowOrchestrator => "workflow_orchestrator",
            Self::RoutineExecutor => "routine_executor",
            Self::LocalRelayPrompt => "local_relay_prompt",
        }
    }
}

impl TryFrom<&str> for MailboxMessageSource {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "composer" => Ok(Self::Composer),
            "draft_start" => Ok(Self::DraftStart),
            "hook_after_turn" => Ok(Self::HookAfterTurn),
            "hook_before_stop" => Ok(Self::HookBeforeStop),
            "hook_auto_resume" => Ok(Self::HookAutoResume),
            "companion_parent_resume" => Ok(Self::CompanionParentResume),
            "workflow_orchestrator" => Ok(Self::WorkflowOrchestrator),
            "routine_executor" => Ok(Self::RoutineExecutor),
            "local_relay_prompt" => Ok(Self::LocalRelayPrompt),
            other => Err(DomainError::InvalidConfig(format!(
                "agent_run_mailbox_messages.source 无效: {other}"
            ))),
        }
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
    pub runtime_session_id: String,
    pub origin: MailboxMessageOrigin,
    pub source: MailboxMessageSource,
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
    pub runtime_session_id: String,
    pub origin: MailboxMessageOrigin,
    pub source: MailboxMessageSource,
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
    pub preview: String,
    pub has_images: bool,
    pub retain_payload: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunMailboxState {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub runtime_session_id: String,
    pub paused: bool,
    pub pause_reason: Option<String>,
    pub pause_message: Option<String>,
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
        runtime_session_id: String,
        reason: String,
        message: Option<String>,
    ) -> Result<AgentRunMailboxState, DomainError>;

    async fn resume_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        runtime_session_id: String,
    ) -> Result<AgentRunMailboxState, DomainError>;

    async fn get_state(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Option<AgentRunMailboxState>, DomainError>;

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
