use std::collections::HashMap;

use agentdash_agent_protocol::UserInputBlock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use agentdash_spi::CapabilityState;
use agentdash_spi::PromptPayload;
use agentdash_spi::hooks::HookControlTarget;
pub use agentdash_spi::session_persistence::{
    ApplyMountOperationsEffect, ApplyVfsOverlayEffect, CapabilityArtifactSource,
    CapabilityContributionRecord, CapabilityDeclarationRecord, CapabilityDimensionKey,
    EFFECT_TYPE_APPLY_MOUNT_OPERATIONS, EFFECT_TYPE_APPLY_VFS_OVERLAY,
    EFFECT_TYPE_SET_COMPANION_AGENT_ROSTER, EFFECT_TYPE_SET_MCP_SERVER_SET,
    EFFECT_TYPE_SET_TOOL_ACCESS, ExecutionStatus, PendingCapabilityStateTransition,
    RuntimeCapabilityEffectRecord, RuntimeCapabilityTransition, SessionMeta,
    SetCompanionAgentRosterEffect, SetMcpServerSetEffect, SetToolAccessEffect, TitleSource,
};

/// 纯用户输入 — HTTP 反序列化的目标。
/// 不包含任何后端注入字段。
///
/// `input` 是 canonical 用户输入（`Vec<UserInputBlock>`），贯穿
/// API -> 应用 -> 连接器 -> AgentMessage，与 steer 同形；图片等多模态结构化携带，不再拍平。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptInput {
    #[serde(default)]
    pub input: Option<Vec<UserInputBlock>>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub executor_config: Option<agentdash_spi::AgentConfig>,
    #[serde(default)]
    pub backend_selection: Option<BackendSelectionInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackendSelectionInput {
    pub mode: BackendSelectionInputMode,
    #[serde(default)]
    pub backend_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendSelectionInputMode {
    Explicit,
    AutoIdle,
    WorkspaceBinding,
}

pub const CAPABILITY_DIMENSION_TOOL: &str = "tool";
pub const CAPABILITY_DIMENSION_MCP: &str = "mcp";
pub const CAPABILITY_DIMENSION_COMPANION: &str = "companion";
pub const CAPABILITY_DIMENSION_VFS: &str = "vfs";

pub const DECLARATION_TYPE_CAPABILITY_DIRECTIVE: &str = "capability_directive";
pub const DECLARATION_TYPE_MOUNT_OPERATION: &str = "mount_operation";

/// AgentFrame runtime transition 的主目标。
///
/// `frame_id` 表达要更新的 effective runtime surface；`delivery_runtime_session_id`
/// 用于同步 live connector / runtime registry。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFrameRuntimeTarget {
    pub frame_id: Uuid,
    pub delivery_runtime_session_id: String,
}

/// Hook runtime 的业务 owner 与 delivery binding。
///
/// `control_target` 表达 hook policy owner；`delivery_runtime_session_id` 用于同步 live connector
/// 和 runtime registry cache。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFrameHookRuntimeTarget {
    pub control_target: HookControlTarget,
    pub delivery_runtime_session_id: String,
}

impl AgentFrameHookRuntimeTarget {
    pub fn new(
        control_target: HookControlTarget,
        delivery_runtime_session_id: impl Into<String>,
    ) -> Self {
        Self {
            control_target,
            delivery_runtime_session_id: delivery_runtime_session_id.into(),
        }
    }

    pub fn frame_id(&self) -> Uuid {
        self.control_target.frame_id
    }
}

/// 本轮 prompt 是否触发 Hook snapshot 重载 + `SessionStart` hook 触发器。
///
/// 语义为 "hook 层感知的本轮重载指令"；bootstrap 状态由 `LifecycleAgent.bootstrap_status` 管理。
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

/// 单次 prompt launch 的启动路径判定结果。
///
/// 决定了 prompt pipeline 在发起 connector.prompt 前需要执行哪些前置准备。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptLaunchPath {
    /// 普通对话轮：无需额外上下文准备（首轮 / 已有 live runtime / 有 executor follow-up）。
    Plain,
    /// Owner 首轮 bootstrap：session 尚未完成 owner 初始化（`bootstrap_state == Pending`）。
    OwnerBootstrap,
    /// 冷启动恢复：进程重启后需从持久化事件重建上下文。
    RepositoryRehydrate(SessionRepositoryRehydrateMode),
}

/// Session launch 阶段只消费 runtime trace 所需的持久化事实。
#[derive(Debug, Clone, Default)]
pub struct RuntimeTraceLaunchState {
    pub executor_session_id: Option<String>,
    pub last_event_seq: u64,
}

impl RuntimeTraceLaunchState {
    pub fn has_executor_follow_up(&self) -> bool {
        self.executor_session_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
    }
}

impl From<&SessionMeta> for RuntimeTraceLaunchState {
    fn from(meta: &SessionMeta) -> Self {
        Self {
            executor_session_id: meta.executor_session_id.clone(),
            last_event_seq: meta.last_event_seq,
        }
    }
}

/// 根据 runtime trace launch state + LifecycleAgent bootstrap 状态判定 prompt 应走哪种启动路径。
///
/// 判定优先级：
/// 1. Agent bootstrap 未完成 → **OwnerBootstrap**
/// 2. 冷启动（无 live runtime + 有历史事件 + 无 executor follow-up） → **RepositoryRehydrate**
/// 3. 其余（首轮 / 同进程续跑 / 有 executor follow-up） → **Plain**
///
/// `agent_needs_bootstrap` 来自 `LifecycleAgent.needs_bootstrap()`，取代原
/// `SessionMeta.bootstrap_state` 的判断。
pub fn resolve_prompt_launch_path(
    runtime_trace_state: &RuntimeTraceLaunchState,
    has_live_executor_session: bool,
    supports_repository_restore: bool,
    agent_needs_bootstrap: bool,
) -> PromptLaunchPath {
    // P1: Agent 未完成首轮 bootstrap
    if agent_needs_bootstrap {
        return PromptLaunchPath::OwnerBootstrap;
    }

    // P2: 冷启动恢复（三个条件同时满足）：
    //   - 进程内没有该 session 的 live connector runtime
    //   - session 有历史事件（last_event_seq > 0 表示曾经执行过）
    //   - 执行器侧没有可复用的 follow-up session（否则直接 Plain 续跑）
    if !has_live_executor_session
        && runtime_trace_state.last_event_seq > 0
        && !runtime_trace_state.has_executor_follow_up()
    {
        return PromptLaunchPath::RepositoryRehydrate(if supports_repository_restore {
            SessionRepositoryRehydrateMode::ExecutorState
        } else {
            SessionRepositoryRehydrateMode::SystemContext
        });
    }

    // P3: 默认 — 普通对话轮
    PromptLaunchPath::Plain
}

#[derive(Debug, Clone)]
pub struct ResolvedPromptPayload {
    pub text_prompt: String,
    pub prompt_payload: PromptPayload,
    /// canonical 用户输入：贯穿投递与持久化的单一形态。
    pub input: Vec<UserInputBlock>,
}

impl UserPromptInput {
    /// 解析出有效的 prompt payload。
    /// - `text_prompt`：仅用于标题提示 / trace 元信息的文本摘要
    /// - `input`：canonical 用户输入（`Vec<UserInputBlock>`），投递与持久化的单一形态
    ///
    /// 入参已是 canonical `Vec<UserInputBlock>`（与 steer 同形），不再经 ACP ContentBlock 反序列化。
    pub fn resolve_prompt_payload(&self) -> Result<ResolvedPromptPayload, String> {
        let input = self
            .input
            .as_ref()
            .ok_or_else(|| "必须提供 input".to_string())?;
        if input.is_empty() {
            return Err("input 不能为空数组".to_string());
        }
        let prompt_payload = PromptPayload::Input(input.clone());
        let text_prompt = prompt_payload.to_fallback_text();
        if text_prompt.trim().is_empty() {
            return Err("input 中没有有效内容".to_string());
        }
        Ok(ResolvedPromptPayload {
            text_prompt,
            prompt_payload,
            input: input.clone(),
        })
    }

    pub fn from_text(text: impl AsRef<str>) -> Self {
        let trimmed = text.as_ref().trim();
        Self {
            input: Some(agentdash_agent_protocol::text_user_input_blocks(trimmed)),
            env: HashMap::new(),
            executor_config: None,
            backend_selection: None,
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
    Cancelling {
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
    Lost {
        turn_id: Option<String>,
        message: Option<String>,
    },
}
