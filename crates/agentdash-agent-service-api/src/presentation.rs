use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentInteractionRequest {
    Approval {
        prompt: String,
        reason: Option<String>,
        proposed_action: Option<Value>,
    },
    UserInput {
        prompt: String,
        questions: Vec<AgentInteractionQuestion>,
    },
    McpElicitation {
        server: String,
        prompt: String,
        schema: Value,
    },
    DynamicTool {
        namespace: Option<String>,
        tool: String,
        prompt: String,
        arguments: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentInteractionQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub allows_free_form: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentInteractionStatus {
    Pending,
    Resolved,
    Cancelled,
    Expired,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentInteractionResolution {
    Approved,
    Denied { reason: Option<String> },
    UserInput { answers: Value },
    McpElicitation { response: Value },
    DynamicToolResult { result: Value },
    Cancelled { reason: Option<String> },
    Expired,
    Lost { reason: String },
}
