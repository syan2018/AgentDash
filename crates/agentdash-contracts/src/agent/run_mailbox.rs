use agentdash_agent_protocol::codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::session::SessionMessageRefDto;
use crate::workflow::{
    AgentFrameRefDto, AgentRunCommandPreconditionView, AgentRunRefDto, LifecycleRunRefDto,
    RuntimeSessionRefDto,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MailboxMessageOrigin {
    User,
    System,
    Hook,
    Companion,
    Workflow,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MailboxSourceIdentity {
    pub namespace: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub correlation_ref: Option<String>,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route: Option<String>,
    pub display_label_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SteeringStopEffect {
    None,
    ContinueOnStop,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MailboxDelivery {
    LaunchOrContinueTurn,
    SteerActiveTurn { stop_effect: SteeringStopEffect },
    ResumeLaunchSource { launch_source: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsumptionBarrier {
    ImmediateIfIdle,
    AgentLoopTurnBoundary,
    AgentRunTurnBoundary,
    ManualResume,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MailboxDrainMode {
    One,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunMessageAcceptedRefs {
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: AgentRunRefDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frame_ref: Option<AgentFrameRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_session_ref: Option<RuntimeSessionRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agent_run_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub protocol_turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunToolCallApprovalResponse {
    pub approved: bool,
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: AgentRunRefDto,
    pub tool_call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunToolCallRejectionResponse {
    pub rejected: bool,
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: AgentRunRefDto,
    pub tool_call_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct MailboxMessageView {
    pub id: String,
    pub origin: MailboxMessageOrigin,
    pub source: MailboxSourceIdentity,
    pub delivery: MailboxDelivery,
    pub barrier: ConsumptionBarrier,
    pub drain_mode: MailboxDrainMode,
    pub status: MailboxMessageStatus,
    pub preview: String,
    pub has_images: bool,
    pub attempt_count: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub accepted_refs: Option<AgentRunMessageAcceptedRefs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub can_promote: bool,
    pub can_delete: bool,
    pub can_reorder: bool,
    pub can_recall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunMailboxMoveRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub after_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunMailboxMessageContentView {
    pub id: String,
    #[ts(type = "JsonValue")]
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct MailboxStateView {
    pub paused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pause_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
    pub can_resume: bool,
    #[serde(default)]
    pub hide_system_steer_messages: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunCommandReceipt {
    pub client_command_id: String,
    pub status: String,
    pub duplicate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendSelectionModeDto {
    Explicit,
    AutoIdle,
    WorkspaceBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct BackendSelectionRequestDto {
    pub mode: BackendSelectionModeDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backend_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunAcceptedRefs {
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: AgentRunRefDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frame_ref: Option<AgentFrameRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_session_ref: Option<RuntimeSessionRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunComposerSubmitRequest {
    /// canonical 用户输入，由后端写入 mailbox 并按 scheduler outcome 消费或排队。
    pub input: Vec<codex::UserInput>,
    pub client_command_id: String,
    pub command: AgentRunCommandPreconditionView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub executor_config: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backend_selection: Option<BackendSelectionRequestDto>,
    /// 投递意图：`"steer"` 表示用户明确要求注入 active turn，其余情况排队等待。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_intent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeSessionCommandStateDto {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunMessageCommandOutcome {
    Launched,
    Queued,
    Steered,
    Deleted,
    Resumed,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunMessageCommandResponse {
    pub command_receipt: AgentRunCommandReceipt,
    pub outcome: AgentRunMessageCommandOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mailbox_message: Option<MailboxMessageView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub accepted_refs: Option<AgentRunMessageAcceptedRefs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_state: Option<RuntimeSessionCommandStateDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fork: Option<AgentRunForkOutcomeView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunForkRequest {
    pub client_command_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fork_point_ref: Option<SessionMessageRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub metadata_json: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunForkSubmitRequest {
    pub input: Vec<codex::UserInput>,
    pub client_command_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fork_point_ref: Option<SessionMessageRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub metadata_json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub executor_config: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backend_selection: Option<BackendSelectionRequestDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunForkLineageView {
    pub id: String,
    pub parent: AgentRunMessageAcceptedRefs,
    pub child: AgentRunMessageAcceptedRefs,
    pub relation_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fork_point_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fork_point_ref: Option<SessionMessageRefDto>,
    pub forked_by_user_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunForkOutcomeView {
    pub outcome: String,
    pub parent_refs: AgentRunMessageAcceptedRefs,
    pub child_refs: AgentRunMessageAcceptedRefs,
    pub lineage: AgentRunForkLineageView,
    pub redirect: AgentRunRefDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunForkResponse {
    pub command_receipt: AgentRunCommandReceipt,
    pub outcome: String,
    pub parent_refs: AgentRunMessageAcceptedRefs,
    pub child_refs: AgentRunMessageAcceptedRefs,
    pub lineage: AgentRunForkLineageView,
    pub redirect: AgentRunRefDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunMailboxView {
    pub state: MailboxStateView,
    #[serde(default)]
    pub messages: Vec<MailboxMessageView>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mailbox_source_identity_serializes_as_open_wire_object() {
        let source = MailboxSourceIdentity {
            namespace: "routine".to_string(),
            kind: "trigger".to_string(),
            source_ref: Some("routine-execution-1".to_string()),
            correlation_ref: Some("routine-trigger-1".to_string()),
            actor: "routine".to_string(),
            route: Some("reuse".to_string()),
            display_label_key: "mailbox.source.routine.trigger".to_string(),
            metadata: Some(json!({ "entity_key": "story-1" })),
        };

        let value = serde_json::to_value(&source).expect("serialize source identity");
        assert_eq!(
            value,
            json!({
                "namespace": "routine",
                "kind": "trigger",
                "source_ref": "routine-execution-1",
                "correlation_ref": "routine-trigger-1",
                "actor": "routine",
                "route": "reuse",
                "display_label_key": "mailbox.source.routine.trigger",
                "metadata": { "entity_key": "story-1" }
            })
        );

        let decoded: MailboxSourceIdentity =
            serde_json::from_value(value).expect("deserialize source identity");
        assert_eq!(decoded, source);
    }
}
