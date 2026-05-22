use std::collections::{BTreeMap, BTreeSet, HashMap};

use agentdash_agent_protocol::ContentBlock;
use agentdash_domain::workflow::{MountDirective, ToolCapabilityDirective};
use serde::{Deserialize, Serialize};

use agentdash_domain::session_binding::StorySessionId;
pub use agentdash_spi::CapabilityState;
use agentdash_spi::PromptPayload;
use agentdash_spi::context::capability::CompanionAgentEntry;
use agentdash_spi::{SessionMcpServer, ToolCapability, ToolCapabilityFilter, ToolCluster, Vfs};
use uuid::Uuid;

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

/// PhaseNode 已激活但当前没有 live turn 可热更新时，写入 runtime command store 的意图。
///
/// 下一次 prompt 进入 pipeline 时会按顺序消费 requested runtime commands，把 patch replay
/// 到 construction base projection 上，再由 capability projection normalizer 补齐派生维度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCapabilityStateTransition {
    pub id: String,
    pub run_id: Uuid,
    pub lifecycle_key: String,
    pub phase_node: String,
    pub capability_keys: BTreeSet<String>,
    pub patch: RuntimeContextPatch,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
}

/// Runtime command store 持久化的上下文变更意图。
///
/// 这里保存的是可解释 patch，而不是完整 `CapabilityState` 快照。Skill baseline、
/// effective VFS、runtime surface 等派生维度在 replay 后由 projection pipeline 生成。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeContextPatch {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_directives: Vec<ToolCapabilityDirective>,
    #[serde(default)]
    pub tool_intent: RuntimeToolIntent,
    #[serde(default)]
    pub mcp_intent: RuntimeMcpIntent,
    #[serde(default)]
    pub companion_intent: RuntimeCompanionIntent,
    #[serde(default)]
    pub vfs_intent: RuntimeVfsIntent,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuntimeToolIntent {
    #[default]
    NoChange,
    SetEffectiveTool {
        capabilities: BTreeSet<ToolCapability>,
        enabled_clusters: BTreeSet<ToolCluster>,
        tool_policy: BTreeMap<String, ToolCapabilityFilter>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuntimeMcpIntent {
    #[default]
    NoChange,
    SetEffectiveServers {
        servers: Vec<SessionMcpServer>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuntimeCompanionIntent {
    #[default]
    NoChange,
    SetAgents {
        agents: Vec<CompanionAgentEntry>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeVfsIntent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<Vfs>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mount_directives: Vec<MountDirective>,
}

impl RuntimeVfsIntent {
    pub fn is_empty(&self) -> bool {
        self.overlay.is_none() && self.mount_directives.is_empty()
    }
}

impl RuntimeContextPatch {
    pub fn from_effective_runtime_projection(
        state: &CapabilityState,
        vfs_overlay: Option<Vfs>,
        mount_directives: Vec<MountDirective>,
        tool_directives: Vec<ToolCapabilityDirective>,
    ) -> Self {
        Self {
            tool_directives,
            tool_intent: RuntimeToolIntent::SetEffectiveTool {
                capabilities: state.tool.capabilities.clone(),
                enabled_clusters: state.tool.enabled_clusters.clone(),
                tool_policy: state.tool.tool_policy.clone(),
            },
            mcp_intent: RuntimeMcpIntent::SetEffectiveServers {
                servers: state.tool.mcp_servers.clone(),
            },
            companion_intent: RuntimeCompanionIntent::SetAgents {
                agents: state.companion.agents.clone(),
            },
            vfs_intent: RuntimeVfsIntent {
                overlay: vfs_overlay,
                mount_directives,
            },
        }
    }
}

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

/// 会话标题来源：区分 LLM 自动生成 vs 用户手动设定。
/// `User` 标题不会被自动覆盖；`Auto` 标题可在下次生成时更新。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TitleSource {
    #[default]
    Auto,
    User,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBootstrapState {
    /// 普通 session，不需要 owner 首轮 bootstrap。
    #[default]
    Plain,
    /// 已绑定 owner，但首轮上下文还未正式注入 session 历史。
    Pending,
    /// owner 首轮 bootstrap 已完成；后续仅允许正常续跑或冷启动 rehydrate。
    Bootstrapped,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionSessionContext {
    pub dispatch_id: String,
    /// 主（父）Story session ID — companion 的 owner。
    ///
    /// Model C 下 companion session 是 Story root session 的子会话（见
    /// `.trellis/spec/backend/story-task-runtime.md` §2.5）。JSON wire 字段
    /// 名保持 `parentSessionId`（camelCase）不变；类型改为 [`StorySessionId`]
    /// 只是在签名上明示"这个 ID 指向 Story root"。
    pub parent_session_id: StorySessionId,
    pub parent_turn_id: String,
    pub companion_label: String,
    pub slice_mode: String,
    pub adoption_mode: String,
    /// dispatch 请求的 payload.type（用于 companion_respond 结果类型校验）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_type: Option<String>,
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
    #[serde(default)]
    pub title_source: TitleSource,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub last_event_seq: u64,
    #[serde(default)]
    pub last_execution_status: ExecutionStatus,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_layout: Option<serde_json::Value>,
    #[serde(default)]
    pub visible_canvas_mount_ids: Vec<String>,
    #[serde(default)]
    pub bootstrap_state: SessionBootstrapState,
}

/// Session 执行状态（持久化到 `SessionMeta.last_execution_status`）。
///
/// 替代原先裸字符串 `"idle"/"running"/"completed"/"failed"/"interrupted"` 的散落字面量。
/// 序列化为 `snake_case` 字符串，与数据库已有值兼容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    #[default]
    Idle,
    Running,
    Completed,
    Failed,
    Interrupted,
}

impl ExecutionStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Interrupted)
    }
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Interrupted => write!(f, "interrupted"),
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
