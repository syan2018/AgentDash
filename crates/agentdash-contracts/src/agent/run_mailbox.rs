use agentdash_agent_protocol::codex_app_server_protocol as codex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::workflow::{AgentFrameRefDto, AgentRunRefDto, LifecycleRunRefDto};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunCommandReceipt {
    pub client_command_id: String,
    pub status: String,
    pub duplicate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub accepted_runtime_operation_id: Option<String>,
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
    pub runtime_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_operation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AgentRunComposerSubmitRequest {
    /// canonical 用户输入，由后端写入 mailbox 并按 scheduler outcome 消费或排队。
    pub input: Vec<codex::UserInput>,
    pub client_command_id: String,
    /// 投递意图：`"steer"` 表示用户明确要求注入 active turn，其余情况排队等待。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_intent: Option<AgentRunComposerDeliveryIntent>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunComposerDeliveryIntent {
    Steer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunMessageCommandOutcome {
    Dispatched,
    Queued,
    Steered,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunMessageCommandResponse {
    pub command_receipt: AgentRunCommandReceipt,
    pub outcome: AgentRunMessageCommandOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mailbox_message_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_submit_rejects_existing_run_execution_overrides() {
        let payload = serde_json::json!({
            "input": [{ "type": "text", "text": "hello", "text_elements": [] }],
            "client_command_id": "command-1",
            "executor_config": { "model_id": "other-model" }
        });

        let error = serde_json::from_value::<AgentRunComposerSubmitRequest>(payload)
            .expect_err("existing AgentRun composer must reject execution overrides");
        assert!(
            error
                .to_string()
                .contains("unknown field `executor_config`")
        );
    }

    #[test]
    fn composer_submit_rejects_unknown_delivery_intent() {
        let payload = serde_json::json!({
            "input": [{ "type": "text", "text": "hello", "text_elements": [] }],
            "client_command_id": "command-1",
            "delivery_intent": "enqueue"
        });

        serde_json::from_value::<AgentRunComposerSubmitRequest>(payload)
            .expect_err("delivery intent must use a canonical generated value");
    }

    #[test]
    fn queued_message_response_exposes_only_the_mailbox_identity() {
        let response = AgentRunMessageCommandResponse {
            command_receipt: AgentRunCommandReceipt {
                client_command_id: "command-1".to_string(),
                status: "queued".to_string(),
                duplicate: false,
                accepted_runtime_operation_id: None,
                message: None,
            },
            outcome: AgentRunMessageCommandOutcome::Queued,
            mailbox_message_id: Some("mailbox-1".to_string()),
        };

        let value = serde_json::to_value(response).expect("serialize queued response");
        assert_eq!(value["mailbox_message_id"], "mailbox-1");
        assert!(value.get("mailbox_message").is_none());
        assert!(value.get("accepted_refs").is_none());
    }
}
