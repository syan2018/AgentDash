use std::collections::HashMap;

use agentdash_agent_protocol::ContentBlock;
use serde::{Deserialize, Serialize};

pub use agentdash_spi::CapabilityState;
use agentdash_spi::PromptPayload;
pub use agentdash_spi::session_persistence::{
    ApplyMountOperationsEffect, ApplyVfsOverlayEffect, CapabilityArtifactSource,
    CapabilityContributionRecord, CapabilityDeclarationRecord, CapabilityDimensionKey,
    CompanionSessionContext, EFFECT_TYPE_APPLY_MOUNT_OPERATIONS, EFFECT_TYPE_APPLY_VFS_OVERLAY,
    EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER, EFFECT_TYPE_SET_MCP_SERVER_SET,
    EFFECT_TYPE_SET_TOOL_ACCESS, ExecutionStatus, PendingCapabilityStateTransition,
    RuntimeCapabilityEffectRecord, RuntimeCapabilityTransition, SessionBootstrapState, SessionMeta,
    SetCompanionAgentRosterEffect, SetMcpServerSetEffect, SetToolAccessEffect, TitleSource,
};

/// 纯用户输入 — HTTP 反序列化的目标。
/// 不包含任何后端注入字段。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptInput {
    #[serde(default)]
    pub prompt_blocks: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_spi::AgentConfig>,
}

pub const CAPABILITY_DIMENSION_TOOL: &str = "tool";
pub const CAPABILITY_DIMENSION_MCP: &str = "mcp";
pub const CAPABILITY_DIMENSION_COMPANION: &str = "companion";
pub const CAPABILITY_DIMENSION_VFS: &str = "vfs";

pub const DECLARATION_TYPE_CAPABILITY_DIRECTIVE: &str = "capability_directive";
pub const DECLARATION_TYPE_MOUNT_OPERATION: &str = "mount_operation";

/// 本轮 prompt 是否触发 Hook snapshot 重载 + `SessionStart` hook 触发器。
///
/// 本类型由 E7（`04-30-session-pipeline-architecture-refactor`）从
/// `SessionBootstrapAction` 重命名而来，语义收敛为"hook 层感知的本轮重载指令"；
/// `SessionMeta.bootstrap_state` 仍然独立负责 session 生命周期持久化标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HookSnapshotReloadTrigger {
    /// 本轮不触发 hook snapshot 重载（普通续跑场景）。
    #[default]
    None,
    /// 本轮需要重新 load hook snapshot，并触发 `SessionStart` hook。
    /// 典型场景：owner 首轮初始化、冷启动续跑。
    Reload,
}

/// Session 恢复时的上下文重建策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRepositoryRehydrateMode {
    /// 从持久化事件重建 continuation context frame（适用于不支持 executor restore 的执行器）。
    SystemContext,
    /// 从持久化事件重建为 `Vec<AgentMessage>`，交由 connector 走执行器原生的 session restore。
    ExecutorState,
}

/// Session prompt 的生命周期阶段判定结果。
///
/// 决定了 prompt pipeline 在发起 connector.prompt 前需要执行哪些前置准备。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPromptLifecycle {
    /// 普通对话轮：无需额外上下文准备（首轮 / 已有 live runtime / 有 executor follow-up）。
    Plain,
    /// Owner 首轮 bootstrap：session 尚未完成 owner 初始化（`bootstrap_state == Pending`）。
    OwnerBootstrap,
    /// 冷启动恢复：进程重启后需从持久化事件重建上下文。
    RepositoryRehydrate(SessionRepositoryRehydrateMode),
}

/// 根据 session 元数据判定当前 prompt 应走哪种生命周期路径。
///
/// 判定优先级：
/// 1. `Pending bootstrap` → **OwnerBootstrap**：session 尚未完成 owner 初始化
/// 2. 冷启动（无 live runtime + 有历史事件 + 无 executor follow-up） → **RepositoryRehydrate**
/// 3. 其余（首轮 / 同进程续跑 / 有 executor follow-up） → **Plain**
pub fn resolve_session_prompt_lifecycle(
    meta: &SessionMeta,
    has_live_executor_session: bool,
    supports_repository_restore: bool,
) -> SessionPromptLifecycle {
    // P1: 未完成 owner bootstrap 的 session 必须走初始化流程
    if meta.bootstrap_state == SessionBootstrapState::Pending {
        return SessionPromptLifecycle::OwnerBootstrap;
    }

    let has_executor_follow_up = meta
        .executor_session_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    // P2: 冷启动恢复（三个条件同时满足）：
    //   - 进程内没有该 session 的 live connector runtime
    //   - session 有历史事件（last_event_seq > 0 表示曾经执行过）
    //   - 执行器侧没有可复用的 follow-up session（否则直接 Plain 续跑）
    if !has_live_executor_session && meta.last_event_seq > 0 && !has_executor_follow_up {
        return SessionPromptLifecycle::RepositoryRehydrate(if supports_repository_restore {
            SessionRepositoryRehydrateMode::ExecutorState
        } else {
            SessionRepositoryRehydrateMode::SystemContext
        });
    }

    // P3: 默认 — 普通对话轮
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
            env: HashMap::new(),
            executor_config: None,
        }
    }
}

/// 带有运行时上下文的执行状态（含 turn_id / message 等附加信息）。
///
/// 不用于持久化，仅用于 API 查询响应。
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
