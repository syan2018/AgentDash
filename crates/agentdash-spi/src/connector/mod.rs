use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
};

use agentdash_agent_types::{AgentMessage, MessageRef};
use agentdash_domain::backend::BackendExecutionSelectionMode;
use agentdash_domain::common::{AgentConfig, Vfs};
use async_trait::async_trait;
use futures::Stream;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::context::capability::SkillEntry;
use crate::hooks::{ContextFrame, HookRuntimeAccess};
use agentdash_agent_types::DynAgentRuntimeDelegate;

pub mod capability_delta;

pub use capability_delta::{
    CapabilityStateDelta, DefaultMountDelta, NamedEntityDelta, SetDelta, VfsSurfaceDelta,
    compute_capability_state_delta,
};

/// 连接器类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorType {
    /// 本地子进程执行器（Claude Code, Codex, AMP 等）
    LocalExecutor,
    /// 远程 ACP 后端
    RemoteAcpBackend,
}

/// 连接器能力声明
#[derive(Debug, Clone, Default, Serialize)]
pub struct ConnectorCapabilities {
    pub supports_cancel: bool,
    pub supports_steering: bool,
    pub supports_discovery: bool,
    pub supports_variants: bool,
    pub supports_model_override: bool,
    pub supports_permission_policy: bool,
    pub supports_source_session_title: bool,
}

/// 连接器对外暴露的执行器选项（用于前端选择器渲染）
#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub available: bool,
}

/// Session 级执行上下文（Who + Where）。
///
/// 在当前 turn 内语义上不可变：它是 session 启动时拍下的"身份 + 执行环境"快照。
/// 下一 turn 若有改动则重新组装，不在 turn 内 mutate 这里的字段。
#[derive(Clone)]
pub struct ExecutionSessionFrame {
    pub turn_id: String,
    pub working_directory: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub executor_config: AgentConfig,
    /// 本轮完整 MCP 声明（内部类型，带 relay 标记）。
    ///
    /// 云端内嵌 connector 不自行处理这里的 MCP，而是消费 Application 已经预构建好的
    /// `turn.assembled_tools`。Relay/remote transport connector 可将该结构原样下发给
    /// 远端 agent，由远端 agent 自行建联。
    pub mcp_servers: Vec<SessionMcpServer>,
    pub vfs: Option<Vfs>,
    /// Relay/backend execution placement resolved during session launch.
    ///
    /// This field is set only for remote backend executions. It is the connector-facing
    /// projection of the already claimed backend execution lease.
    pub backend_execution: Option<ExecutionBackendPlacement>,
    /// 发起本次执行的用户身份（由 HTTP 层注入）。
    pub identity: Option<crate::platform::auth::AuthIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionBackendPlacement {
    pub backend_id: String,
    pub lease_id: uuid::Uuid,
    pub selection_mode: BackendExecutionSelectionMode,
}

/// Turn 级执行上下文（How + 运行时控制面）。
///
/// per-turn 动态：工具集、hook runtime、runtime delegate、系统 prompt 产出等；
/// 会随 session hot-update 或 hook 触发而重建。
#[derive(Clone, Default)]
pub struct ExecutionTurnFrame {
    pub hook_runtime: Option<Arc<dyn HookRuntimeAccess>>,
    pub capability_state: CapabilityState,
    pub runtime_delegate: Option<DynAgentRuntimeDelegate>,
    /// 当 session 生命周期层判定为"冷启动仓储恢复"且执行器支持原生恢复时，
    /// 会把重建出的消息历史放在这里，供 connector 恢复连续会话。
    pub restored_session_state: Option<RestoredSessionState>,
    /// 本轮可见的 ContextFrame 列表（含 identity / mission / capability / pending_action...）。
    ///
    /// connector 需要自行决定消费策略：
    /// - in-process connector 可按 frame kind 分类消费（如 identity 走 set_system_prompt）。
    /// - generic connector 可把 frames 渲染成文本后与用户输入拼接。
    pub context_frames: Vec<ContextFrame>,
    /// Application 层预构建的工具列表（runtime + direct MCP + relay MCP）。
    ///
    /// 内嵌 connector 只持有并调用这里的 `DynAgentTool`，不重新持有
    /// `McpServer` 声明，也不自行区分 direct / relay MCP。
    pub assembled_tools: Vec<agentdash_agent_types::DynAgentTool>,
}

/// 连接器拿到的一次 `prompt(...)` 调用上下文。
///
/// 拆分为 `session`（Who/Where，不可变）与 `turn`（How，可变）两层，让 connector
/// 能清晰区分"身份 + 执行环境"与"本轮工具 + 运行时控制面"。
#[derive(Clone)]
pub struct ExecutionContext {
    pub session: ExecutionSessionFrame,
    pub turn: ExecutionTurnFrame,
}

/// VFS 中发现的项目级指导文件。
#[derive(Debug, Clone)]
pub struct DiscoveredGuideline {
    /// 文件名（如 `AGENTS.md`）。
    pub file_name: String,
    /// 所属 mount 标识。
    pub mount_id: String,
    /// 相对于 mount 根的路径（如 `AGENTS.md` 或 `packages/foo/AGENTS.md`）。
    pub path: String,
    /// 文件全文内容。
    pub content: String,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("turn_id", &self.session.turn_id)
            .field("executor_config", &self.session.executor_config)
            .field("hook_runtime", &self.turn.hook_runtime)
            .field(
                "runtime_delegate",
                &self.turn.runtime_delegate.as_ref().map(|_| ".."),
            )
            .field(
                "restored_session_state",
                &self
                    .turn
                    .restored_session_state
                    .as_ref()
                    .map(|state| state.messages.len()),
            )
            .field("context_frames", &self.turn.context_frames.len())
            .finish_non_exhaustive()
    }
}

/// 从 `ExecutionContext.session.vfs` 的 default mount 解析工作区路径（`root_ref` 按本地路径处理）。
pub fn workspace_path_from_context(context: &ExecutionContext) -> Result<PathBuf, ConnectorError> {
    let space =
        context.session.vfs.as_ref().ok_or_else(|| {
            ConnectorError::InvalidConfig("ExecutionContext 缺少 vfs".to_string())
        })?;
    let mount = space
        .default_mount()
        .ok_or_else(|| ConnectorError::InvalidConfig("vfs 缺少 default_mount".to_string()))?;
    let path = PathBuf::from(mount.root_ref.trim());
    if path.as_os_str().is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "default mount 的 root_ref 为空".to_string(),
        ));
    }
    Ok(path)
}

#[derive(Debug, Clone, Default)]
pub struct DiscoveryContext {
    pub working_dir: Option<PathBuf>,
    pub identity: Option<crate::platform::auth::AuthIdentity>,
}

#[derive(Debug, Clone, Default)]
pub struct RestoredSessionState {
    pub messages: Vec<AgentMessage>,
    pub message_refs: Vec<Option<MessageRef>>,
}

/// 工具簇标识 — 每个簇控制一组相关工具的注入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCluster {
    /// Read-only access: mounts_list, fs_read, fs_glob, fs_grep
    Read,
    /// File editing: fs_apply_patch
    Write,
    /// 命令执行：shell_exec
    Execute,
    /// Workflow 节点终结：complete_lifecycle_node
    Workflow,
    /// 协作与交互：companion_request, companion_respond
    Collaboration,
    /// Canvas 资产：canvases_list, canvas_start, bind_canvas_data, present_canvas
    Canvas,
}

/// 单个 capability 下的工具级过滤规则。
///
/// 这是 `ToolCapabilityDirective` 归约后的运行态权威表示；directive 只存在于配置层。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCapabilityFilter {
    /// 非空时，该 capability 只允许这些工具。
    pub include_only: BTreeSet<String>,
    /// 明确屏蔽的工具。
    pub exclude: BTreeSet<String>,
}

impl ToolCapabilityFilter {
    pub fn is_empty(&self) -> bool {
        self.include_only.is_empty() && self.exclude.is_empty()
    }

    pub fn allows(&self, tool_name: &str) -> bool {
        if !self.include_only.is_empty() && !self.include_only.contains(tool_name) {
            return false;
        }
        !self.exclude.contains(tool_name)
    }
}

/// 工具 + MCP 维度的运行态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDimension {
    /// 最终生效的能力全集（well-known + custom MCP）。
    pub capabilities: BTreeSet<crate::ToolCapability>,
    /// capability 展开后的运行态工具簇集合。
    pub enabled_clusters: BTreeSet<ToolCluster>,
    /// 运行态唯一工具级过滤表；key 是 capability key，value 是该 capability 下的工具策略。
    pub tool_policy: BTreeMap<String, ToolCapabilityFilter>,
    /// 平台 + 自定义 MCP server 完整列表。
    pub mcp_servers: Vec<SessionMcpServer>,
}

/// Companion 维度的运行态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionDimension {
    /// 当前 session 可调用的 companion agent 列表。
    pub agents: Vec<crate::context::capability::CompanionAgentEntry>,
}

/// VFS 维度的运行态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfsDimension {
    /// 运行态文件/上下文访问状态。
    pub active: Option<Vfs>,
}

/// Skill 维度的运行态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDimension {
    /// 当前 session 可见的 skills（由 workspace 发现链路产出）。
    pub skills: Vec<SkillEntry>,
}

/// 解析后的能力运行态——AgentFrame revision 的只读投影。
///
/// **数据流向**：
/// - **写入**：所有写入通过 `AgentFrameBuilder::with_capability_state` →
///   AgentFrame revision（持久化权威源）→ 内存缓存同步。
/// - **读取**：运行时读取来自内存缓存（`SessionProfile.capability_state` /
///   `TurnExecution.capability_state`），缓存内容与最新 frame revision 保持一致。
/// - **投影**：`project_capability_state_from_frame` 从 AgentFrame JSON 反序列化出此结构。
///
/// 保留此结构体用于序列化、事件 payload（`CapabilityStateDelta`）、
/// 以及各层消费者的只读查询。直接对字段赋值仅限于 diff 应用路径
/// （`replay_effect` 等纯函数在 clone 副本上操作）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityState {
    /// 工具 + MCP 维度。
    pub tool: ToolDimension,
    /// Companion 维度。
    pub companion: CompanionDimension,
    /// VFS 维度。
    pub vfs: VfsDimension,
    /// Skill 维度。
    #[serde(default)]
    pub skill: SkillDimension,
}

impl CapabilityState {
    /// 全量簇 — 适用于 Story session 等需要全部工具的场景。
    pub fn all() -> Self {
        Self {
            tool: ToolDimension {
                enabled_clusters: BTreeSet::from([
                    ToolCluster::Read,
                    ToolCluster::Write,
                    ToolCluster::Execute,
                    ToolCluster::Workflow,
                    ToolCluster::Collaboration,
                    ToolCluster::Canvas,
                ]),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// 检查指定簇是否启用。
    pub fn has(&self, cluster: ToolCluster) -> bool {
        self.tool.enabled_clusters.contains(&cluster)
    }

    /// 检查指定工具是否可用（cluster 启用）。
    ///
    /// 该方法仅保留给没有 capability 维度的旧调用点；新代码应使用
    /// `is_capability_tool_enabled`。
    pub fn is_tool_enabled(&self, tool_name: &str, cluster: ToolCluster) -> bool {
        let _ = tool_name;
        self.has(cluster)
    }

    /// 检查指定 capability 下的工具是否可用。
    pub fn is_capability_tool_enabled(
        &self,
        capability_key: &str,
        tool_name: &str,
        cluster: Option<ToolCluster>,
    ) -> bool {
        if let Some(cluster) = cluster
            && !self.has(cluster)
        {
            return false;
        }

        let capability_granted = self
            .tool
            .capabilities
            .contains(&crate::ToolCapability::new(capability_key));
        if cluster.is_none() && !capability_granted {
            return false;
        }
        if !self.tool.capabilities.is_empty() && !capability_granted {
            return false;
        }

        self.tool
            .tool_policy
            .get(capability_key)
            .is_none_or(|filter| filter.allows(tool_name))
    }

    /// 检查 `<capability>::<tool>` 是否被工具级 directive 屏蔽。
    pub fn is_tool_path_excluded(&self, capability_key: &str, tool_name: &str) -> bool {
        !self.is_capability_tool_enabled(capability_key, tool_name, None)
    }

    /// 以 `<capability>::<tool>` 形式导出当前白名单路径，供事件 diff / UI 展示使用。
    pub fn included_tool_paths(&self) -> BTreeSet<String> {
        self.tool
            .tool_policy
            .iter()
            .flat_map(|(capability, filter)| {
                filter
                    .include_only
                    .iter()
                    .map(move |tool| format!("{capability}::{tool}"))
            })
            .collect()
    }

    /// 以 `<capability>::<tool>` 形式导出当前屏蔽路径，供事件 diff / UI 展示使用。
    pub fn excluded_tool_paths(&self) -> BTreeSet<String> {
        self.tool
            .tool_policy
            .iter()
            .flat_map(|(capability, filter)| {
                filter
                    .exclude
                    .iter()
                    .map(move |tool| format!("{capability}::{tool}"))
            })
            .collect()
    }

    /// 从簇数组构造。
    pub fn from_clusters(clusters: impl IntoIterator<Item = ToolCluster>) -> Self {
        Self {
            tool: ToolDimension {
                enabled_clusters: clusters.into_iter().collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// 已解析能力全集的 string key 视图——供 hook runtime 初始化使用。
    pub fn capability_keys(&self) -> BTreeSet<String> {
        self.tool
            .capabilities
            .iter()
            .map(|c| c.key().to_string())
            .collect()
    }

    /// 与另一个 CapabilityState 做交集（用于 agent 级配置裁剪）。
    pub fn intersect(&self, other: &CapabilityState) -> Self {
        Self {
            tool: ToolDimension {
                capabilities: self
                    .tool
                    .capabilities
                    .intersection(&other.tool.capabilities)
                    .cloned()
                    .collect(),
                enabled_clusters: self
                    .tool
                    .enabled_clusters
                    .intersection(&other.tool.enabled_clusters)
                    .copied()
                    .collect(),
                tool_policy: merge_tool_policy_for_intersection(
                    &self.tool.tool_policy,
                    &other.tool.tool_policy,
                ),
                mcp_servers: self.tool.mcp_servers.clone(),
            },
            companion: self.companion.clone(),
            vfs: self.vfs.clone(),
            // skill 不参与 capability 交集裁剪，保持调用方当前会话可见技能面。
            skill: self.skill.clone(),
        }
    }
}

fn merge_tool_policy_for_intersection(
    left: &BTreeMap<String, ToolCapabilityFilter>,
    right: &BTreeMap<String, ToolCapabilityFilter>,
) -> BTreeMap<String, ToolCapabilityFilter> {
    let mut merged = left.clone();
    for (capability, filter) in right {
        let entry = merged.entry(capability.clone()).or_default();
        if entry.include_only.is_empty() {
            entry.include_only = filter.include_only.clone();
        } else if !filter.include_only.is_empty() {
            entry.include_only = entry
                .include_only
                .intersection(&filter.include_only)
                .cloned()
                .collect();
        }
        entry.exclude.extend(filter.exclude.iter().cloned());
    }
    merged.retain(|_, filter| !filter.is_empty());
    merged
}

// ── Session 级 MCP Server 声明 ─────────────────────────────────

/// Session 级 MCP server — 内部统一类型，通过 `uses_relay` 标记区分直连与中继。
///
/// relay 标记是 server 的内禀属性，不应作为独立的 `HashSet<String>` 旁路传递。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionMcpServer {
    pub name: String,
    pub transport: McpTransportConfig,
    /// 是否通过 relay backend 代理而非云端直连。
    pub uses_relay: bool,
}

// MCP transport 配置统一归 domain，spi 直接复用，避免领域/SPI 两份等价定义漂移。
pub use agentdash_domain::mcp_preset::{McpEnvVar, McpHttpHeader, McpTransportConfig};

impl SessionMcpServer {}

/// 按 relay 标记分组：返回 (relay_server_names, direct_servers)。
pub fn partition_session_mcp_servers(
    servers: &[SessionMcpServer],
) -> (Vec<String>, Vec<SessionMcpServer>) {
    let mut relay_names = Vec::new();
    let mut direct = Vec::new();
    for s in servers {
        if s.uses_relay {
            relay_names.push(s.name.clone());
        } else {
            direct.push(s.clone());
        }
    }
    (relay_names, direct)
}

#[derive(Debug, Clone)]
pub enum PromptPayload {
    Text(String),
    /// canonical 用户输入：贯穿 API -> 应用 -> 连接器 -> AgentMessage 的结构化输入单元。
    /// 取代旧 `Blocks(Vec<ContentBlock>)`，让图片等多模态内容结构化直达模型而不拍平。
    Input(Vec<agentdash_agent_protocol::UserInputBlock>),
}

impl PromptPayload {
    /// 投递路径：把 canonical 输入映射为模型层 `ContentPart`（图片直达 `ContentPart::Image`）。
    /// 连接器统一消费此方法，不再各自拍平。
    pub fn to_content_parts(&self) -> Vec<agentdash_agent_types::ContentPart> {
        match self {
            Self::Text(text) => {
                let text = text.trim();
                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![agentdash_agent_types::ContentPart::text(text)]
                }
            }
            Self::Input(input) => {
                agentdash_agent_protocol::user_input_blocks_to_content_parts(input)
            }
        }
    }

    /// 文本摘要：仅供标题提示 / trace 元信息，**不是**投递路径。
    pub fn to_fallback_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Input(input) => agentdash_agent_protocol::codex_user_input_to_text(input)
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use agentdash_agent_protocol::codex_app_server_protocol as codex;

    use super::*;

    #[test]
    fn prompt_payload_input_to_content_parts_keeps_image_structured() {
        // 投递路径：canonical 输入经唯一映射，图片真出 ContentPart::Image，不再拍平成占位文本。
        let input = vec![
            codex::UserInput::Text {
                text: "请分析这张图".to_string(),
                text_elements: Vec::new(),
            },
            codex::UserInput::Image {
                detail: None,
                url: "data:image/png;base64,AAAA".to_string(),
            },
        ];
        let parts = PromptPayload::Input(input).to_content_parts();
        assert_eq!(parts.len(), 2);
        assert_eq!(
            parts[0],
            agentdash_agent_types::ContentPart::text("请分析这张图")
        );
        assert!(matches!(
            parts[1],
            agentdash_agent_types::ContentPart::Image { .. }
        ));
        assert_eq!(
            parts[1],
            agentdash_agent_types::ContentPart::image("image/png", "AAAA")
        );
    }

    #[test]
    fn prompt_payload_text_to_content_parts() {
        let parts = PromptPayload::Text("  hi  ".to_string()).to_content_parts();
        assert_eq!(parts, vec![agentdash_agent_types::ContentPart::text("hi")]);
    }

    #[test]
    fn capability_tool_filter_excludes_and_whitelists_by_capability_key() {
        let mut flow = CapabilityState::from_clusters([ToolCluster::Read]);
        flow.tool
            .capabilities
            .insert(crate::ToolCapability::new("file_read"));
        flow.tool
            .tool_policy
            .entry("file_read".to_string())
            .or_default()
            .exclude
            .insert("fs_grep".to_string());

        assert!(flow.is_capability_tool_enabled("file_read", "fs_read", Some(ToolCluster::Read)));
        assert!(!flow.is_capability_tool_enabled("file_read", "fs_grep", Some(ToolCluster::Read)));

        let filter = flow
            .tool
            .tool_policy
            .entry("file_read".to_string())
            .or_default();
        filter.include_only.insert("fs_read".to_string());

        assert!(flow.is_capability_tool_enabled("file_read", "fs_read", Some(ToolCluster::Read)));
        assert!(!flow.is_capability_tool_enabled(
            "file_read",
            "mounts_list",
            Some(ToolCluster::Read)
        ));
    }

    #[test]
    fn capability_tool_filter_respects_capabilities_when_present() {
        let mut flow = CapabilityState::default();
        flow.tool
            .capabilities
            .insert(crate::ToolCapability::new("workflow_management"));

        assert!(flow.is_capability_tool_enabled("workflow_management", "get_workflow", None));
        assert!(!flow.is_capability_tool_enabled("story_management", "get_story_context", None));
    }

    #[test]
    fn capability_tool_filter_denies_mcp_tools_when_capability_state_is_empty() {
        let flow = CapabilityState::default();

        assert!(
            !flow.is_capability_tool_enabled("workflow_management", "upsert_workflow_tool", None),
            "MCP 工具没有 cluster 兜底，必须先由 canonical CapabilityState 授予 capability"
        );
    }
}

pub type ExecutionStream = Pin<
    Box<
        dyn Stream<Item = Result<agentdash_agent_protocol::BackboneEnvelope, ConnectorError>>
            + Send
            + 'static,
    >,
>;

/// 运行时工具构建 SPI。
/// 由 application 层持有，executor 层提供具体实现。
#[async_trait]
pub trait RuntimeToolProvider: Send + Sync {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<agentdash_agent_types::DynAgentTool>, ConnectorError>;
}

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("执行器配置无效: {0}")]
    InvalidConfig(String),
    #[error("执行器启动失败: {0}")]
    SpawnFailed(String),
    #[error("执行器运行错误: {0}")]
    Runtime(String),
    #[error("连接失败: {0}")]
    ConnectionFailed(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[async_trait]
pub trait AgentConnector: Send + Sync {
    fn connector_id(&self) -> &'static str;

    fn connector_type(&self) -> ConnectorType;

    fn capabilities(&self) -> ConnectorCapabilities;

    /// 指示给定 executor 是否支持基于 session 仓储的原生消息恢复。
    ///
    /// 当返回 `true` 时，session 生命周期层会在冷启动 continuation 场景下
    /// 传入 `ExecutionContext.restored_session_state`，而不是退化为 continuation 文本。
    fn supports_repository_restore(&self, _executor: &str) -> bool {
        false
    }

    fn list_executors(&self) -> Vec<AgentInfo>;

    async fn discover_options_stream(
        &self,
        executor: &str,
        working_dir: Option<PathBuf>,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError>;

    async fn discover_options_stream_with_context(
        &self,
        executor: &str,
        context: DiscoveryContext,
    ) -> Result<BoxStream<'static, json_patch::Patch>, ConnectorError> {
        self.discover_options_stream(executor, context.working_dir)
            .await
    }

    /// 返回当前进程内该 session 是否仍有可直接续跑的执行器 runtime。
    ///
    /// 这与 session 事件广播或订阅状态不同；仅用于判断是否可以跳过仓储恢复。
    async fn has_live_session(&self, _session_id: &str) -> bool {
        false
    }

    async fn supports_session_steering(&self, session_id: &str) -> bool {
        self.capabilities().supports_steering && self.has_live_session(session_id).await
    }

    async fn prompt(
        &self,
        session_id: &str,
        follow_up_session_id: Option<&str>,
        prompt: &PromptPayload,
        context: ExecutionContext,
    ) -> Result<ExecutionStream, ConnectorError>;

    async fn cancel(&self, session_id: &str) -> Result<(), ConnectorError>;

    /// 向正在运行的 session 注入用户 steer 消息。
    ///
    /// 与 `prompt` 不同，这里不创建新 turn，也不重新进入 launch/claim 流程。
    async fn steer_session(
        &self,
        session_id: &str,
        _expected_turn_id: &str,
        _input: Vec<agentdash_agent_protocol::UserInputBlock>,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::Runtime(format!(
            "connector `{}` 不支持 session `{session_id}` 的 steering",
            self.connector_id()
        )))
    }

    async fn approve_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<(), ConnectorError>;

    async fn reject_tool_call(
        &self,
        session_id: &str,
        tool_call_id: &str,
        reason: Option<String>,
    ) -> Result<(), ConnectorError>;

    /// Phase Node 切换时热更新 session 的工具集。
    /// `tools` 为 application 层预构建好的完整工具列表（runtime + MCP），
    /// connector 直接 replace-set 到运行中 agent。
    /// 默认 no-op — 仅 PiAgentConnector 等 in-process connector 需要实现。
    async fn update_session_tools(
        &self,
        _session_id: &str,
        _tools: Vec<agentdash_agent_types::DynAgentTool>,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }

    /// 向活跃 session 注入一条用户消息（用于能力变更等 out-of-band 通知）。
    ///
    /// 与 `prompt` 不同，这里只是把消息塞进 steering 队列，
    /// 下一次 LLM 调用前会被自动合并到对话末尾，保持 KV cache 前缀稳定。
    /// 默认 no-op — 仅 in-process connector（如 PiAgent）需要实现。
    async fn push_session_notification(
        &self,
        session_id: &str,
        _message: String,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::Runtime(format!(
            "connector `{}` 不支持 session `{session_id}` 的 steering notification",
            self.connector_id()
        )))
    }
}
