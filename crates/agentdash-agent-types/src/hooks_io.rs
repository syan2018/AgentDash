use serde::{Deserialize, Serialize};

use crate::content::ContentPart;
use crate::context::AgentContext;
use crate::message::{AgentMessage, ToolCallInfo};
use crate::tool::AgentToolResult;

// ─── Hook 输入/输出 DTO ────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct BeforeToolCallResult {
    pub block: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AfterToolCallResult {
    pub content: Option<Vec<ContentPart>>,
    pub details: Option<serde_json::Value>,
    pub is_error: Option<bool>,
}

pub struct BeforeToolCallContext<'a> {
    pub assistant_message: &'a AgentMessage,
    pub tool_call: &'a ToolCallInfo,
    pub args: &'a serde_json::Value,
    pub context: &'a AgentContext,
}

pub struct AfterToolCallContext<'a> {
    pub assistant_message: &'a AgentMessage,
    pub tool_call: &'a ToolCallInfo,
    pub args: &'a serde_json::Value,
    pub result: &'a AgentToolResult,
    pub is_error: bool,
    pub context: &'a AgentContext,
}

// ─── Tool Approval Types ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolApprovalRequest {
    pub tool_call: ToolCallInfo,
    pub args: serde_json::Value,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ToolApprovalOutcome {
    Approved,
    Rejected {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}
