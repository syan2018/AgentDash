use std::{collections::HashMap, path::PathBuf};

use agent_client_protocol::{ContentBlock, McpServer};
use serde::{Deserialize, Serialize};

use agentdash_spi::{AddressSpace, PromptPayload};

/// 纯用户输入 — HTTP 反序列化的目标。
/// 不包含任何后端注入字段。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptInput {
    #[serde(default)]
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_spi::AgentConfig>,
}

/// 后端完整请求 — 包含用户输入 + 后端注入的运行时上下文。
///
/// 由 session bootstrap 代码组合 `UserPromptInput` + 后端注入字段构造。
pub struct PromptSessionRequest {
    pub user_input: UserPromptInput,
    pub mcp_servers: Vec<McpServer>,
    pub workspace_root: Option<PathBuf>,
    pub address_space: Option<AddressSpace>,
    pub flow_capabilities: Option<agentdash_spi::FlowCapabilities>,
    pub system_context: Option<String>,
    /// Session 模型判定出的 bootstrap 动作。
    /// owner 首轮初始化与冷启动续跑都由 session 生命周期层决定，
    /// route / frontend 只传原始用户输入。
    pub bootstrap_action: SessionBootstrapAction,
    /// 发起本次 prompt 的用户身份（由 HTTP handler 从 session 注入）。
    pub identity: Option<agentdash_spi::auth::AuthIdentity>,
    /// Turn 事件回调（替代 TurnMonitor）。
    /// 由 task 执行层注入，在 session pipeline 事件流和终态时回调。
    /// 为 None 时不执行任何回调（普通 session prompt 场景）。
    pub post_turn_handler: Option<super::post_turn_handler::DynPostTurnHandler>,
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
            bootstrap_action: SessionBootstrapAction::None,
            identity: None,
            post_turn_handler: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBootstrapAction {
    None,
    OwnerContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBootstrapState {
    /// 普通 session，不需要 owner 首轮 bootstrap。
    Plain,
    /// 已绑定 owner，但首轮上下文还未正式注入 session 历史。
    Pending,
    /// owner 首轮 bootstrap 已完成；后续仅允许正常续跑或冷启动 rehydrate。
    Bootstrapped,
}

impl Default for SessionBootstrapState {
    fn default() -> Self {
        Self::Plain
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRepositoryRehydrateMode {
    SystemContext,
    ExecutorState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPromptLifecycle {
    Plain,
    OwnerBootstrap,
    RepositoryRehydrate(SessionRepositoryRehydrateMode),
}

pub fn resolve_session_prompt_lifecycle(
    meta: &SessionMeta,
    has_live_runtime: bool,
    supports_repository_restore: bool,
) -> SessionPromptLifecycle {
    if meta.bootstrap_state == SessionBootstrapState::Pending {
        return SessionPromptLifecycle::OwnerBootstrap;
    }

    let has_executor_follow_up = meta
        .executor_session_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    if !has_live_runtime && meta.last_event_seq > 0 && !has_executor_follow_up {
        return SessionPromptLifecycle::RepositoryRehydrate(if supports_repository_restore {
            SessionRepositoryRehydrateMode::ExecutorState
        } else {
            SessionRepositoryRehydrateMode::SystemContext
        });
    }

    SessionPromptLifecycle::Plain
}

#[derive(Debug, Clone)]
pub struct ResolvedPromptPayload {
    pub text_prompt: String,
    pub prompt_payload: PromptPayload,
    pub user_blocks: Vec<ContentBlock>,
}

impl UserPromptInput {
    /// 解析出有效的 prompt payload。
    /// - `text_prompt`：仅用于标题提示 / trace 元信息的文本摘要
    /// - `user_blocks`：注入会话流时保留的原始 ACP ContentBlock
    pub fn resolve_prompt_payload(&self) -> Result<ResolvedPromptPayload, String> {
        let blocks = self
            .prompt_blocks
            .as_ref()
            .ok_or_else(|| "必须提供 promptBlocks".to_string())?;
        if blocks.is_empty() {
            return Err("promptBlocks 不能为空数组".to_string());
        }
        let mut user_blocks = Vec::with_capacity(blocks.len());
        for (index, block) in blocks.iter().enumerate() {
            let parsed = serde_json::from_value::<ContentBlock>(block.clone())
                .map_err(|e| format!("promptBlocks[{index}] 不是有效 ACP ContentBlock: {e}"))?;
            user_blocks.push(parsed);
        }
        let prompt_payload = PromptPayload::Blocks(user_blocks.clone());
        let text_prompt = prompt_payload.to_fallback_text();
        if text_prompt.trim().is_empty() {
            return Err("promptBlocks 中没有有效内容".to_string());
        }
        Ok(ResolvedPromptPayload {
            text_prompt,
            prompt_payload,
            user_blocks,
        })
    }

    pub fn from_text(text: impl AsRef<str>) -> Self {
        let trimmed = text.as_ref().trim();
        Self {
            prompt_blocks: Some(vec![serde_json::json!({
                "type": "text",
                "text": trimmed,
            })]),
            working_dir: None,
            env: HashMap::new(),
            executor_config: None,
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
    /// 指定的 companion agent 名称（如 "code-reviewer"），None 表示继承父会话执行器
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub last_event_seq: u64,
    #[serde(default = "SessionMeta::default_status")]
    pub last_execution_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_terminal_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_config: Option<agentdash_spi::AgentConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_context: Option<CompanionSessionContext>,
    #[serde(default)]
    pub visible_canvas_mount_ids: Vec<String>,
    #[serde(default)]
    pub bootstrap_state: SessionBootstrapState,
}

impl SessionMeta {
    pub(crate) fn default_status() -> String {
        "idle".to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionExecutionState {
    Idle,
    Running {
        turn_id: Option<String>,
    },
    Completed {
        turn_id: String,
    },
    Failed {
        turn_id: String,
        message: Option<String>,
    },
    Interrupted {
        turn_id: Option<String>,
        message: Option<String>,
    },
}
