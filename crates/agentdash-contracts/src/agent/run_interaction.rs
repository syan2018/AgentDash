use agentdash_agent_service_api::AgentInputContent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::session::SessionMessageRefDto;
use crate::workflow::{
    AgentFrameRefDto, AgentRunCommandPreconditionView, AgentRunRefDto, LifecycleRunRefDto,
};

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
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunComposerSubmitRequest {
    /// canonical 用户输入，由后端同步交给具体 Agent。
    pub input: Vec<AgentInputContent>,
    pub client_command_id: String,
    pub command: AgentRunCommandPreconditionView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub executor_config: Option<Value>,
    /// 投递意图：`"steer"` 表示用户明确要求注入 active turn。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_intent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backend_selection: Option<BackendSelectionRequestDto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunMessageCommandOutcome {
    Launched,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunMessageCommandResponse {
    pub command_receipt: AgentRunCommandReceipt,
    pub outcome: AgentRunMessageCommandOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub accepted_refs: Option<AgentRunMessageAcceptedRefs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fork: Option<AgentRunForkOutcomeView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunCommandOnlyRequest {
    pub client_command_id: String,
    pub command: AgentRunCommandPreconditionView,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunContextCompactionCommandOutcome {
    ScheduledNextTurn,
    LaunchedCompactionTurn,
    Completed,
    NoEligibleMessages,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunContextCompactionCommandResponse {
    pub command_receipt: AgentRunCommandReceipt,
    pub outcome: AgentRunContextCompactionCommandOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
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
    pub input: Vec<AgentInputContent>,
    pub client_command_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "JsonValue")]
    pub executor_config: Option<Value>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_submit_preserves_existing_run_execution_override() {
        let payload = serde_json::json!({
            "input": [{ "kind": "text", "text": "hello" }],
            "client_command_id": "command-1",
            "executor_config": { "model_id": "other-model" }
        });

        let request = serde_json::from_value::<AgentRunComposerSubmitRequest>(payload)
            .expect("existing AgentRun composer accepts an explicit execution override");
        assert_eq!(
            request.executor_config,
            Some(serde_json::json!({ "model_id": "other-model" }))
        );
    }

    #[test]
    fn composer_submit_accepts_enqueue_delivery_intent() {
        let payload = serde_json::json!({
            "input": [{ "kind": "text", "text": "hello" }],
            "client_command_id": "command-1",
            "delivery_intent": "enqueue"
        });

        let request = serde_json::from_value::<AgentRunComposerSubmitRequest>(payload)
            .expect("unknown delivery values remain forward compatible");
        assert_eq!(request.delivery_intent.as_deref(), Some("enqueue"));
    }

    #[test]
    fn launched_message_response_exposes_command_receipt() {
        let response = AgentRunMessageCommandResponse {
            command_receipt: AgentRunCommandReceipt {
                client_command_id: "command-1".to_string(),
                status: "accepted".to_string(),
                duplicate: false,
                message: None,
            },
            outcome: AgentRunMessageCommandOutcome::Launched,
            accepted_refs: None,
            fork: None,
        };

        let value = serde_json::to_value(response).expect("serialize launched response");
        assert!(value.get("accepted_refs").is_none());
    }
}
