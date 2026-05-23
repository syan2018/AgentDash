use std::fmt;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::common::{Mount, MountLink};
use crate::session_binding::{ChildSessionId, SessionOwnerType};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
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

pub fn normalize_workflow_binding_kinds(
    kinds: Vec<WorkflowBindingKind>,
) -> Result<Vec<WorkflowBindingKind>, String> {
    let mut normalized = Vec::new();
    for candidate in [WorkflowBindingKind::Project, WorkflowBindingKind::Story] {
        if kinds.contains(&candidate) {
            normalized.push(candidate);
        }
    }
    if normalized.is_empty() {
        return Err("workflow binding_kinds 至少需要一个挂载类型".to_string());
    }
    Ok(normalized)
}

pub fn workflow_binding_kinds_cover(
    required: &[WorkflowBindingKind],
    available: &[WorkflowBindingKind],
) -> bool {
    required.iter().all(|kind| available.contains(kind))
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
    pub guidance: Option<String>,
    #[serde(default)]
    pub context_bindings: Vec<WorkflowContextBinding>,
}

/// 顶层能力模型的声明式配置。
///
/// 工具能力、资源挂载、上下文和策略都属于顶层 Capability Model 的不同维度。
/// 当前已经落地 tool 与 mount 两个维度，后续 context overlay、permission policy、
/// resource budget 等能力维度继续扩展在这里。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, Default)]
pub struct CapabilityConfig {
    /// 工具能力维度的声明式变更。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_directives: Vec<ToolCapabilityDirective>,
    /// VFS/mount 维度的声明式变更。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mount_directives: Vec<MountDirective>,
}

/// VFS/mount 能力指令。
///
/// 这些指令描述 step/workflow 对资源空间的临时装载、撤销、link 和默认 mount
/// 切换。实际运行时会先继承当前 session 的 VFS，再按顺序应用这些指令。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MountDirective {
    AddMount {
        mount: Mount,
    },
    RemoveMount {
        mount_id: String,
    },
    ReplaceMount {
        mount: Mount,
    },
    AddLink {
        link: MountLink,
    },
    RemoveLink {
        from_mount_id: String,
        from_path: String,
    },
    SetDefaultMount {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mount_id: Option<String>,
    },
}

/// Standalone 场景下 input port 的满足策略。
///
/// Lifecycle 内运行时由 edge wire 自动满足；standalone（如主 agent 给子 agent
/// 分配 workflow）时由此字段指示调用方如何提供输入。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StandaloneFulfillment {
    /// 调用方必须在启动前通过 `lifecycle://artifacts/{key}` 写入
    #[default]
    Required,
    /// 可选输入，未提供时使用 default_value
    Optional {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_value: Option<String>,
    },
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
    CompanionResult,
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

// ── ToolCapability 路径 ──

/// ToolCapability 路径 — 统一表达「能力级」和「工具级」两种寻址。
///
/// `capability` 是 tool capability key（如 `"file_read"` 或 `"mcp:code_analyzer"`）；
/// `tool` 为 `None` 表示短 path（整个能力），`Some(name)` 表示长 path（能力下的某个工具）。
///
/// 分隔符统一为 `::`（与 Rust 模块路径同构），与 `mcp:<server>` 的单冒号前缀不冲突。
/// MCP server name 禁止含 `::`，由 preset 校验层强制。
///
/// JSON 形式序列化为 qualified string：`"file_read"` / `"file_read::fs_grep"`
/// / `"workflow_management::upsert_workflow_tool"` / `"mcp:code_analyzer::scan"`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, JsonSchema)]
pub struct ToolCapabilityPath {
    pub capability: String,
    pub tool: Option<String>,
}

const TOOL_CAPABILITY_PATH_SEPARATOR: &str = "::";

impl ToolCapabilityPath {
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
            Some(tool) => format!("{}{TOOL_CAPABILITY_PATH_SEPARATOR}{tool}", self.capability),
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
            return Err("ToolCapabilityPath 不能为空".to_string());
        }

        // 统计 `::` 出现次数（按字符位置扫描，避免误处理单 `:` 前缀）
        let parts: Vec<&str> = trimmed.split(TOOL_CAPABILITY_PATH_SEPARATOR).collect();
        match parts.len() {
            1 => Ok(Self {
                capability: parts[0].to_string(),
                tool: None,
            }),
            2 => {
                let cap = parts[0];
                let tool = parts[1];
                if cap.is_empty() {
                    return Err(format!("ToolCapabilityPath `{s}` 缺少 capability 段"));
                }
                if tool.is_empty() {
                    return Err(format!("ToolCapabilityPath `{s}` 缺少 tool 段"));
                }
                Ok(Self {
                    capability: cap.to_string(),
                    tool: Some(tool.to_string()),
                })
            }
            _ => Err(format!(
                "ToolCapabilityPath `{s}` 包含多个 `::` 分隔符，仅允许一级工具寻址"
            )),
        }
    }
}

impl fmt::Display for ToolCapabilityPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_qualified_string())
    }
}

impl Serialize for ToolCapabilityPath {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_qualified_string())
    }
}

impl<'de> Deserialize<'de> for ToolCapabilityPath {
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
    /// Workflow 级顶层能力配置，包含工具能力、mount/context/policy 等能力维度。
    #[serde(default, skip_serializing_if = "CapabilityConfig::is_empty")]
    pub capability_config: CapabilityConfig,
    /// Workflow 产出声明 — 同时作为完成条件：port gate 门禁根据 `gate_strategy` 检查交付。
    ///
    /// Lifecycle step 绑定 workflow 时自动继承这些 ports 作为默认值，step 编辑器可 override。
    #[serde(
        default,
        alias = "recommended_output_ports",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub output_ports: Vec<OutputPortDefinition>,
    /// Workflow 输入声明 — 同时作为运行约束：lifecycle 内由 edge wire 满足，standalone 由调用方写入。
    #[serde(
        default,
        alias = "recommended_input_ports",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub input_ports: Vec<InputPortDefinition>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSessionTerminalState {
    Completed,
    Failed,
    Interrupted,
}

/// Lifecycle node 类型：Agent Node 创建独立 session，Phase Node 在前一个 session 内切换 contract
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleNodeType {
    /// 创建独立 agent session 执行工作
    #[default]
    AgentNode,
    /// 不创建新 session，在前一个 session 内切换 workflow contract
    PhaseNode,
}

/// 门禁策略：定义 output port 交付检查的严格程度。
/// 实际检查逻辑由对应的 Rhai Hook Preset 实现。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateStrategy {
    #[default]
    Existence,
    Schema,
    LlmJudge,
}

/// Input port 上下文构建策略：控制前驱 output artifact 如何注入后继 session。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    #[default]
    Full,
    Summary,
    MetadataOnly,
    Custom,
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

/// 工具能力指令 —— 在 agent baseline 上执行 Add/Remove。
///
/// `Add(path)` 追加能力或启用工具，`Remove(path)` 屏蔽能力或屏蔽工具。
/// `path` 为短 path 表示能力级操作；长 path 表示工具级操作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolCapabilityDirective {
    Add(ToolCapabilityPath),
    Remove(ToolCapabilityPath),
}

impl ToolCapabilityDirective {
    /// 返回指令操作的 capability key（无论工具级还是能力级）。
    pub fn key(&self) -> &str {
        match self {
            Self::Add(path) | Self::Remove(path) => &path.capability,
        }
    }

    /// 返回指令携带的 path 引用。
    pub fn path(&self) -> &ToolCapabilityPath {
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
        Self::Add(ToolCapabilityPath::of_capability(key))
    }

    /// 快捷构造：能力级 Remove 指令（短 path）。
    pub fn remove_simple(key: impl Into<String>) -> Self {
        Self::Remove(ToolCapabilityPath::of_capability(key))
    }

    /// 快捷构造：工具级 Add 指令（长 path）。
    pub fn add_tool(cap: impl Into<String>, tool: impl Into<String>) -> Self {
        Self::Add(ToolCapabilityPath::of_tool(cap, tool))
    }

    /// 快捷构造：工具级 Remove 指令（长 path）。
    pub fn remove_tool(cap: impl Into<String>, tool: impl Into<String>) -> Self {
        Self::Remove(ToolCapabilityPath::of_tool(cap, tool))
    }
}

/// 工具能力归约状态机 slot —— 对单个 tool capability key 在一串 directive 后的最终状态。
///
/// 状态转移表：见 `compute_capabilities` 实现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCapabilitySlotState {
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
pub struct ToolCapabilityReduction {
    pub slots: std::collections::BTreeMap<String, ToolCapabilitySlotState>,
    pub excluded_tools: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
}

/// 在一串 directive 上执行 slot 规则归约，产出 `ToolCapabilityReduction`。
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
pub fn reduce_tool_capability_directives(
    directives: &[ToolCapabilityDirective],
) -> ToolCapabilityReduction {
    use std::collections::{BTreeMap, BTreeSet};

    let mut slots: BTreeMap<String, ToolCapabilitySlotState> = BTreeMap::new();
    let mut excluded_tools: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for directive in directives {
        match directive {
            ToolCapabilityDirective::Add(path) => {
                let key = path.capability.clone();
                match &path.tool {
                    None => {
                        slots.insert(key, ToolCapabilitySlotState::FullCapability);
                    }
                    Some(tool) => {
                        let entry = slots
                            .entry(key.clone())
                            .or_insert(ToolCapabilitySlotState::NotDeclared);
                        match entry {
                            ToolCapabilitySlotState::FullCapability => {
                                // no-op：全集已启用
                            }
                            ToolCapabilitySlotState::ToolWhitelist(set) => {
                                set.insert(tool.clone());
                            }
                            ToolCapabilitySlotState::NotDeclared
                            | ToolCapabilitySlotState::Blocked => {
                                let mut set = BTreeSet::new();
                                set.insert(tool.clone());
                                *entry = ToolCapabilitySlotState::ToolWhitelist(set);
                            }
                        }
                    }
                }
            }
            ToolCapabilityDirective::Remove(path) => {
                let key = path.capability.clone();
                match &path.tool {
                    None => {
                        slots.insert(key, ToolCapabilitySlotState::Blocked);
                    }
                    Some(tool) => {
                        // 从白名单中移除（若存在），同时写入 excluded_tools
                        if let Some(ToolCapabilitySlotState::ToolWhitelist(set)) =
                            slots.get_mut(&key)
                        {
                            set.remove(tool);
                        }
                        excluded_tools.entry(key).or_default().insert(tool.clone());
                    }
                }
            }
        }
    }

    ToolCapabilityReduction {
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
    /// Step 级顶层能力配置，应用顺序在 workflow contract 配置之后。
    #[serde(default, skip_serializing_if = "CapabilityConfig::is_empty")]
    pub capability_config: CapabilityConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ActivityDefinition {
    pub key: String,
    #[serde(default)]
    pub description: String,
    pub executor: ActivityExecutorSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_ports: Vec<InputPortDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_ports: Vec<OutputPortDefinition>,
    #[serde(default)]
    pub completion_policy: ActivityCompletionPolicy,
    #[serde(default)]
    pub iteration_policy: ActivityIterationPolicy,
    #[serde(default)]
    pub join_policy: ActivityJoinPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityExecutorSpec {
    Agent(AgentActivityExecutorSpec),
    Function(FunctionActivityExecutorSpec),
    Human(HumanActivityExecutorSpec),
}

impl ActivityExecutorSpec {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Agent(_) => "agent",
            Self::Function(_) => "function",
            Self::Human(_) => "human",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct AgentActivityExecutorSpec {
    pub workflow_key: String,
    #[serde(default)]
    pub session_policy: AgentSessionPolicy,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionPolicy {
    #[default]
    SpawnChild,
    ContinueRoot,
    AttachExisting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FunctionActivityExecutorSpec {
    ApiRequest(ApiRequestExecutorSpec),
    BashExec(BashExecExecutorSpec),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ApiRequestExecutorSpec {
    pub method: String,
    pub url_template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_template: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct BashExecExecutorSpec {
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HumanActivityExecutorSpec {
    Approval(HumanApprovalExecutorSpec),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct HumanApprovalExecutorSpec {
    pub form_schema_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityCompletionPolicy {
    OutputPorts { required_ports: Vec<String> },
    ExecutorTerminal,
    HumanDecision { decision_port: String },
    HookGate { hook_key: String },
    OpenEnded,
}

impl Default for ActivityCompletionPolicy {
    fn default() -> Self {
        Self::ExecutorTerminal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ActivityIterationPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub artifact_alias: ArtifactAliasPolicy,
}

impl Default for ActivityIterationPolicy {
    fn default() -> Self {
        Self {
            max_attempts: Some(1),
            artifact_alias: ArtifactAliasPolicy::Latest,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactAliasPolicy {
    #[default]
    Latest,
    PerAttempt,
    LatestAndHistory,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityJoinPolicy {
    #[default]
    All,
    Any,
    First,
    NOfM {
        n: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ActivityTransition {
    pub from: String,
    pub to: String,
    #[serde(default = "default_activity_transition_kind")]
    pub kind: ActivityTransitionKind,
    #[serde(default)]
    pub condition: TransitionCondition,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_bindings: Vec<ArtifactBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_traversals: Option<u32>,
}

fn default_activity_transition_kind() -> ActivityTransitionKind {
    ActivityTransitionKind::Flow
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityTransitionKind {
    Flow,
    Artifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransitionCondition {
    Always,
    ArtifactFieldEquals {
        activity: String,
        port: String,
        path: String,
        value: Value,
    },
    HumanDecisionEquals {
        activity: String,
        decision_port: String,
        value: String,
    },
    AgentSignalEquals {
        activity: String,
        signal_key: String,
        value: Value,
    },
}

impl Default for TransitionCondition {
    fn default() -> Self {
        Self::Always
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ArtifactBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_activity: Option<String>,
    pub from_port: String,
    pub to_port: String,
    #[serde(default)]
    pub alias: ArtifactAliasPolicy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityAttemptStatus {
    Pending,
    Ready,
    Claiming,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl ActivityAttemptStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityAttemptState {
    pub activity_key: String,
    pub attempt: u32,
    pub status: ActivityAttemptStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_run: Option<ExecutorRunRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityPortValue {
    pub port_key: String,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityOutputArtifact {
    pub activity_key: String,
    pub attempt: u32,
    pub port_key: String,
    pub value: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityInputArtifact {
    pub activity_key: String,
    pub attempt: u32,
    pub port_key: String,
    pub source_activity_key: String,
    pub source_attempt: u32,
    pub source_port_key: String,
    pub value: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityLifecycleRunState {
    pub status: ActivityRunStatus,
    pub attempts: Vec<ActivityAttemptState>,
    pub outputs: Vec<ActivityOutputArtifact>,
    pub inputs: Vec<ActivityInputArtifact>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityRunStatus {
    Ready,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutorRunRef {
    AgentSession { session_id: ChildSessionId },
    FunctionRun { run_id: String },
    HumanDecision { decision_id: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityExecutionClaimStatus {
    Claiming,
    Running,
    Succeeded,
    Failed,
    Abandoned,
}

impl ActivityExecutionClaimStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claiming => "claiming",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Claiming | Self::Running)
    }
}

impl std::str::FromStr for ActivityExecutionClaimStatus {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "claiming" => Ok(Self::Claiming),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "abandoned" => Ok(Self::Abandoned),
            _ => Err(format!("activity execution claim status 无效: {raw}")),
        }
    }
}

impl CapabilityConfig {
    pub fn is_empty(&self) -> bool {
        self.tool_directives.is_empty() && self.mount_directives.is_empty()
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

fn bool_true() -> bool {
    true
}

#[cfg(test)]
use super::validation::{
    validate_activity_lifecycle_definition, validate_lifecycle_definition,
    validate_workflow_definition,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_contract() -> WorkflowContract {
        WorkflowContract {
            injection: WorkflowInjectionSpec {
                guidance: Some("read spec first".to_string()),
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
            capability_config: Default::default(),
        }];

        let error = validate_lifecycle_definition("lc", "Lifecycle", "missing", &steps, &[])
            .expect_err("fail");
        assert!(error.contains("entry_step_key"));
    }

    #[test]
    fn validate_lifecycle_definition_rejects_phase_node_entry() {
        let mut steps = vec![simple_step("start")];
        steps[0].node_type = LifecycleNodeType::PhaseNode;

        let error = validate_lifecycle_definition("lc", "Lifecycle", "start", &steps, &[])
            .expect_err("entry phase node should fail");
        assert!(error.contains("PhaseNode"));
        assert!(error.contains("AgentNode"));
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
                capability_config: Default::default(),
            },
            LifecycleStepDefinition {
                key: "b".to_string(),
                description: String::new(),
                workflow_key: None,
                node_type: Default::default(),
                output_ports: vec![],
                input_ports: vec![],
                capability_config: Default::default(),
            },
            LifecycleStepDefinition {
                key: "c".to_string(),
                description: String::new(),
                workflow_key: None,
                node_type: Default::default(),
                output_ports: vec![],
                input_ports: vec![],
                capability_config: Default::default(),
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
                capability_config: Default::default(),
            },
            LifecycleStepDefinition {
                key: "b".to_string(),
                description: String::new(),
                workflow_key: None,
                node_type: Default::default(),
                output_ports: vec![],
                input_ports: vec![],
                capability_config: Default::default(),
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
            capability_config: Default::default(),
        }
    }

    fn activity_agent(
        key: &str,
        input_ports: Vec<InputPortDefinition>,
        output_ports: Vec<OutputPortDefinition>,
    ) -> ActivityDefinition {
        ActivityDefinition {
            key: key.to_string(),
            description: String::new(),
            executor: ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
                workflow_key: format!("workflow.{key}"),
                session_policy: AgentSessionPolicy::SpawnChild,
            }),
            input_ports,
            output_ports,
            completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
            iteration_policy: ActivityIterationPolicy {
                max_attempts: Some(3),
                artifact_alias: ArtifactAliasPolicy::LatestAndHistory,
            },
            join_policy: ActivityJoinPolicy::All,
        }
    }

    fn activity_human_approval(
        key: &str,
        input_ports: Vec<InputPortDefinition>,
        output_ports: Vec<OutputPortDefinition>,
    ) -> ActivityDefinition {
        ActivityDefinition {
            key: key.to_string(),
            description: String::new(),
            executor: ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(
                HumanApprovalExecutorSpec {
                    form_schema_key: "approval.plan_review".to_string(),
                    title: None,
                },
            )),
            input_ports,
            output_ports,
            completion_policy: ActivityCompletionPolicy::HumanDecision {
                decision_port: "decision".to_string(),
            },
            iteration_policy: ActivityIterationPolicy {
                max_attempts: Some(3),
                artifact_alias: ArtifactAliasPolicy::LatestAndHistory,
            },
            join_policy: ActivityJoinPolicy::All,
        }
    }

    fn input_port(key: &str) -> InputPortDefinition {
        InputPortDefinition {
            key: key.to_string(),
            description: format!("{key} input"),
            context_strategy: ContextStrategy::Full,
            context_template: None,
            standalone_fulfillment: StandaloneFulfillment::Required,
        }
    }

    fn output_port(key: &str) -> OutputPortDefinition {
        OutputPortDefinition {
            key: key.to_string(),
            description: format!("{key} output"),
            gate_strategy: GateStrategy::Existence,
            gate_params: None,
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
    fn validate_activity_lifecycle_accepts_human_approval_loop() {
        let activities = vec![
            activity_agent(
                "plan",
                vec![input_port("feedback")],
                vec![output_port("proposal")],
            ),
            activity_human_approval(
                "approval",
                vec![input_port("proposal")],
                vec![output_port("decision")],
            ),
            activity_agent(
                "implement",
                vec![input_port("approved_plan")],
                vec![output_port("summary")],
            ),
        ];
        let transitions = vec![
            ActivityTransition {
                from: "plan".to_string(),
                to: "approval".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::Always,
                artifact_bindings: vec![ArtifactBinding {
                    from_activity: None,
                    from_port: "proposal".to_string(),
                    to_port: "proposal".to_string(),
                    alias: ArtifactAliasPolicy::Latest,
                }],
                max_traversals: None,
            },
            ActivityTransition {
                from: "approval".to_string(),
                to: "implement".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::HumanDecisionEquals {
                    activity: "approval".to_string(),
                    decision_port: "decision".to_string(),
                    value: "approved".to_string(),
                },
                artifact_bindings: vec![ArtifactBinding {
                    from_activity: Some("plan".to_string()),
                    from_port: "proposal".to_string(),
                    to_port: "approved_plan".to_string(),
                    alias: ArtifactAliasPolicy::Latest,
                }],
                max_traversals: None,
            },
            ActivityTransition {
                from: "approval".to_string(),
                to: "plan".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::HumanDecisionEquals {
                    activity: "approval".to_string(),
                    decision_port: "decision".to_string(),
                    value: "rejected".to_string(),
                },
                artifact_bindings: vec![ArtifactBinding {
                    from_activity: None,
                    from_port: "decision".to_string(),
                    to_port: "feedback".to_string(),
                    alias: ArtifactAliasPolicy::Latest,
                }],
                max_traversals: None,
            },
        ];

        validate_activity_lifecycle_definition(
            "lc",
            "Lifecycle",
            "plan",
            &activities,
            &transitions,
        )
        .expect("approval loop should be bounded by typed decision and retry policy");
    }

    #[test]
    fn validate_activity_lifecycle_rejects_missing_artifact_port() {
        let activities = vec![
            activity_agent("plan", vec![], vec![output_port("proposal")]),
            activity_agent("implement", vec![input_port("approved_plan")], vec![]),
        ];
        let transitions = vec![ActivityTransition {
            from: "plan".to_string(),
            to: "implement".to_string(),
            kind: ActivityTransitionKind::Flow,
            condition: TransitionCondition::Always,
            artifact_bindings: vec![ArtifactBinding {
                from_activity: None,
                from_port: "missing".to_string(),
                to_port: "approved_plan".to_string(),
                alias: ArtifactAliasPolicy::Latest,
            }],
            max_traversals: None,
        }];

        let err = validate_activity_lifecycle_definition(
            "lc",
            "Lifecycle",
            "plan",
            &activities,
            &transitions,
        )
        .expect_err("missing output port should fail");
        assert!(err.contains("from_port"));
    }

    #[test]
    fn validate_activity_lifecycle_rejects_unbounded_entry_loop() {
        let mut plan = activity_agent("plan", vec![], vec![output_port("proposal")]);
        plan.iteration_policy.max_attempts = None;
        let activities = vec![
            plan,
            activity_agent("review", vec![input_port("proposal")], vec![]),
        ];
        let transitions = vec![
            ActivityTransition {
                from: "plan".to_string(),
                to: "review".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::Always,
                artifact_bindings: vec![ArtifactBinding {
                    from_activity: None,
                    from_port: "proposal".to_string(),
                    to_port: "proposal".to_string(),
                    alias: ArtifactAliasPolicy::Latest,
                }],
                max_traversals: None,
            },
            ActivityTransition {
                from: "review".to_string(),
                to: "plan".to_string(),
                kind: ActivityTransitionKind::Flow,
                condition: TransitionCondition::Always,
                artifact_bindings: vec![],
                max_traversals: None,
            },
        ];

        let err = validate_activity_lifecycle_definition(
            "lc",
            "Lifecycle",
            "plan",
            &activities,
            &transitions,
        )
        .expect_err("unbounded loop should fail");
        assert!(err.contains("循环 transition"));
    }

    #[test]
    fn validate_activity_lifecycle_rejects_unconditional_self_loop() {
        let activities = vec![activity_agent(
            "plan",
            vec![],
            vec![output_port("proposal")],
        )];
        let transitions = vec![ActivityTransition {
            from: "plan".to_string(),
            to: "plan".to_string(),
            kind: ActivityTransitionKind::Flow,
            condition: TransitionCondition::Always,
            artifact_bindings: vec![],
            max_traversals: Some(3),
        }];

        let err = validate_activity_lifecycle_definition(
            "lc",
            "Lifecycle",
            "plan",
            &activities,
            &transitions,
        )
        .expect_err("unconditional self loop should fail");
        assert!(err.contains("无条件自环"));
    }

    #[test]
    fn activity_executor_serializes_human_kind_and_type() {
        let executor = ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(
            HumanApprovalExecutorSpec {
                form_schema_key: "approval.plan_review".to_string(),
                title: None,
            },
        ));

        let value = serde_json::to_value(executor).expect("serialize executor");
        assert_eq!(value["kind"], "human");
        assert_eq!(value["type"], "approval");
        assert_eq!(value["form_schema_key"], "approval.plan_review");
    }

    #[test]
    fn activity_executor_serializes_function_kind_and_type() {
        let executor = ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::BashExec(
            BashExecExecutorSpec {
                command: "pnpm".to_string(),
                args: vec!["test".to_string()],
                working_directory: None,
            },
        ));

        let value = serde_json::to_value(executor).expect("serialize executor");
        assert_eq!(value["kind"], "function");
        assert_eq!(value["type"], "bash_exec");
        assert_eq!(value["command"], "pnpm");
    }

    #[test]
    fn activity_executor_serializes_agent_kind() {
        let executor = ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
            workflow_key: "workflow.plan".to_string(),
            session_policy: AgentSessionPolicy::SpawnChild,
        });

        let value = serde_json::to_value(executor).expect("serialize executor");
        assert_eq!(value["kind"], "agent");
        assert_eq!(value["workflow_key"], "workflow.plan");
        assert_eq!(value["session_policy"], "spawn_child");
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
            normalize_workflow_binding_kinds(vec![
                WorkflowBindingKind::Story,
                WorkflowBindingKind::Project,
                WorkflowBindingKind::Story,
            ])
            .unwrap(),
            vec![WorkflowBindingKind::Project, WorkflowBindingKind::Story]
        );
        assert!(workflow_binding_kinds_cover(
            &[WorkflowBindingKind::Story],
            &[WorkflowBindingKind::Project, WorkflowBindingKind::Story]
        ));
        assert!(!workflow_binding_kinds_cover(
            &[WorkflowBindingKind::Project, WorkflowBindingKind::Story],
            &[WorkflowBindingKind::Story]
        ));
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
    fn tool_capability_path_parse_short() {
        let path = ToolCapabilityPath::parse("file_read").unwrap();
        assert_eq!(path.capability, "file_read");
        assert_eq!(path.tool, None);
        assert!(!path.is_tool_level());
        assert_eq!(path.to_qualified_string(), "file_read");
    }

    #[test]
    fn tool_capability_path_parse_long() {
        let path = ToolCapabilityPath::parse("file_read::fs_grep").unwrap();
        assert_eq!(path.capability, "file_read");
        assert_eq!(path.tool.as_deref(), Some("fs_grep"));
        assert!(path.is_tool_level());
        assert_eq!(path.to_qualified_string(), "file_read::fs_grep");
    }

    #[test]
    fn tool_capability_path_parse_mcp_prefix() {
        let short = ToolCapabilityPath::parse("mcp:code_analyzer").unwrap();
        assert_eq!(short.capability, "mcp:code_analyzer");
        assert_eq!(short.tool, None);

        let long = ToolCapabilityPath::parse("mcp:code_analyzer::scan").unwrap();
        assert_eq!(long.capability, "mcp:code_analyzer");
        assert_eq!(long.tool.as_deref(), Some("scan"));
    }

    #[test]
    fn tool_capability_path_parse_rejects_empty() {
        assert!(ToolCapabilityPath::parse("").is_err());
        assert!(ToolCapabilityPath::parse("   ").is_err());
    }

    #[test]
    fn tool_capability_path_parse_rejects_empty_segments() {
        assert!(ToolCapabilityPath::parse("::tool").is_err());
        assert!(ToolCapabilityPath::parse("cap::").is_err());
    }

    #[test]
    fn tool_capability_path_parse_rejects_multi_segment() {
        assert!(ToolCapabilityPath::parse("a::b::c").is_err());
    }

    #[test]
    fn tool_capability_path_serde_uses_qualified_string() {
        let short = ToolCapabilityPath::of_capability("file_read");
        assert_eq!(serde_json::to_string(&short).unwrap(), r#""file_read""#);

        let long = ToolCapabilityPath::of_tool("file_read", "fs_grep");
        assert_eq!(
            serde_json::to_string(&long).unwrap(),
            r#""file_read::fs_grep""#
        );

        let back: ToolCapabilityPath = serde_json::from_str(r#""file_read::fs_grep""#).unwrap();
        assert_eq!(back, long);
    }

    #[test]
    fn tool_capability_directive_add_remove() {
        let add = ToolCapabilityDirective::add_simple("file_read");
        assert!(add.is_add());
        assert!(!add.is_remove());
        assert_eq!(add.key(), "file_read");

        let remove = ToolCapabilityDirective::remove_simple("canvas");
        assert!(!remove.is_add());
        assert!(remove.is_remove());
        assert_eq!(remove.key(), "canvas");
    }

    #[test]
    fn tool_capability_directive_serde_roundtrip() {
        let directives = vec![
            ToolCapabilityDirective::add_simple("file_read"),
            ToolCapabilityDirective::remove_simple("canvas"),
            ToolCapabilityDirective::add_tool("file_read", "fs_read"),
            ToolCapabilityDirective::remove_tool("file_read", "fs_grep"),
            ToolCapabilityDirective::add_simple("mcp:code_analyzer"),
        ];
        let json = serde_json::to_string(&directives).unwrap();
        let deserialized: Vec<ToolCapabilityDirective> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, directives);
    }

    #[test]
    fn tool_capability_directive_json_shape() {
        let add_tool = ToolCapabilityDirective::add_tool("file_read", "fs_read");
        let json = serde_json::to_string(&add_tool).unwrap();
        assert_eq!(json, r#"{"add":"file_read::fs_read"}"#);

        let remove_cap = ToolCapabilityDirective::remove_simple("shell_execute");
        let json = serde_json::to_string(&remove_cap).unwrap();
        assert_eq!(json, r#"{"remove":"shell_execute"}"#);
    }

    #[test]
    fn reduce_empty_yields_empty_state() {
        let reduction = reduce_tool_capability_directives(&[]);
        assert!(reduction.slots.is_empty());
        assert!(reduction.excluded_tools.is_empty());
    }

    #[test]
    fn reduce_add_capability_sets_full_capability() {
        let directives = vec![ToolCapabilityDirective::add_simple("workflow_management")];
        let reduction = reduce_tool_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("workflow_management"),
            Some(&ToolCapabilitySlotState::FullCapability)
        );
    }

    #[test]
    fn reduce_add_tool_yields_whitelist() {
        let directives = vec![ToolCapabilityDirective::add_tool("file_read", "fs_read")];
        let reduction = reduce_tool_capability_directives(&directives);
        match reduction.slots.get("file_read") {
            Some(ToolCapabilitySlotState::ToolWhitelist(set)) => {
                assert!(set.contains("fs_read"));
            }
            other => panic!("期望 ToolWhitelist,实际: {other:?}"),
        }
    }

    #[test]
    fn reduce_remove_capability_marks_blocked() {
        let directives = vec![ToolCapabilityDirective::remove_simple("shell_execute")];
        let reduction = reduce_tool_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("shell_execute"),
            Some(&ToolCapabilitySlotState::Blocked)
        );
    }

    #[test]
    fn reduce_remove_tool_writes_excluded() {
        let directives = vec![ToolCapabilityDirective::remove_tool("file_read", "fs_grep")];
        let reduction = reduce_tool_capability_directives(&directives);
        let excluded = reduction.excluded_tools.get("file_read").unwrap();
        assert!(excluded.contains("fs_grep"));
    }

    #[test]
    fn reduce_add_tool_then_add_cap_upgrades_to_full() {
        let directives = vec![
            ToolCapabilityDirective::add_tool("file_read", "fs_read"),
            ToolCapabilityDirective::add_simple("file_read"),
        ];
        let reduction = reduce_tool_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("file_read"),
            Some(&ToolCapabilitySlotState::FullCapability)
        );
    }

    #[test]
    fn reduce_add_cap_then_remove_tool_keeps_full_plus_exclusion() {
        // FullCapability 状态下的 Remove(tool) 不降级，excluded_tools 记录屏蔽项
        let directives = vec![
            ToolCapabilityDirective::add_simple("file_read"),
            ToolCapabilityDirective::remove_tool("file_read", "fs_grep"),
        ];
        let reduction = reduce_tool_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("file_read"),
            Some(&ToolCapabilitySlotState::FullCapability)
        );
        let excluded = reduction.excluded_tools.get("file_read").unwrap();
        assert!(excluded.contains("fs_grep"));
    }

    #[test]
    fn reduce_remove_then_add_re_enables() {
        // 后来者胜
        let directives = vec![
            ToolCapabilityDirective::remove_simple("canvas"),
            ToolCapabilityDirective::add_simple("canvas"),
        ];
        let reduction = reduce_tool_capability_directives(&directives);
        assert_eq!(
            reduction.slots.get("canvas"),
            Some(&ToolCapabilitySlotState::FullCapability)
        );
    }

    #[test]
    fn reduce_add_tool_then_remove_tool_drops_from_whitelist() {
        let directives = vec![
            ToolCapabilityDirective::add_tool("file_read", "fs_read"),
            ToolCapabilityDirective::add_tool("file_read", "fs_glob"),
            ToolCapabilityDirective::remove_tool("file_read", "fs_read"),
        ];
        let reduction = reduce_tool_capability_directives(&directives);
        match reduction.slots.get("file_read") {
            Some(ToolCapabilitySlotState::ToolWhitelist(set)) => {
                assert!(!set.contains("fs_read"));
                assert!(set.contains("fs_glob"));
            }
            other => panic!("期望 ToolWhitelist,实际: {other:?}"),
        }
        let excluded = reduction.excluded_tools.get("file_read").unwrap();
        assert!(excluded.contains("fs_read"));
    }

    #[test]
    fn workflow_contract_capability_config_default_empty() {
        let json = r#"{}"#;
        let contract: WorkflowContract = serde_json::from_str(json).unwrap();
        assert!(contract.capability_config.is_empty());

        let back = serde_json::to_string(&contract).unwrap();
        assert!(
            !back.contains("capability_config"),
            "空 capability_config 不应出现在序列化结果中: {back}"
        );
    }

    #[test]
    fn workflow_contract_tool_directives_roundtrip() {
        let contract = WorkflowContract {
            capability_config: CapabilityConfig {
                tool_directives: vec![
                    ToolCapabilityDirective::add_simple("workflow_management"),
                    ToolCapabilityDirective::remove_simple("shell_execute"),
                    ToolCapabilityDirective::add_tool("file_read", "fs_read"),
                ],
                ..CapabilityConfig::default()
            },
            ..WorkflowContract::default()
        };
        let json = serde_json::to_string(&contract).unwrap();
        assert!(json.contains("capability_config"));
        assert!(json.contains("tool_directives"));
        assert!(!json.contains("capability_directives"));
        let back: WorkflowContract = serde_json::from_str(&json).unwrap();
        assert_eq!(back.capability_config, contract.capability_config);
    }

    #[test]
    fn capability_config_mount_directives_roundtrip() {
        let contract = WorkflowContract {
            capability_config: CapabilityConfig {
                mount_directives: vec![MountDirective::AddMount {
                    mount: Mount {
                        id: "review".to_string(),
                        provider: "inline_fs".to_string(),
                        backend_id: "backend".to_string(),
                        root_ref: "inline://review".to_string(),
                        capabilities: vec![crate::common::MountCapability::Read],
                        default_write: false,
                        display_name: "Review".to_string(),
                        metadata: serde_json::Value::Null,
                    },
                }],
                ..CapabilityConfig::default()
            },
            ..WorkflowContract::default()
        };
        let json = serde_json::to_string(&contract).unwrap();
        assert!(json.contains("capability_config"));
        assert!(json.contains("mount_directives"));

        let back: WorkflowContract = serde_json::from_str(&json).unwrap();
        assert_eq!(back.capability_config, contract.capability_config);
    }

    #[test]
    fn workflow_contract_ignores_legacy_fields_gracefully() {
        // 旧数据可能残留 constraints / completion / capabilities 字段，
        // 移除 deny_unknown_fields 后应静默忽略
        let json = r#"{"constraints":[],"completion":{"checks":[]},"capabilities":["workflow_management"]}"#;
        let contract: WorkflowContract = serde_json::from_str(json).expect("旧数据应当可反序列化");
        assert!(contract.output_ports.is_empty());
    }
}
