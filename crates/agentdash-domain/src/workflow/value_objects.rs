use std::fmt;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session_binding::{ChildSessionId, SessionOwnerType};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// Workflow 可挂载到哪一类 owner。
/// 这里只描述绑定范围，不表达 workflow 自身的业务主语。
///
/// **Model C 收敛（2026-04-27）**：原先的 `Task` 变体已被移除——Task 不再作为独立
/// aggregate，而是 Story aggregate 下的 child entity；task-scope lifecycle
/// definition 统一归到 Story binding。详见
/// `.trellis/spec/backend/story-task-runtime.md`。
///
/// 注意：`SessionOwnerType::Task` 仍然存在（session binding 的 owner 坐标系
/// 不受影响），但当需要把它映射到 `WorkflowBindingKind` 时，会落到 `Story`。
pub enum WorkflowBindingKind {
    Project,
    Story,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// Workflow 建议由哪一类 owner/session 使用。
/// 它是绑定层提示，不是 workflow 内建业务角色。
///
/// 与 `WorkflowBindingKind` 1:1 对应；同步收敛为 `Project / Story`。
pub enum WorkflowBindingRole {
    Project,
    Story,
}

impl WorkflowBindingKind {
    pub fn binding_scope_key(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Story => "story",
        }
    }

    pub fn from_binding_scope(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "project" => Some(Self::Project),
            "story" => Some(Self::Story),
            _ => None,
        }
    }

    pub fn from_owner_type(raw: &str) -> Option<Self> {
        Self::from_binding_scope(raw)
    }
}

impl WorkflowBindingRole {
    pub fn binding_scope_key(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Story => "story",
        }
    }
}

impl From<SessionOwnerType> for WorkflowBindingKind {
    /// 将 session owner 类型映射为 workflow binding kind。
    ///
    /// **Model C 决策**：`SessionOwnerType::Task` 映射到 `WorkflowBindingKind::Story`。
    /// 理由：Task 所属的 Story 是 binding 定义的自然归属；task 级的 lifecycle
    /// 统一由 Story-bound lifecycle 承载（一个 Story 下每个 task 激活其对应的
    /// step）。这里会丢掉 task_id 信息——上层若需要区分 task，必须通过
    /// `SessionOwnerCtx::Task { story_id, task_id, .. }` 单独保留，而不是依赖
    /// `WorkflowBindingKind`。
    fn from(value: SessionOwnerType) -> Self {
        match value {
            SessionOwnerType::Project => Self::Project,
            SessionOwnerType::Story | SessionOwnerType::Task => Self::Story,
        }
    }
}

impl From<WorkflowBindingKind> for WorkflowBindingRole {
    fn from(value: WorkflowBindingKind) -> Self {
        match value {
            WorkflowBindingKind::Project => Self::Project,
            WorkflowBindingKind::Story => Self::Story,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowDefinitionSource {
    BuiltinSeed,
    UserAuthored,
    Cloned,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub field_path: String,
    pub severity: ValidationSeverity,
}

impl ValidationIssue {
    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        field_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field_path: field_path.into(),
            severity: ValidationSeverity::Error,
        }
    }

    pub fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        field_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field_path: field_path.into(),
            severity: ValidationSeverity::Warning,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct WorkflowContextBinding {
    pub locator: String,
    pub reason: String,
    #[serde(default = "bool_true")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, Default)]
pub struct WorkflowInjectionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default)]
    pub instructions: Vec<String>,
    #[serde(default)]
    pub context_bindings: Vec<WorkflowContextBinding>,
}

/// Standalone 场景下 input port 的满足策略。
///
/// Lifecycle 内运行时由 edge wire 自动满足；standalone（如主 agent 给子 agent
/// 分配 workflow）时由此字段指示调用方如何提供输入。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StandaloneFulfillment {
    /// 调用方必须在启动前通过 `lifecycle://artifacts/{key}` 写入
    Required,
    /// 可选输入，未提供时使用 default_value
    Optional {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_value: Option<String>,
    },
}

impl Default for StandaloneFulfillment {
    fn default() -> Self {
        Self::Required
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowHookTrigger {
    UserPromptSubmit,
    BeforeTool,
    AfterTool,
    AfterTurn,
    BeforeStop,
    SessionTerminal,
    BeforeSubagentDispatch,
    AfterSubagentDispatch,
    SubagentResult,
    BeforeCompact,
    AfterCompact,
    BeforeProviderRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct WorkflowHookRuleSpec {
    pub key: String,
    pub trigger: WorkflowHookTrigger,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(default = "bool_true")]
    pub enabled: bool,
}

// ── Capability 路径 ──

/// Capability 路径 — 统一表达「能力级」和「工具级」两种寻址。
///
/// `capability` 是 capability key（如 `"file_read"` 或 `"mcp:code_analyzer"`）；
/// `tool` 为 `None` 表示短 path（整个能力），`Some(name)` 表示长 path（能力下的某个工具）。
///
/// 分隔符统一为 `::`（与 Rust 模块路径同构），与 `mcp:<server>` 的单冒号前缀不冲突。
/// MCP server name 禁止含 `::`，由 preset 校验层强制。
///
/// JSON 形式序列化为 qualified string：`"file_read"` / `"file_read::fs_grep"`
/// / `"mcp:code_analyzer"` / `"mcp:workflow_management::upsert"`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, JsonSchema)]
pub struct CapabilityPath {
    pub capability: String,
    pub tool: Option<String>,
}

const CAPABILITY_PATH_SEPARATOR: &str = "::";

impl CapabilityPath {
    /// 构造能力级短 path。
    pub fn of_capability(key: impl Into<String>) -> Self {
        Self {
            capability: key.into(),
            tool: None,
        }
    }

    /// 构造工具级长 path。
    pub fn of_tool(cap: impl Into<String>, tool: impl Into<String>) -> Self {
        Self {
            capability: cap.into(),
            tool: Some(tool.into()),
        }
    }

    /// 是否为工具级 path。
    pub fn is_tool_level(&self) -> bool {
        self.tool.is_some()
    }

    /// 序列化为 qualified string —— `"cap"` 或 `"cap::tool"`。
    pub fn to_qualified_string(&self) -> String {
        match &self.tool {
            None => self.capability.clone(),
            Some(tool) => format!("{}{CAPABILITY_PATH_SEPARATOR}{tool}", self.capability),
        }
    }

    /// 解析 qualified string —— 反向对应 `to_qualified_string`。
    ///
    /// 规则：
    /// - 空字符串 → Err
    /// - 恰好一个 `::` → Ok(long path)；两边均不得为空
    /// - 多于一个 `::` → Err（不允许多级嵌套）
    /// - 无 `::` → Ok(short path)
    pub fn parse(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err("CapabilityPath 不能为空".to_string());
        }

        // 统计 `::` 出现次数（按字符位置扫描，避免误处理单 `:` 前缀）
        let parts: Vec<&str> = trimmed.split(CAPABILITY_PATH_SEPARATOR).collect();
        match parts.len() {
            1 => Ok(Self {
                capability: parts[0].to_string(),
                tool: None,
            }),
            2 => {
                let cap = parts[0];
                let tool = parts[1];
                if cap.is_empty() {
                    return Err(format!("CapabilityPath `{s}` 缺少 capability 段"));
                }
                if tool.is_empty() {
                    return Err(format!("CapabilityPath `{s}` 缺少 tool 段"));
                }
                Ok(Self {
                    capability: cap.to_string(),
                    tool: Some(tool.to_string()),
                })
            }
            _ => Err(format!(
                "CapabilityPath `{s}` 包含多个 `::` 分隔符，仅允许一级工具寻址"
            )),
        }
    }
}

impl fmt::Display for CapabilityPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_qualified_string())
    }
}

impl Serialize for CapabilityPath {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_qualified_string())
    }
}

impl<'de> Deserialize<'de> for CapabilityPath {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, Default)]
pub struct WorkflowContract {
    #[serde(default)]
    pub injection: WorkflowInjectionSpec,
    #[serde(default)]
    pub hook_rules: Vec<WorkflowHookRuleSpec>,
    /// Workflow 产出声明 — 同时作为完成条件：port gate 门禁根据 `gate_strategy` 检查交付。
    ///
    /// Lifecycle step 绑定 workflow 时自动继承这些 ports 作为默认值，step 编辑器可 override。
    #[serde(default, alias = "recommended_output_ports", skip_serializing_if = "Vec::is_empty")]
    pub output_ports: Vec<OutputPortDefinition>,
    /// Workflow 输入声明 — 同时作为运行约束：lifecycle 内由 edge wire 满足，standalone 由调用方写入。
    #[serde(default, alias = "recommended_input_ports", skip_serializing_if = "Vec::is_empty")]
    pub input_ports: Vec<InputPortDefinition>,
    /// Workflow 级能力指令序列。
    ///
    /// 每条指令在 agent baseline 上应用 Add / Remove：
    /// - `Add(path)` — 追加能力（短 path）或启用某个工具（长 path）
    /// - `Remove(path)` — 屏蔽能力（短 path）或屏蔽某个工具（长 path）
    ///
    /// 运行时 hook 可叠加 delta 指令；`compute_effective_capabilities` 走同一条归约路径。
    ///
    /// 序列化示例：
    /// ```json
    /// [{"add": "workflow_management"}, {"remove": "shell_execute"}, {"add": "file_read::fs_read"}]
    /// ```
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_directives: Vec<CapabilityDirective>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSessionTerminalState {
    Completed,
    Failed,
    Interrupted,
}

/// Lifecycle node 类型：Agent Node 创建独立 session，Phase Node 在前一个 session 内切换 contract
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleNodeType {
    /// 创建独立 agent session 执行工作
    AgentNode,
    /// 不创建新 session，在前一个 session 内切换 workflow contract
    PhaseNode,
}

impl Default for LifecycleNodeType {
    fn default() -> Self {
        Self::PhaseNode
    }
}

/// 门禁策略：定义 output port 交付检查的严格程度。
/// 实际检查逻辑由对应的 Rhai Hook Preset 实现。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateStrategy {
    Existence,
    Schema,
    LlmJudge,
}

impl Default for GateStrategy {
    fn default() -> Self {
        Self::Existence
    }
}

/// Input port 上下文构建策略：控制前驱 output artifact 如何注入后继 session。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    Full,
    Summary,
    MetadataOnly,
    Custom,
}

impl Default for ContextStrategy {
    fn default() -> Self {
        Self::Full
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct OutputPortDefinition {
    pub key: String,
    pub description: String,
    #[serde(default)]
    pub gate_strategy: GateStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct InputPortDefinition {
    pub key: String,
    pub description: String,
    #[serde(default)]
    pub context_strategy: ContextStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_template: Option<String>,
    /// Standalone 运行时（非 lifecycle edge wire）如何满足此 input port。
    #[serde(default)]
    pub standalone_fulfillment: StandaloneFulfillment,
}

/// 运行时能力指令 —— 在 agent baseline 上执行 Add/Remove。
///
/// `Add(path)` 追加能力或启用工具，`Remove(path)` 屏蔽能力或屏蔽工具。
/// `path` 为短 path 表示能力级操作；长 path 表示工具级操作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDirective {
    Add(CapabilityPath),
    Remove(CapabilityPath),
}

impl CapabilityDirective {
    /// 返回指令操作的 capability key（无论工具级还是能力级）。
    pub fn key(&self) -> &str {
        match self {
            Self::Add(path) | Self::Remove(path) => &path.capability,
        }
    }

    /// 返回指令携带的 path 引用。
    pub fn path(&self) -> &CapabilityPath {
        match self {
            Self::Add(p) | Self::Remove(p) => p,
        }
    }

    pub fn is_add(&self) -> bool {
        matches!(self, Self::Add(_))
    }

    pub fn is_remove(&self) -> bool {
        matches!(self, Self::Remove(_))
    }

    /// 快捷构造：能力级 Add 指令（短 path）。
    pub fn add_simple(key: impl Into<String>) -> Self {
        Self::Add(CapabilityPath::of_capability(key))
    }

    /// 快捷构造：能力级 Remove 指令（短 path）。
    pub fn remove_simple(key: impl Into<String>) -> Self {
        Self::Remove(CapabilityPath::of_capability(key))
    }

    /// 快捷构造：工具级 Add 指令（长 path）。
    pub fn add_tool(cap: impl Into<String>, tool: impl Into<String>) -> Self {
        Self::Add(CapabilityPath::of_tool(cap, tool))
    }

    /// 快捷构造：工具级 Remove 指令（长 path）。
    pub fn remove_tool(cap: impl Into<String>, tool: impl Into<String>) -> Self {
        Self::Remove(CapabilityPath::of_tool(cap, tool))
    }
}

/// 能力归约状态机 slot —— 对单个 capability key 在一串 directive 后的最终状态。
///
/// 状态转移表：见 `compute_effective_capabilities` 实现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilitySlotState {
    /// 未被任何 directive 命中（对 auto_granted 能力仍可见）。
    NotDeclared,
    /// 命中过 `Add(cap, None)` —— 启用该 capability 全集。
    FullCapability,
    /// 仅命中过工具级 Add —— 只启用白名单内的工具。
    ToolWhitelist(std::collections::BTreeSet<String>),
    /// 最近命中 `Remove(cap, None)` —— 屏蔽整个能力。
    Blocked,
}

/// 归约结果：按 capability key 汇总 slot 状态 + 工具级排除集合。
#[derive(Debug, Clone, Default)]
pub struct CapabilityReduction {
    pub slots: std::collections::BTreeMap<String, CapabilitySlotState>,
    pub excluded_tools: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
}

/// 在一串 directive 上执行 slot 规则归约，产出 `CapabilityReduction`。
///
/// 规则详见 `.trellis/spec/backend/capability/tool-capability-pipeline.md`
/// 「Slot 归约规则」章节。核心要点：
///
/// - `Add(cap, None)` → `FullCapability`（并清空 whitelist，若原先是 whitelist 状态则同义升级）
/// - `Add(cap, Some(t))` → `ToolWhitelist{t+}`（对 `FullCapability` 是 no-op）
/// - `Remove(cap, None)` → `Blocked`
/// - `Remove(cap, Some(t))` → 写入 `excluded_tools[cap] += t`，并在白名单中移除 t
///
/// 后来者胜 —— 每条指令按序执行。
pub fn reduce_capability_directives(directives: &[CapabilityDirective]) -> CapabilityReduction {
    use std::collections::{BTreeMap, BTreeSet};

    let mut slots: BTreeMap<String, CapabilitySlotState> = BTreeMap::new();
    let mut excluded_tools: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for directive in directives {
        match directive {
            CapabilityDirective::Add(path) => {
                let key = path.capability.clone();
                match &path.tool {
                    None => {
                        slots.insert(key, CapabilitySlotState::FullCapability);
                    }
                    Some(tool) => {
                        let entry = slots
                            .entry(key.clone())
                            .or_insert(CapabilitySlotState::NotDeclared);
                        match entry {
                            CapabilitySlotState::FullCapability => {
                                // no-op：全集已启用
                            }
                            CapabilitySlotState::ToolWhitelist(set) => {
                                set.insert(tool.clone());
                            }
                            CapabilitySlotState::NotDeclared | CapabilitySlotState::Blocked => {
                                let mut set = BTreeSet::new();
                                set.insert(tool.clone());
                                *entry = CapabilitySlotState::ToolWhitelist(set);
                            }
                        }
                    }
                }
            }
            CapabilityDirective::Remove(path) => {
                let key = path.capability.clone();
                match &path.tool {
                    None => {
                        slots.insert(key, CapabilitySlotState::Blocked);
                    }
                    Some(tool) => {
                        // 从白名单中移除（若存在），同时写入 excluded_tools
                        if let Some(CapabilitySlotState::ToolWhitelist(set)) = slots.get_mut(&key) {
                            set.remove(tool);
                        }
                        excluded_tools.entry(key).or_default().insert(tool.clone());
                    }
                }
            }
        }
    }

    CapabilityReduction {
        slots,
        excluded_tools,
    }
}

/// Lifecycle edge 类别：控制流 vs 数据流。
///
/// - `Flow`：无数据语义的顺序约束（前驱完成即激活后继）。
/// - `Artifact`：端口级数据依赖；自动蕴含 Flow 约束（B 消费 A.port → B dep A）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleEdgeKind {
    Flow,
    Artifact,
}

fn default_edge_kind() -> LifecycleEdgeKind {
    // 既有持久化数据无 kind 字段时统一视为 artifact（历史边全部带 port）
    LifecycleEdgeKind::Artifact
}

/// Lifecycle DAG 边——控制流 + 数据流的统一承载。
///
/// `kind = Flow` 时 `from_port` / `to_port` 必须为 `None`；
/// `kind = Artifact` 时两者必须为 `Some`。
/// node 级别依赖通过 `node_deps_from_edges()` 从 flow/artifact 两类边统一计算。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct LifecycleEdge {
    #[serde(default = "default_edge_kind")]
    pub kind: LifecycleEdgeKind,
    pub from_node: String,
    pub to_node: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_port: Option<String>,
}

impl LifecycleEdge {
    /// 构造控制流边：仅表达顺序约束，无 port。
    pub fn flow(from_node: impl Into<String>, to_node: impl Into<String>) -> Self {
        Self {
            kind: LifecycleEdgeKind::Flow,
            from_node: from_node.into(),
            to_node: to_node.into(),
            from_port: None,
            to_port: None,
        }
    }

    /// 构造 artifact 边：端口级数据依赖；隐含 flow 约束。
    pub fn artifact(
        from_node: impl Into<String>,
        from_port: impl Into<String>,
        to_node: impl Into<String>,
        to_port: impl Into<String>,
    ) -> Self {
        Self {
            kind: LifecycleEdgeKind::Artifact,
            from_node: from_node.into(),
            to_node: to_node.into(),
            from_port: Some(from_port.into()),
            to_port: Some(to_port.into()),
        }
    }

    pub fn is_flow(&self) -> bool {
        matches!(self.kind, LifecycleEdgeKind::Flow)
    }

    pub fn is_artifact(&self) -> bool {
        matches!(self.kind, LifecycleEdgeKind::Artifact)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct LifecycleStepDefinition {
    pub key: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_key: Option<String>,
    #[serde(default)]
    pub node_type: LifecycleNodeType,
    /// Step 级产出约束：该节点必须交付的 artifacts
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_ports: Vec<OutputPortDefinition>,
    /// Step 级消费声明：该节点从前驱接收的 artifacts
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_ports: Vec<InputPortDefinition>,
}

impl LifecycleStepDefinition {
    /// 返回修剪后的 workflow_key（去空白、过滤空串）。
    pub fn effective_workflow_key(&self) -> Option<&str> {
        self.workflow_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleRunStatus {
    Draft,
    Ready,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStepExecutionStatus {
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleStepState {
    pub step_key: String,
    pub status: LifecycleStepExecutionStatus,
    /// 若该 step 开启独立 child session（AgentNode 语义），绑定在此。
    ///
    /// Model C 下 step 子会话是挂在 Story root session 下的派生会话，参见
    /// `.trellis/spec/backend/story-task-runtime.md` §2.5。物理上仍是会话
    /// 字符串 ID；此处以 [`ChildSessionId`] 别名明确"这不是 Story root"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<ChildSessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<Value>,
    #[serde(default)]
    pub gate_collision_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleExecutionEventKind {
    StepActivated,
    StepCompleted,
    ConstraintBlocked,
    CompletionEvaluated,
    ArtifactAppended,
    ContextInjected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleExecutionEntry {
    pub timestamp: DateTime<Utc>,
    pub step_key: String,
    pub event_kind: LifecycleExecutionEventKind,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EffectiveSessionContract {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_step_key: Option<String>,
    #[serde(default)]
    pub injection: WorkflowInjectionSpec,
    #[serde(default)]
    pub hook_rules: Vec<WorkflowHookRuleSpec>,
}

pub fn validate_workflow_definition(
    key: &str,
    name: &str,
    contract: &WorkflowContract,
) -> Result<(), String> {
    validate_identity("workflow.key", key)?;
    validate_non_empty("workflow.name", name)?;
    validate_contract(contract, "workflow.contract")
}

pub fn validate_lifecycle_definition(
    key: &str,
    name: &str,
    entry_step_key: &str,
    steps: &[LifecycleStepDefinition],
    edges: &[LifecycleEdge],
) -> Result<(), String> {
    validate_identity("lifecycle.key", key)?;
    validate_non_empty("lifecycle.name", name)?;
    validate_identity("lifecycle.entry_step_key", entry_step_key)?;
    if steps.is_empty() {
        return Err("lifecycle.steps 至少需要一个 step".to_string());
    }

    let mut seen_step_keys = std::collections::BTreeSet::new();
    for (index, step) in steps.iter().enumerate() {
        validate_identity(&format!("lifecycle.steps[{index}].key"), &step.key)?;
        if let Some(workflow_key) = step
            .workflow_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            validate_identity(
                &format!("lifecycle.steps[{index}].workflow_key"),
                workflow_key,
            )?;
        }
        if !seen_step_keys.insert(step.key.clone()) {
            return Err(format!("lifecycle.steps[{index}].key 重复: {}", step.key));
        }
    }

    if !steps.iter().any(|step| step.key == entry_step_key) {
        return Err(format!(
            "lifecycle.entry_step_key `{entry_step_key}` 未出现在 lifecycle.steps 中"
        ));
    }

    // 多 step lifecycle 必须显式声明 edges——禁止 fallback 依赖
    if steps.len() >= 2 && edges.is_empty() {
        return Err(
            "lifecycle.edges 不能为空：多 step lifecycle 必须显式声明 flow / artifact edges"
                .to_string(),
        );
    }

    if !edges.is_empty() {
        validate_edge_topology(steps, edges, entry_step_key)?;
    }

    Ok(())
}

fn validate_contract(contract: &WorkflowContract, field_path: &str) -> Result<(), String> {
    for (index, binding) in contract.injection.context_bindings.iter().enumerate() {
        validate_non_empty(
            &format!("{field_path}.injection.context_bindings[{index}].locator"),
            &binding.locator,
        )?;
        validate_non_empty(
            &format!("{field_path}.injection.context_bindings[{index}].reason"),
            &binding.reason,
        )?;
    }

    let mut seen_rule_keys = std::collections::BTreeSet::new();
    for (index, rule) in contract.hook_rules.iter().enumerate() {
        validate_identity(&format!("{field_path}.hook_rules[{index}].key"), &rule.key)?;
        if rule.preset.is_none() && rule.script.is_none() {
            return Err(format!(
                "{field_path}.hook_rules[{index}] 必须指定 preset 或 script"
            ));
        }
        if !seen_rule_keys.insert(rule.key.clone()) {
            return Err(format!(
                "{field_path}.hook_rules[{index}].key 重复: {}",
                rule.key
            ));
        }
    }

    let mut seen_output_port_keys = std::collections::BTreeSet::new();
    for (index, port) in contract.output_ports.iter().enumerate() {
        validate_identity(
            &format!("{field_path}.output_ports[{index}].key"),
            &port.key,
        )?;
        validate_non_empty(
            &format!("{field_path}.output_ports[{index}].description"),
            &port.description,
        )?;
        if !seen_output_port_keys.insert(port.key.clone()) {
            return Err(format!(
                "{field_path}.output_ports[{index}].key 重复: {}",
                port.key
            ));
        }
    }

    let mut seen_input_port_keys = std::collections::BTreeSet::new();
    for (index, port) in contract.input_ports.iter().enumerate() {
        validate_identity(
            &format!("{field_path}.input_ports[{index}].key"),
            &port.key,
        )?;
        validate_non_empty(
            &format!("{field_path}.input_ports[{index}].description"),
            &port.description,
        )?;
        if !seen_input_port_keys.insert(port.key.clone()) {
            return Err(format!(
                "{field_path}.input_ports[{index}].key 重复: {}",
                port.key
            ));
        }
    }

    Ok(())
}

/// 从 port-level edges 计算 node 级别依赖关系。
/// 返回 `{ to_node -> Set<from_node> }`，多条连同一对 node 的 edge 自动去重。
pub fn node_deps_from_edges(
    edges: &[LifecycleEdge],
) -> std::collections::HashMap<&str, std::collections::BTreeSet<&str>> {
    let mut deps: std::collections::HashMap<&str, std::collections::BTreeSet<&str>> =
        std::collections::HashMap::new();
    for edge in edges {
        deps.entry(edge.to_node.as_str())
            .or_default()
            .insert(edge.from_node.as_str());
    }
    deps
}

/// Edge-based DAG 拓扑校验：
/// 1. edge 引用的 node key 必须存在于 steps 中
/// 2. 不允许自连接
/// 3. kind 感知 port 约束：Flow 不可带 port；Artifact 必须带 port
/// 4. entry node 不应有入边
/// 5. 不得存在孤岛 step（无入边也无出边）——单 step lifecycle 除外
/// 6. 不得存在循环依赖（Kahn's algorithm，node 级别去重）
fn validate_edge_topology(
    steps: &[LifecycleStepDefinition],
    edges: &[LifecycleEdge],
    entry_step_key: &str,
) -> Result<(), String> {
    let step_keys: std::collections::BTreeSet<&str> =
        steps.iter().map(|s| s.key.as_str()).collect();

    for (i, edge) in edges.iter().enumerate() {
        if !step_keys.contains(edge.from_node.as_str()) {
            return Err(format!(
                "lifecycle.edges[{i}].from_node 引用了不存在的 step: {}",
                edge.from_node
            ));
        }
        if !step_keys.contains(edge.to_node.as_str()) {
            return Err(format!(
                "lifecycle.edges[{i}].to_node 引用了不存在的 step: {}",
                edge.to_node
            ));
        }
        if edge.from_node == edge.to_node {
            return Err(format!(
                "lifecycle.edges[{i}] 不能自连接: {}",
                edge.from_node
            ));
        }
        match edge.kind {
            LifecycleEdgeKind::Flow => {
                if edge.from_port.is_some() || edge.to_port.is_some() {
                    return Err(format!("lifecycle.edges[{i}] kind=flow 不应携带 port"));
                }
            }
            LifecycleEdgeKind::Artifact => {
                if edge.from_port.is_none() || edge.to_port.is_none() {
                    return Err(format!(
                        "lifecycle.edges[{i}] kind=artifact 必须同时声明 from_port 与 to_port"
                    ));
                }
            }
        }
    }

    let node_deps = node_deps_from_edges(edges);

    if node_deps.contains_key(entry_step_key) {
        return Err(format!("entry_step_key `{entry_step_key}` 不应有入边"));
    }

    // 禁止孤岛 step（既无入边也无出边）——单 step lifecycle 除外
    if steps.len() > 1 {
        let mut touched: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for edge in edges {
            touched.insert(edge.from_node.as_str());
            touched.insert(edge.to_node.as_str());
        }
        for step in steps {
            if !touched.contains(step.key.as_str()) {
                return Err(format!(
                    "lifecycle.steps `{}` 是孤岛（无入边也无出边）",
                    step.key
                ));
            }
        }
    }

    // Kahn's algorithm — node 级别
    let mut in_degree: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut adj: std::collections::HashMap<&str, std::collections::BTreeSet<&str>> =
        std::collections::HashMap::new();
    for step in steps {
        in_degree.entry(step.key.as_str()).or_insert(0);
    }
    for (to_node, from_nodes) in &node_deps {
        *in_degree.entry(to_node).or_insert(0) += from_nodes.len();
        for from_node in from_nodes {
            adj.entry(from_node).or_default().insert(to_node);
        }
    }

    let mut queue: std::collections::VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(&k, _)| k)
        .collect();
    let mut visited = 0usize;

    while let Some(node) = queue.pop_front() {
        visited += 1;
        if let Some(successors) = adj.get(node) {
            for &succ in successors {
                if let Some(deg) = in_degree.get_mut(succ) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(succ);
                    }
                }
            }
        }
    }

    if visited != steps.len() {
        return Err("lifecycle DAG 存在循环依赖".to_string());
    }

    Ok(())
}

fn validate_identity(field: &str, value: &str) -> Result<(), String> {
    validate_non_empty(field, value)?;
    if value.chars().any(char::is_whitespace) {
        return Err(format!("{field} 不能包含空白字符"));
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} 不能为空"));
    }
    Ok(())
}

fn bool_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_contract() -> WorkflowContract {
        WorkflowContract {
            injection: WorkflowInjectionSpec {
                instructions: vec!["read spec first".to_string()],
                context_bindings: vec![WorkflowContextBinding {
                    locator: ".trellis/workflow.md".to_string(),
                    reason: "workflow".to_string(),
                    required: true,
                    title: None,
                }],
                ..WorkflowInjectionSpec::default()
            },
            ..WorkflowContract::default()
        }
    }

    #[test]
    fn validate_workflow_definition_rejects_duplicate_output_port_keys() {
        let mut contract = sample_contract();
        contract.output_ports = vec![
            OutputPortDefinition {
                key: "a".to_string(),
                description: "x".to_string(),
                gate_strategy: GateStrategy::Existence,
                gate_params: None,
            },
            OutputPortDefinition {
                key: "a".to_string(),
                description: "y".to_string(),
                gate_strategy: GateStrategy::Existence,
                gate_params: None,
            },
        ];

        let error = validate_workflow_definition("wf", "Workflow", &contract).expect_err("fail");
        assert!(error.contains("重复"));
    }

    #[test]
    fn validate_lifecycle_definition_requires_entry_step() {
        let steps = vec![LifecycleStepDefinition {
            key: "start".to_string(),
            description: String::new(),
            workflow_key: Some("wf_start".to_string()),
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
        }];

        let error = validate_lifecycle_definition("lc", "Lifecycle", "missing", &steps, &[])
            .expect_err("fail");
        assert!(error.contains("entry_step_key"));
    }

    #[test]
    fn validate_edge_topology_detects_cycle() {
        let steps = vec![
            LifecycleStepDefinition {
                key: "a".to_string(),
                description: String::new(),
                workflow_key: None,
                node_type: Default::default(),
                output_ports: vec![],
                input_ports: vec![],
            },
            LifecycleStepDefinition {
                key: "b".to_string(),
                description: String::new(),
                workflow_key: None,
                node_type: Default::default(),
                output_ports: vec![],
                input_ports: vec![],
            },
            LifecycleStepDefinition {
                key: "c".to_string(),
                description: String::new(),
                workflow_key: None,
                node_type: Default::default(),
                output_ports: vec![],
                input_ports: vec![],
            },
        ];
        // a → b → c → b（b-c 形成环，a 是入口无入边）
        let edges = vec![
            LifecycleEdge::artifact("a", "out", "b", "in"),
            LifecycleEdge::artifact("b", "out", "c", "in"),
            LifecycleEdge::artifact("c", "out", "b", "in2"),
        ];
        let err = validate_lifecycle_definition("lc", "Lifecycle", "a", &steps, &edges)
            .expect_err("should detect cycle");
        assert!(err.contains("循环"));
    }

    #[test]
    fn validate_edge_topology_rejects_entry_with_incoming() {
        let steps = vec![
            LifecycleStepDefinition {
                key: "a".to_string(),
                description: String::new(),
                workflow_key: None,
                node_type: Default::default(),
                output_ports: vec![],
                input_ports: vec![],
            },
            LifecycleStepDefinition {
                key: "b".to_string(),
                description: String::new(),
                workflow_key: None,
                node_type: Default::default(),
                output_ports: vec![],
                input_ports: vec![],
            },
        ];
        let edges = vec![LifecycleEdge::artifact("b", "out", "a", "in")];
        let err = validate_lifecycle_definition("lc", "Lifecycle", "a", &steps, &edges)
            .expect_err("entry should not have incoming");
        assert!(err.contains("入边"));
    }

    fn simple_step(key: &str) -> LifecycleStepDefinition {
        LifecycleStepDefinition {
            key: key.to_string(),
            description: String::new(),
            workflow_key: None,
            node_type: Default::default(),
            output_ports: vec![],
            input_ports: vec![],
        }
    }

    #[test]
    fn validate_rejects_multi_step_without_edges() {
        let steps = vec![simple_step("a"), simple_step("b")];
        let err = validate_lifecycle_definition("lc", "Lifecycle", "a", &steps, &[])
            .expect_err("multi-step without edges must fail");
        assert!(err.contains("lifecycle.edges 不能为空"));
    }

    #[test]
    fn validate_accepts_single_step_without_edges() {
        let steps = vec![simple_step("solo")];
        validate_lifecycle_definition("lc", "Lifecycle", "solo", &steps, &[])
            .expect("single-step without edges should pass");
    }

    #[test]
    fn validate_rejects_flow_edge_with_port() {
        let steps = vec![simple_step("a"), simple_step("b")];
        let edges = vec![LifecycleEdge {
            kind: LifecycleEdgeKind::Flow,
            from_node: "a".into(),
            to_node: "b".into(),
            from_port: Some("out".into()),
            to_port: None,
        }];
        let err = validate_lifecycle_definition("lc", "Lifecycle", "a", &steps, &edges)
            .expect_err("flow edge must not carry port");
        assert!(err.contains("kind=flow"));
    }

    #[test]
    fn validate_rejects_artifact_edge_without_port() {
        let steps = vec![simple_step("a"), simple_step("b")];
        let edges = vec![LifecycleEdge {
            kind: LifecycleEdgeKind::Artifact,
            from_node: "a".into(),
            to_node: "b".into(),
            from_port: None,
            to_port: None,
        }];
        let err = validate_lifecycle_definition("lc", "Lifecycle", "a", &steps, &edges)
            .expect_err("artifact edge must have both ports");
        assert!(err.contains("kind=artifact"));
    }

    #[test]
    fn validate_rejects_island_step() {
        let steps = vec![simple_step("a"), simple_step("b"), simple_step("c")];
        // 只连 a → b，c 是孤岛
        let edges = vec![LifecycleEdge::flow("a", "b")];
        let err = validate_lifecycle_definition("lc", "Lifecycle", "a", &steps, &edges)
            .expect_err("island step must be rejected");
        assert!(err.contains("孤岛"));
    }

    #[test]
    fn validate_accepts_pure_flow_edges() {
        let steps = vec![simple_step("a"), simple_step("b"), simple_step("c")];
        let edges = vec![LifecycleEdge::flow("a", "b"), LifecycleEdge::flow("b", "c")];
        validate_lifecycle_definition("lc", "Lifecycle", "a", &steps, &edges)
            .expect("pure flow lifecycle should pass");
    }

    #[test]
    fn lifecycle_edge_deserializes_without_kind_as_artifact() {
        // 历史持久化数据无 kind 字段，应兼容反序列化为 Artifact
        let json = r#"{"from_node":"a","from_port":"out","to_node":"b","to_port":"in"}"#;
        let edge: LifecycleEdge = serde_json::from_str(json).expect("deserialize legacy edge");
        assert_eq!(edge.kind, LifecycleEdgeKind::Artifact);
        assert_eq!(edge.from_port.as_deref(), Some("out"));
        assert_eq!(edge.to_port.as_deref(), Some("in"));
    }
    #[test]
    fn workflow_binding_kind_from_owner_type_uses_binding_scope() {
        assert_eq!(
            WorkflowBindingKind::from_owner_type(" story "),
            Some(WorkflowBindingKind::Story)
        );
        assert_eq!(
            WorkflowBindingKind::from_binding_scope("project"),
            Some(WorkflowBindingKind::Project)
        );
        // Model C 收敛：binding_kind 不再接受 "task"
        assert_eq!(WorkflowBindingKind::from_owner_type("task"), None);
        assert_eq!(WorkflowBindingKind::from_owner_type("session"), None);
    }

    #[test]
    fn workflow_binding_scope_conversions_stay_consistent() {
        assert_eq!(
            WorkflowBindingKind::from(SessionOwnerType::Project),
            WorkflowBindingKind::Project
        );
        assert_eq!(
            WorkflowBindingRole::from(WorkflowBindingKind::Story).binding_scope_key(),
            "story"
        );
        assert_eq!(WorkflowBindingKind::Project.binding_scope_key(), "project");
        assert_eq!(WorkflowBindingKind::Story.binding_scope_key(), "story");
    }

    #[test]
    fn workflow_binding_kind_from_session_owner_task_maps_to_story() {
        // Model C: SessionOwnerType::Task 映射到 WorkflowBindingKind::Story
        // 因为 task 级 lifecycle 统一由 Story-bound lifecycle 承载
        assert_eq!(
            WorkflowBindingKind::from(SessionOwnerType::Task),
            WorkflowBindingKind::Story
        );
        assert_eq!(
            WorkflowBindingKind::from(SessionOwnerType::Story),
            WorkflowBindingKind::Story
        );
    }

    #[test]
    fn capability_path_parse_short() {
        let path = CapabilityPath::parse("file_read").unwrap();
        assert_eq!(path.capability, "file_read");
        assert_eq!(path.tool, None);
        assert!(!path.is_tool_level());
        assert_eq!(path.to_qualified_string(), "file_read");
    }

    #[test]
    fn capability_path_parse_long() {
        let path = CapabilityPath::parse("file_read::fs_grep").unwrap();
        assert_eq!(path.capability, "file_read");
        assert_eq!(path.tool.as_deref(), Some("fs_grep"));
        assert!(path.is_tool_level());
        assert_eq!(path.to_qualified_string(), "file_read::fs_grep");
    }

    #[test]
    fn capability_path_parse_mcp_prefix() {
        let short = CapabilityPath::parse("mcp:code_analyzer").unwrap();
        assert_eq!(short.capability, "mcp:code_analyzer");
        assert_eq!(short.tool, None);

        let long = CapabilityPath::parse("mcp:workflow_management::upsert").unwrap();
        assert_eq!(long.capability, "mcp:workflow_management");
        assert_eq!(long.tool.as_deref(), Some("upsert"));
    }

    #[test]
    fn capability_path_parse_rejects_empty() {
        assert!(CapabilityPath::parse("").is_err());
        assert!(CapabilityPath::parse("   ").is_err());
    }

    #[test]
    fn capability_path_parse_rejects_empty_segments() {
        assert!(CapabilityPath::parse("::tool").is_err());
        assert!(CapabilityPath::parse("cap::").is_err());
    }

    #[test]
    fn capability_path_parse_rejects_multi_segment() {
        assert!(CapabilityPath::parse("a::b::c").is_err());
    }

    #[test]
    fn capability_path_serde_uses_qualified_string() {
        let short = CapabilityPath::of_capability("file_read");
        assert_eq!(serde_json::to_string(&short).unwrap(), r#""file_read""#);

        let long = CapabilityPath::of_tool("file_read", "fs_grep");
        assert_eq!(
            serde_json::to_string(&long).unwrap(),
            r#""file_read::fs_grep""#
        );

        let back: CapabilityPath = serde_json::from_str(r#""file_read::fs_grep""#).unwrap();
        assert_eq!(back, long);
    }

    #[test]
    fn capability_directive_add_remove() {
        let add = CapabilityDirective::add_simple("file_read");
        assert!(add.is_add());
        assert!(!add.is_remove());
        assert_eq!(add.key(), "file_read");

        let remove = CapabilityDirective::remove_simple("canvas");
        assert!(!remove.is_add());
        assert!(remove.is_remove());
        assert_eq!(remove.key(), "canvas");
    }

    #[test]
    fn capability_directive_serde_roundtrip() {
        let directives = vec![
            CapabilityDirective::add_simple("file_read"),
            CapabilityDirective::remove_simple("canvas"),
            CapabilityDirective::add_tool("file_read", "fs_read"),
            CapabilityDirective::remove_tool("file_read", "fs_grep"),
            CapabilityDirective::add_simple("mcp:code_analyzer"),
        ];
        let json = serde_json::to_string(&directives).unwrap();
        let deserialized: Vec<CapabilityDirective> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, directives);
    }

    #[test]
    fn capability_directive_json_shape() {
        let add_tool = CapabilityDirective::add_tool("file_read", "fs_read");
        let json = serde_json::to_string(&add_tool).unwrap();
        assert_eq!(json, r#"{"add":"file_read::fs_read"}"#);

        let remove_cap = CapabilityDirective::remove_simple("shell_execute");
        let json = serde_json::to_string(&remove_cap).unwrap();
        assert_eq!(json, r#"{"remove":"shell_execute"}"#);
    }

    #[test]
    fn reduce_empty_yields_empty_state() {
        let reduction = reduce_capability_directives(&[]);
        assert!(reduction.slots.is_empty());
        assert!(reduction.excluded_tools.is_empty());
    }

    #[test]
    fn reduce_add_capability_sets_full_capability() {
        let directives = vec![CapabilityDirective::add_simple("workflow_management")];
        let reduction = reduce_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("workflow_management"),
            Some(&CapabilitySlotState::FullCapability)
        );
    }

    #[test]
    fn reduce_add_tool_yields_whitelist() {
        let directives = vec![CapabilityDirective::add_tool("file_read", "fs_read")];
        let reduction = reduce_capability_directives(&directives);
        match reduction.slots.get("file_read") {
            Some(CapabilitySlotState::ToolWhitelist(set)) => {
                assert!(set.contains("fs_read"));
            }
            other => panic!("期望 ToolWhitelist,实际: {other:?}"),
        }
    }

    #[test]
    fn reduce_remove_capability_marks_blocked() {
        let directives = vec![CapabilityDirective::remove_simple("shell_execute")];
        let reduction = reduce_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("shell_execute"),
            Some(&CapabilitySlotState::Blocked)
        );
    }

    #[test]
    fn reduce_remove_tool_writes_excluded() {
        let directives = vec![CapabilityDirective::remove_tool("file_read", "fs_grep")];
        let reduction = reduce_capability_directives(&directives);
        let excluded = reduction.excluded_tools.get("file_read").unwrap();
        assert!(excluded.contains("fs_grep"));
    }

    #[test]
    fn reduce_add_tool_then_add_cap_upgrades_to_full() {
        let directives = vec![
            CapabilityDirective::add_tool("file_read", "fs_read"),
            CapabilityDirective::add_simple("file_read"),
        ];
        let reduction = reduce_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("file_read"),
            Some(&CapabilitySlotState::FullCapability)
        );
    }

    #[test]
    fn reduce_add_cap_then_remove_tool_keeps_full_plus_exclusion() {
        // FullCapability 状态下的 Remove(tool) 不降级，excluded_tools 记录屏蔽项
        let directives = vec![
            CapabilityDirective::add_simple("file_read"),
            CapabilityDirective::remove_tool("file_read", "fs_grep"),
        ];
        let reduction = reduce_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("file_read"),
            Some(&CapabilitySlotState::FullCapability)
        );
        let excluded = reduction.excluded_tools.get("file_read").unwrap();
        assert!(excluded.contains("fs_grep"));
    }

    #[test]
    fn reduce_remove_then_add_re_enables() {
        // 后来者胜
        let directives = vec![
            CapabilityDirective::remove_simple("canvas"),
            CapabilityDirective::add_simple("canvas"),
        ];
        let reduction = reduce_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("canvas"),
            Some(&CapabilitySlotState::FullCapability)
        );
    }

    #[test]
    fn reduce_add_tool_then_remove_tool_drops_from_whitelist() {
        let directives = vec![
            CapabilityDirective::add_tool("file_read", "fs_read"),
            CapabilityDirective::add_tool("file_read", "fs_glob"),
            CapabilityDirective::remove_tool("file_read", "fs_read"),
        ];
        let reduction = reduce_capability_directives(&directives);
        match reduction.slots.get("file_read") {
            Some(CapabilitySlotState::ToolWhitelist(set)) => {
                assert!(!set.contains("fs_read"));
                assert!(set.contains("fs_glob"));
            }
            other => panic!("期望 ToolWhitelist,实际: {other:?}"),
        }
        let excluded = reduction.excluded_tools.get("file_read").unwrap();
        assert!(excluded.contains("fs_read"));
    }

    #[test]
    fn workflow_contract_capability_directives_default_empty() {
        let json = r#"{}"#;
        let contract: WorkflowContract = serde_json::from_str(json).unwrap();
        assert!(contract.capability_directives.is_empty());

        let back = serde_json::to_string(&contract).unwrap();
        assert!(
            !back.contains("capability_directives"),
            "空 capability_directives 不应出现在序列化结果中: {back}"
        );
    }

    #[test]
    fn workflow_contract_capability_directives_roundtrip() {
        let contract = WorkflowContract {
            capability_directives: vec![
                CapabilityDirective::add_simple("workflow_management"),
                CapabilityDirective::remove_simple("shell_execute"),
                CapabilityDirective::add_tool("file_read", "fs_read"),
            ],
            ..WorkflowContract::default()
        };
        let json = serde_json::to_string(&contract).unwrap();
        let back: WorkflowContract = serde_json::from_str(&json).unwrap();
        assert_eq!(back.capability_directives, contract.capability_directives);
    }

    #[test]
    fn workflow_contract_ignores_legacy_fields_gracefully() {
        // 旧数据可能残留 constraints / completion / capabilities 字段，
        // 移除 deny_unknown_fields 后应静默忽略
        let json = r#"{"constraints":[],"completion":{"checks":[]},"capabilities":["workflow_management"]}"#;
        let contract: WorkflowContract =
            serde_json::from_str(json).expect("旧数据应当可反序列化");
        assert!(contract.output_ports.is_empty());
    }
}
