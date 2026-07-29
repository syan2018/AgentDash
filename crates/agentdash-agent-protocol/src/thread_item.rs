//! AgentDash 运行协议类型出口。
//!
//! Codex Protocol 已经覆盖的 item 与状态语义直接从 Codex 导出；AgentDash 只在
//! Codex 没有一等 variant 的地方做加法扩展。

use crate::codex_app_server_protocol as codex;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;
use ts_rs::TS;

pub use codex::{
    CommandExecutionStatus, DynamicToolCallOutputContentItem, DynamicToolCallStatus,
    McpToolCallStatus, PatchApplyStatus, ThreadItem as CodexThreadItem,
};

/// 工具 owner 声明的 canonical conversation presentation family。
///
/// 该字段随工具定义穿过 Product surface、Complete Agent binding 与 Agent native history；
/// presentation adapter 只消费此声明，不从运行时工具名反推卡片类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "family", rename_all = "snake_case")]
#[ts(tag = "family", export_to = "agentdash/")]
pub enum ToolProtocolProjector {
    Command,
    FileChange,
    FsRead,
    FsGrep,
    FsGlob,
    Mcp { server_key: String },
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
#[ts(export_to = "agentdash/")]
pub enum AgentDashThreadItem {
    AgentDash(AgentDashNativeThreadItem),
    #[ts(type = "Exclude<ThreadItem, { type: \"contextCompaction\" }>")]
    Codex(AgentDashCodexThreadItem),
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(transparent)]
#[ts(
    type = "Exclude<ThreadItem, { type: \"contextCompaction\" }>",
    export_to = "agentdash/"
)]
pub struct AgentDashCodexThreadItem(codex::ThreadItem);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(tag = "type", export_to = "agentdash/")]
pub enum AgentDashNativeThreadItem {
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    ContextCompaction {
        id: String,
        status: AgentDashCompactionStatus,
        error: Option<String>,
        started_at_ms: Option<u64>,
        completed_at_ms: Option<u64>,
        context_revision: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    ShellExec {
        id: String,
        command: String,
        cwd: Option<String>,
        execution_mode: ShellExecExecutionMode,
        arguments: serde_json::Value,
        status: codex::DynamicToolCallStatus,
        aggregated_output: Option<String>,
        exit_code: Option<i32>,
        success: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    TerminalControl {
        id: String,
        operation: String,
        terminal_id: String,
        arguments: serde_json::Value,
        input: Option<String>,
        cols: Option<u16>,
        rows: Option<u16>,
        state: Option<String>,
        aggregated_output: Option<String>,
        exit_code: Option<i32>,
        status: codex::DynamicToolCallStatus,
        success: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    FsRead {
        id: String,
        path: String,
        offset: Option<usize>,
        limit: Option<usize>,
        arguments: serde_json::Value,
        status: codex::DynamicToolCallStatus,
        content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
        success: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    FsGrep {
        id: String,
        pattern: String,
        path: Option<String>,
        glob: Option<String>,
        file_type: Option<String>,
        output_mode: Option<String>,
        head_limit: Option<usize>,
        offset: Option<usize>,
        arguments: serde_json::Value,
        status: codex::DynamicToolCallStatus,
        content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
        success: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    FsGlob {
        id: String,
        pattern: String,
        path: Option<String>,
        max_results: Option<usize>,
        arguments: serde_json::Value,
        status: codex::DynamicToolCallStatus,
        content_items: Option<Vec<codex::DynamicToolCallOutputContentItem>>,
        success: Option<bool>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "agentdash/")]
pub enum AgentDashCompactionStatus {
    InProgress,
    Succeeded,
    Failed,
    Lost,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "agentdash/")]
pub enum AgentDashItemTerminalOutcome {
    Succeeded,
    Failed,
    Lost,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "agentdash/")]
pub struct AgentDashItemTerminal {
    pub outcome: AgentDashItemTerminalOutcome,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("item `{item_id}` is not terminal and cannot carry terminal evidence")]
pub struct AgentDashItemNotTerminal {
    pub item_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "agentdash/")]
pub enum ShellExecExecutionMode {
    Platform,
    MountExec,
}

impl AgentDashThreadItem {
    pub fn id(&self) -> &str {
        match self {
            AgentDashThreadItem::Codex(item) => codex_item_id(item.as_ref()),
            AgentDashThreadItem::AgentDash(item) => item.id(),
        }
    }

    pub fn as_codex(&self) -> Option<&codex::ThreadItem> {
        match self {
            AgentDashThreadItem::Codex(item) => Some(item.as_ref()),
            AgentDashThreadItem::AgentDash(_) => None,
        }
    }

    pub fn tool_call_id(&self) -> Option<&str> {
        match self {
            AgentDashThreadItem::Codex(item) => match item.as_ref() {
                codex::ThreadItem::DynamicToolCall { id, .. }
                | codex::ThreadItem::CommandExecution { id, .. }
                | codex::ThreadItem::McpToolCall { id, .. }
                | codex::ThreadItem::FileChange { id, .. }
                | codex::ThreadItem::CollabAgentToolCall { id, .. } => Some(id.as_str()),
                _ => None,
            },
            AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ContextCompaction {
                ..
            }) => None,
            AgentDashThreadItem::AgentDash(item) => Some(item.id()),
        }
    }

    pub fn is_message(&self) -> bool {
        matches!(
            self,
            AgentDashThreadItem::Codex(AgentDashCodexThreadItem(
                codex::ThreadItem::UserMessage { .. }
                    | codex::ThreadItem::HookPrompt { .. }
                    | codex::ThreadItem::AgentMessage { .. }
            ))
        )
    }

    pub fn is_tool_activity(&self) -> bool {
        !matches!(
            self,
            AgentDashThreadItem::Codex(AgentDashCodexThreadItem(
                codex::ThreadItem::UserMessage { .. }
                    | codex::ThreadItem::HookPrompt { .. }
                    | codex::ThreadItem::AgentMessage { .. }
                    | codex::ThreadItem::Plan { .. }
                    | codex::ThreadItem::Reasoning { .. },
            )) | AgentDashThreadItem::AgentDash(
                AgentDashNativeThreadItem::ContextCompaction { .. }
            )
        )
    }

    pub fn is_file_change(&self) -> bool {
        matches!(
            self,
            AgentDashThreadItem::Codex(AgentDashCodexThreadItem(
                codex::ThreadItem::FileChange { .. },
            ))
        )
    }

    pub fn is_context_compaction(&self) -> bool {
        matches!(
            self,
            AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ContextCompaction { .. })
        )
    }

    pub fn is_terminal_control(&self) -> bool {
        matches!(
            self,
            AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::TerminalControl { .. })
        )
    }

    pub fn terminal_evidence(&self) -> Result<AgentDashItemTerminal, AgentDashItemNotTerminal> {
        let (outcome, error) = match self {
            AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ContextCompaction {
                status,
                error,
                ..
            }) => {
                let outcome = match status {
                    AgentDashCompactionStatus::Succeeded => AgentDashItemTerminalOutcome::Succeeded,
                    AgentDashCompactionStatus::Failed => AgentDashItemTerminalOutcome::Failed,
                    AgentDashCompactionStatus::Lost => AgentDashItemTerminalOutcome::Lost,
                    AgentDashCompactionStatus::Cancelled => AgentDashItemTerminalOutcome::Cancelled,
                    AgentDashCompactionStatus::InProgress => {
                        return Err(AgentDashItemNotTerminal {
                            item_id: self.id().to_owned(),
                        });
                    }
                };
                (outcome, error.clone())
            }
            AgentDashThreadItem::AgentDash(item) => {
                if item
                    .status()
                    .is_some_and(|status| *status == codex::DynamicToolCallStatus::InProgress)
                {
                    return Err(AgentDashItemNotTerminal {
                        item_id: self.id().to_owned(),
                    });
                }
                let failed = item.success() == Some(false)
                    || item
                        .status()
                        .is_some_and(|status| *status == codex::DynamicToolCallStatus::Failed);
                (
                    if failed {
                        AgentDashItemTerminalOutcome::Failed
                    } else {
                        AgentDashItemTerminalOutcome::Succeeded
                    },
                    None,
                )
            }
            AgentDashThreadItem::Codex(item) => codex_terminal_evidence(item.as_ref())?,
        };
        Ok(AgentDashItemTerminal { outcome, error })
    }
}

impl AsRef<codex::ThreadItem> for AgentDashCodexThreadItem {
    fn as_ref(&self) -> &codex::ThreadItem {
        &self.0
    }
}

impl<'de> Deserialize<'de> for AgentDashCodexThreadItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let item = codex::ThreadItem::deserialize(deserializer)?;
        if matches!(item, codex::ThreadItem::ContextCompaction { .. }) {
            return Err(de::Error::custom(
                "Codex contextCompaction must enter the canonical AgentDash item shape",
            ));
        }
        Ok(Self(item))
    }
}

fn codex_terminal_evidence(
    item: &codex::ThreadItem,
) -> Result<(AgentDashItemTerminalOutcome, Option<String>), AgentDashItemNotTerminal> {
    use AgentDashItemTerminalOutcome::{Cancelled, Failed, Succeeded};

    let outcome = match item {
        codex::ThreadItem::CommandExecution { status, .. } => match status {
            codex::CommandExecutionStatus::InProgress => return Err(not_terminal(item)),
            codex::CommandExecutionStatus::Failed => Failed,
            codex::CommandExecutionStatus::Declined => Cancelled,
            _ => Succeeded,
        },
        codex::ThreadItem::FileChange { status, .. } => match status {
            codex::PatchApplyStatus::InProgress => return Err(not_terminal(item)),
            codex::PatchApplyStatus::Failed => Failed,
            codex::PatchApplyStatus::Declined => Cancelled,
            _ => Succeeded,
        },
        codex::ThreadItem::McpToolCall { status, .. } => {
            if *status == codex::McpToolCallStatus::InProgress {
                return Err(not_terminal(item));
            } else if *status == codex::McpToolCallStatus::Failed {
                Failed
            } else {
                Succeeded
            }
        }
        codex::ThreadItem::DynamicToolCall {
            status, success, ..
        } => {
            if *status == codex::DynamicToolCallStatus::InProgress {
                return Err(not_terminal(item));
            } else if *status == codex::DynamicToolCallStatus::Failed
                || success.as_ref().and_then(|value| *value) == Some(false)
            {
                Failed
            } else {
                Succeeded
            }
        }
        codex::ThreadItem::ContextCompaction { .. } => {
            unreachable!("canonical wrapper rejects Codex context compaction")
        }
        _ => Succeeded,
    };
    Ok((outcome, None))
}

fn not_terminal(item: &codex::ThreadItem) -> AgentDashItemNotTerminal {
    AgentDashItemNotTerminal {
        item_id: codex_item_id(item).to_owned(),
    }
}

fn codex_item_id(item: &codex::ThreadItem) -> &str {
    match item {
        codex::ThreadItem::UserMessage { id, .. }
        | codex::ThreadItem::HookPrompt { id, .. }
        | codex::ThreadItem::AgentMessage { id, .. }
        | codex::ThreadItem::Plan { id, .. }
        | codex::ThreadItem::Reasoning { id, .. }
        | codex::ThreadItem::CommandExecution { id, .. }
        | codex::ThreadItem::FileChange { id, .. }
        | codex::ThreadItem::McpToolCall { id, .. }
        | codex::ThreadItem::DynamicToolCall { id, .. }
        | codex::ThreadItem::CollabAgentToolCall { id, .. }
        | codex::ThreadItem::SubAgentActivity { id, .. }
        | codex::ThreadItem::WebSearch { id, .. }
        | codex::ThreadItem::ImageView { id, .. }
        | codex::ThreadItem::Sleep { id, .. }
        | codex::ThreadItem::ImageGeneration { id, .. }
        | codex::ThreadItem::EnteredReviewMode { id, .. }
        | codex::ThreadItem::ExitedReviewMode { id, .. }
        | codex::ThreadItem::ContextCompaction { id, .. } => id,
    }
}

impl AgentDashNativeThreadItem {
    pub fn id(&self) -> &str {
        match self {
            AgentDashNativeThreadItem::ContextCompaction { id, .. }
            | AgentDashNativeThreadItem::ShellExec { id, .. }
            | AgentDashNativeThreadItem::TerminalControl { id, .. }
            | AgentDashNativeThreadItem::FsRead { id, .. }
            | AgentDashNativeThreadItem::FsGrep { id, .. }
            | AgentDashNativeThreadItem::FsGlob { id, .. } => id,
        }
    }

    pub fn tool_name(&self) -> &'static str {
        match self {
            AgentDashNativeThreadItem::ContextCompaction { .. } => "context_compaction",
            AgentDashNativeThreadItem::ShellExec { .. } => "shell_exec",
            AgentDashNativeThreadItem::TerminalControl { .. } => "terminal_control",
            AgentDashNativeThreadItem::FsRead { .. } => "fs_read",
            AgentDashNativeThreadItem::FsGrep { .. } => "fs_grep",
            AgentDashNativeThreadItem::FsGlob { .. } => "fs_glob",
        }
    }

    pub fn arguments(&self) -> Option<&serde_json::Value> {
        match self {
            AgentDashNativeThreadItem::ShellExec { arguments, .. }
            | AgentDashNativeThreadItem::TerminalControl { arguments, .. }
            | AgentDashNativeThreadItem::FsRead { arguments, .. }
            | AgentDashNativeThreadItem::FsGrep { arguments, .. }
            | AgentDashNativeThreadItem::FsGlob { arguments, .. } => Some(arguments),
            AgentDashNativeThreadItem::ContextCompaction { .. } => None,
        }
    }

    pub fn status(&self) -> Option<&codex::DynamicToolCallStatus> {
        match self {
            AgentDashNativeThreadItem::ShellExec { status, .. }
            | AgentDashNativeThreadItem::TerminalControl { status, .. }
            | AgentDashNativeThreadItem::FsRead { status, .. }
            | AgentDashNativeThreadItem::FsGrep { status, .. }
            | AgentDashNativeThreadItem::FsGlob { status, .. } => Some(status),
            AgentDashNativeThreadItem::ContextCompaction { .. } => None,
        }
    }

    pub fn content_items(&self) -> Option<&Vec<codex::DynamicToolCallOutputContentItem>> {
        match self {
            AgentDashNativeThreadItem::FsRead { content_items, .. }
            | AgentDashNativeThreadItem::FsGrep { content_items, .. }
            | AgentDashNativeThreadItem::FsGlob { content_items, .. } => content_items.as_ref(),
            AgentDashNativeThreadItem::ShellExec { .. }
            | AgentDashNativeThreadItem::TerminalControl { .. }
            | AgentDashNativeThreadItem::ContextCompaction { .. } => None,
        }
    }

    pub fn success(&self) -> Option<bool> {
        match self {
            AgentDashNativeThreadItem::ShellExec { success, .. }
            | AgentDashNativeThreadItem::TerminalControl { success, .. }
            | AgentDashNativeThreadItem::FsRead { success, .. }
            | AgentDashNativeThreadItem::FsGrep { success, .. }
            | AgentDashNativeThreadItem::FsGlob { success, .. } => *success,
            AgentDashNativeThreadItem::ContextCompaction { status, .. } => match status {
                AgentDashCompactionStatus::Succeeded => Some(true),
                AgentDashCompactionStatus::Failed | AgentDashCompactionStatus::Lost => Some(false),
                AgentDashCompactionStatus::InProgress | AgentDashCompactionStatus::Cancelled => {
                    None
                }
            },
        }
    }

    pub fn shell_output(&self) -> Option<&str> {
        match self {
            AgentDashNativeThreadItem::ShellExec {
                aggregated_output, ..
            } => aggregated_output.as_deref(),
            _ => None,
        }
    }
}

impl From<codex::ThreadItem> for AgentDashThreadItem {
    fn from(value: codex::ThreadItem) -> Self {
        match value {
            codex::ThreadItem::ContextCompaction { id } => {
                AgentDashNativeThreadItem::ContextCompaction {
                    id,
                    status: AgentDashCompactionStatus::InProgress,
                    error: None,
                    started_at_ms: None,
                    completed_at_ms: None,
                    context_revision: None,
                }
                .into()
            }
            item => AgentDashThreadItem::Codex(AgentDashCodexThreadItem(item)),
        }
    }
}

impl From<crate::generated::codex_v2::server_notification::ThreadItem> for AgentDashThreadItem {
    fn from(value: crate::generated::codex_v2::server_notification::ThreadItem) -> Self {
        let value = serde_json::to_value(value).expect("generated server item serializes");
        let item: codex::ThreadItem = serde_json::from_value(value)
            .expect("generated server item conforms to owned ThreadItem schema");
        item.into()
    }
}

impl From<AgentDashNativeThreadItem> for AgentDashThreadItem {
    fn from(value: AgentDashNativeThreadItem) -> Self {
        AgentDashThreadItem::AgentDash(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentDashCompactionStatus, AgentDashItemTerminalOutcome, AgentDashNativeThreadItem,
        AgentDashThreadItem, ToolProtocolProjector, codex,
    };

    #[test]
    fn dynamic_projector_has_only_its_card_family() {
        assert_eq!(
            serde_json::to_value(ToolProtocolProjector::Dynamic).expect("serialize projector"),
            serde_json::json!({"family": "dynamic"})
        );
    }

    #[test]
    fn typed_context_compaction_round_trip_preserves_terminal_evidence() {
        let item = AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ContextCompaction {
            id: "compact-1".to_owned(),
            status: AgentDashCompactionStatus::Lost,
            error: Some("provider outcome unknown".to_owned()),
            started_at_ms: Some(1_000),
            completed_at_ms: Some(2_000),
            context_revision: None,
        });
        let json = serde_json::to_value(&item).expect("serialize typed compaction item");
        let decoded: AgentDashThreadItem =
            serde_json::from_value(json).expect("deserialize typed compaction item");
        assert_eq!(decoded, item);
        assert_eq!(
            decoded
                .terminal_evidence()
                .expect("lost compaction is terminal")
                .outcome,
            AgentDashItemTerminalOutcome::Lost
        );
    }

    #[test]
    fn codex_compaction_is_normalized_before_serialization() {
        let item = AgentDashThreadItem::from(codex::ThreadItem::ContextCompaction {
            id: "compact-1".to_owned(),
        });
        let json = serde_json::to_value(item).expect("serialize canonical compaction");

        assert_eq!(json["type"], "contextCompaction");
        assert_eq!(json["status"], "inProgress");
        assert!(json.get("mode").is_none());
    }

    #[test]
    fn in_progress_item_cannot_claim_terminal_evidence() {
        let item = AgentDashThreadItem::AgentDash(AgentDashNativeThreadItem::ContextCompaction {
            id: "compact-1".to_owned(),
            status: AgentDashCompactionStatus::InProgress,
            error: None,
            started_at_ms: Some(1_000),
            completed_at_ms: None,
            context_revision: None,
        });

        let error = item
            .terminal_evidence()
            .expect_err("progress item must not become terminal");

        assert_eq!(error.item_id, "compact-1");
    }
}
