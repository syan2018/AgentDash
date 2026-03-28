use std::{collections::HashMap, path::PathBuf};

use agent_client_protocol::{ContentBlock, McpServer, TextContent};
use serde::{Deserialize, Serialize};

use agentdash_connector_contract::{AddressSpace, PromptPayload};

/// 纯用户输入 — HTTP 反序列化的目标。
/// 不包含任何后端注入字段。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptInput {
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_connector_contract::AgentConfig>,
}

/// 后端完整请求 — 包含用户输入 + 后端注入的运行时上下文。
///
/// 由 session bootstrap 代码组合 `UserPromptInput` + 后端注入字段构造。
#[derive(Debug, Clone)]
pub struct PromptSessionRequest {
    pub user_input: UserPromptInput,
    pub mcp_servers: Vec<McpServer>,
    pub workspace_root: Option<PathBuf>,
    pub address_space: Option<AddressSpace>,
    pub flow_capabilities: Option<agentdash_connector_contract::FlowCapabilities>,
    pub system_context: Option<String>,
}

impl PromptSessionRequest {
    /// 从 `UserPromptInput` 构造，后端注入字段全部为空。
    pub fn from_user_input(input: UserPromptInput) -> Self {
        Self {
            user_input: input,
            mcp_servers: Vec::new(),
            workspace_root: None,
            address_space: None,
            flow_capabilities: None,
            system_context: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPromptPayload {
    pub text_prompt: String,
    pub prompt_payload: PromptPayload,
    pub user_blocks: Vec<ContentBlock>,
}

impl UserPromptInput {
    /// 解析出有效的 prompt payload。
    /// - `text_prompt`：当前本地执行器仍使用的文本 prompt（由 block 降级拼接）
    /// - `user_blocks`：注入会话流时保留的原始 ACP ContentBlock
    ///
    /// 优先使用 `prompt_blocks`，若不存在则回退到 `prompt` 字段。
    /// 二者同时存在返回 Err。
    pub fn resolve_prompt_payload(&self) -> Result<ResolvedPromptPayload, String> {
        match (&self.prompt, &self.prompt_blocks) {
            (Some(_), Some(_)) => Err("prompt 与 promptBlocks 不能同时传入".to_string()),
            (None, None) => Err("必须提供 prompt 或 promptBlocks".to_string()),
            (Some(p), None) => {
                let trimmed = p.trim();
                if trimmed.is_empty() {
                    Err("prompt 不能为空".to_string())
                } else {
                    let text_prompt = trimmed.to_string();
                    Ok(ResolvedPromptPayload {
                        text_prompt: text_prompt.clone(),
                        prompt_payload: PromptPayload::Text(text_prompt),
                        user_blocks: vec![ContentBlock::Text(TextContent::new(trimmed))],
                    })
                }
            }
            (None, Some(blocks)) => {
                if blocks.is_empty() {
                    return Err("promptBlocks 不能为空数组".to_string());
                }
                let mut user_blocks = Vec::with_capacity(blocks.len());
                for (index, block) in blocks.iter().enumerate() {
                    let parsed =
                        serde_json::from_value::<ContentBlock>(block.clone()).map_err(|e| {
                            format!("promptBlocks[{index}] 不是有效 ACP ContentBlock: {e}")
                        })?;
                    user_blocks.push(parsed);
                }
                let prompt_payload = PromptPayload::Blocks(user_blocks.clone());
                let text_prompt = prompt_payload.to_fallback_text();
                if text_prompt.trim().is_empty() {
                    Err("promptBlocks 中没有有效内容".to_string())
                } else {
                    Ok(ResolvedPromptPayload {
                        text_prompt,
                        prompt_payload,
                        user_blocks,
                    })
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionSessionContext {
    pub dispatch_id: String,
    pub parent_session_id: String,
    pub parent_turn_id: String,
    pub companion_label: String,
    pub slice_mode: String,
    pub adoption_mode: String,
    #[serde(default)]
    pub inherited_fragment_labels: Vec<String>,
    #[serde(default)]
    pub inherited_constraint_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default = "SessionMeta::default_status")]
    pub last_execution_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_terminal_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_config: Option<agentdash_connector_contract::AgentConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_context: Option<CompanionSessionContext>,
}

impl SessionMeta {
    pub(crate) fn default_status() -> String {
        "idle".to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionExecutionState {
    Idle,
    Running { turn_id: Option<String> },
    Completed { turn_id: String },
    Failed { turn_id: String, message: Option<String> },
    Interrupted { turn_id: Option<String>, message: Option<String> },
}
