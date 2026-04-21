use std::fmt;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session_binding::SessionOwnerType;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// Workflow 可挂载到哪一类 owner。
/// 这里只描述绑定范围，不表达 workflow 自身的业务主语。
///
/// 当前与 `WorkflowBindingRole` 1:1 同构——预留扩展点：
/// 未来若引入跨 owner 共享 workflow（如 Story-bound workflow 推荐给 Task），
/// Kind 保持不变，Role 可引入新变体。
pub enum WorkflowBindingKind {
    Project,
    Story,
    Task,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// Workflow 建议由哪一类 owner/session 使用。
/// 它是绑定层提示，不是 workflow 内建业务角色。
///
/// 当前与 `WorkflowBindingKind` 1:1 同构——见 Kind 注释说明预留扩展理由。
pub enum WorkflowBindingRole {
    Project,
    Story,
    Task,
}

impl WorkflowBindingKind {
    pub fn binding_scope_key(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Story => "story",
            Self::Task => "task",
        }
    }

    pub fn from_binding_scope(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "project" => Some(Self::Project),
            "story" => Some(Self::Story),
            "task" => Some(Self::Task),
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
            Self::Task => "task",
        }
    }
}

impl From<SessionOwnerType> for WorkflowBindingKind {
    fn from(value: SessionOwnerType) -> Self {
        match value {
            SessionOwnerType::Project => Self::Project,
            SessionOwnerType::Story => Self::Story,
            SessionOwnerType::Task => Self::Task,
        }
    }
}

impl From<WorkflowBindingKind> for WorkflowBindingRole {
    fn from(value: WorkflowBindingKind) -> Self {
        match value {
            WorkflowBindingKind::Project => Self::Project,
            WorkflowBindingKind::Story => Self::Story,
            WorkflowBindingKind::Task => Self::Task,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowConstraintKind {
    BlockStopUntilChecksPass,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct WorkflowConstraintSpec {
    pub key: String,
    pub kind: WorkflowConstraintKind,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCheckKind {
    ArtifactExists,
    ArtifactCountGte,
    SessionTerminalIn,
    ChecklistEvidencePresent,
    ExplicitActionReceived,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct WorkflowCheckSpec {
    pub key: String,
    pub kind: WorkflowCheckKind,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, Default)]
pub struct WorkflowCompletionSpec {
    #[serde(default)]
    pub checks: Vec<WorkflowCheckSpec>,
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

// ── Capability 结构化条目 ──

/// 工具能力的细粒度声明。
///
/// 每个条目指定一个 capability key，并可选地通过 include/exclude 裁剪
/// 该能力下属的具体工具集。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct CapabilityDetailedEntry {
    pub key: String,
    /// 白名单：仅启用此 capability 下属的指定工具。为空表示启用全部。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_tools: Vec<String>,
    /// 黑名单：从此 capability 下属工具中排除指定工具。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_tools: Vec<String>,
}

/// Workflow 能力声明条目 — 支持简写（纯 key）和结构化（带工具裁剪）两种形式。
///
/// JSON 序列化向后兼容：`"file_system"` 反序列化为 `Simple`，
/// `{ "key": "file_read", "exclude_tools": ["fs_grep"] }` 反序列化为 `Detailed`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(untagged)]
pub enum CapabilityEntry {
    Simple(String),
    Detailed(CapabilityDetailedEntry),
}

impl CapabilityEntry {
    pub fn key(&self) -> &str {
        match self {
            Self::Simple(k) => k,
            Self::Detailed(d) => &d.key,
        }
    }

    pub fn include_tools(&self) -> &[String] {
        match self {
            Self::Simple(_) => &[],
            Self::Detailed(d) => &d.include_tools,
        }
    }

    pub fn exclude_tools(&self) -> &[String] {
        match self {
            Self::Simple(_) => &[],
            Self::Detailed(d) => &d.exclude_tools,
        }
    }

    pub fn has_tool_filter(&self) -> bool {
        match self {
            Self::Simple(_) => false,
            Self::Detailed(d) => !d.include_tools.is_empty() || !d.exclude_tools.is_empty(),
        }
    }

    /// 从纯 key 构造简写条目。
    pub fn simple(key: impl Into<String>) -> Self {
        Self::Simple(key.into())
    }

    /// 从 key + 排除列表构造结构化条目。
    pub fn with_excludes(key: impl Into<String>, excludes: Vec<String>) -> Self {
        Self::Detailed(CapabilityDetailedEntry {
            key: key.into(),
            include_tools: Vec::new(),
            exclude_tools: excludes,
        })
    }

    /// 从 key + 白名单构造结构化条目。
    pub fn with_includes(key: impl Into<String>, includes: Vec<String>) -> Self {
        Self::Detailed(CapabilityDetailedEntry {
            key: key.into(),
            include_tools: includes,
            exclude_tools: Vec::new(),
        })
    }
}

impl fmt::Display for CapabilityEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

impl From<&str> for CapabilityEntry {
    fn from(s: &str) -> Self {
        Self::Simple(s.to_string())
    }
}

impl From<String> for CapabilityEntry {
    fn from(s: String) -> Self {
        Self::Simple(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, Default)]
pub struct WorkflowContract {
    #[serde(default)]
    pub injection: WorkflowInjectionSpec,
    #[serde(default)]
    pub hook_rules: Vec<WorkflowHookRuleSpec>,
    #[serde(default)]
    pub constraints: Vec<WorkflowConstraintSpec>,
    #[serde(default)]
    pub completion: WorkflowCompletionSpec,
    /// 推荐 ports（模板用途）。运行时产出约束由 LifecycleStepDefinition 级 ports 定义。
    #[serde(default, alias = "output_ports", skip_serializing_if = "Vec::is_empty")]
    pub recommended_output_ports: Vec<OutputPortDefinition>,
    #[serde(default, alias = "input_ports", skip_serializing_if = "Vec::is_empty")]
    pub recommended_input_ports: Vec<InputPortDefinition>,
    /// Workflow 级基线能力集合。
    ///
    /// 每个条目可以是：
    /// - 简写形式：`"file_read"` — 启用整个能力的全部工具
    /// - 结构化形式：`{ "key": "file_read", "exclude_tools": ["fs_grep"] }` — 带工具级裁剪
    /// - 平台别名：`"file_system"` — 自动展开为 `file_read + file_write + shell_execute`
    /// - 自定义 MCP：`"mcp:<preset_name>"` — 指向 project 级 McpPreset
    ///
    /// 运行时 hook runtime 可以通过 `CapabilityDirective` 在此基线上动态增减；
    /// Step 级不再承担能力声明。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<CapabilityEntry>,
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
}

/// 运行时能力指令 — 在 workflow 基线上执行增减运算。
///
/// `Add(entry)` 追加能力（支持工具级裁剪），`Remove(key)` 按 key 移除能力。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDirective {
    Add(CapabilityEntry),
    Remove(String),
}

impl CapabilityDirective {
    pub fn key(&self) -> &str {
        match self {
            Self::Add(entry) => entry.key(),
            Self::Remove(k) => k,
        }
    }

    pub fn is_add(&self) -> bool {
        matches!(self, Self::Add(_))
    }

    /// 快捷构造：简单 key 的 Add 指令。
    pub fn add_simple(key: impl Into<String>) -> Self {
        Self::Add(CapabilityEntry::simple(key))
    }

    /// 快捷构造：从 CapabilityEntry 构造 Add 指令。
    pub fn add_entry(entry: CapabilityEntry) -> Self {
        Self::Add(entry)
    }
}

/// 在 workflow 基线能力集上应用运行时指令，产出 effective capability 集合。
///
/// 规则：
/// - 无指令 → 完全继承 baseline
/// - `Add(entry)` → 追加到集合（相同 key 的后项覆盖前项）
/// - `Remove(key)` → 从集合中按 key 移除
pub fn compute_effective_capabilities(
    baseline: &[CapabilityEntry],
    directives: &[CapabilityDirective],
) -> Vec<CapabilityEntry> {
    use std::collections::BTreeMap;

    if directives.is_empty() {
        return baseline.to_vec();
    }

    let mut effective: BTreeMap<String, CapabilityEntry> = baseline
        .iter()
        .map(|e| (e.key().to_string(), e.clone()))
        .collect();

    for directive in directives {
        match directive {
            CapabilityDirective::Add(entry) => {
                effective.insert(entry.key().to_string(), entry.clone());
            }
            CapabilityDirective::Remove(key) => {
                effective.remove(key.as_str());
            }
        }
    }

    effective.into_values().collect()
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
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
    #[serde(default)]
    pub constraints: Vec<WorkflowConstraintSpec>,
    #[serde(default)]
    pub completion: WorkflowCompletionSpec,
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

    let mut seen_constraint_keys = std::collections::BTreeSet::new();
    for (index, constraint) in contract.constraints.iter().enumerate() {
        validate_identity(
            &format!("{field_path}.constraints[{index}].key"),
            &constraint.key,
        )?;
        validate_non_empty(
            &format!("{field_path}.constraints[{index}].description"),
            &constraint.description,
        )?;
        if !seen_constraint_keys.insert(constraint.key.clone()) {
            return Err(format!(
                "{field_path}.constraints[{index}].key 重复: {}",
                constraint.key
            ));
        }
    }

    let mut seen_check_keys = std::collections::BTreeSet::new();
    for (index, check) in contract.completion.checks.iter().enumerate() {
        validate_identity(
            &format!("{field_path}.completion.checks[{index}].key"),
            &check.key,
        )?;
        validate_non_empty(
            &format!("{field_path}.completion.checks[{index}].description"),
            &check.description,
        )?;
        if !seen_check_keys.insert(check.key.clone()) {
            return Err(format!(
                "{field_path}.completion.checks[{index}].key 重复: {}",
                check.key
            ));
        }
    }

    let mut seen_output_port_keys = std::collections::BTreeSet::new();
    for (index, port) in contract.recommended_output_ports.iter().enumerate() {
        validate_identity(
            &format!("{field_path}.recommended_output_ports[{index}].key"),
            &port.key,
        )?;
        validate_non_empty(
            &format!("{field_path}.recommended_output_ports[{index}].description"),
            &port.description,
        )?;
        if !seen_output_port_keys.insert(port.key.clone()) {
            return Err(format!(
                "{field_path}.recommended_output_ports[{index}].key 重复: {}",
                port.key
            ));
        }
    }

    let mut seen_input_port_keys = std::collections::BTreeSet::new();
    for (index, port) in contract.recommended_input_ports.iter().enumerate() {
        validate_identity(
            &format!("{field_path}.recommended_input_ports[{index}].key"),
            &port.key,
        )?;
        validate_non_empty(
            &format!("{field_path}.recommended_input_ports[{index}].description"),
            &port.description,
        )?;
        if !seen_input_port_keys.insert(port.key.clone()) {
            return Err(format!(
                "{field_path}.recommended_input_ports[{index}].key 重复: {}",
                port.key
            ));
        }
    }

    Ok(())
}

/// 从 port-level edges 计算 node 级别依赖关系。
/// 返回 `{ to_node -> Set<from_node> }`，多条连同一对 node 的 edge 自动去重。
pub fn node_deps_from_edges(edges: &[LifecycleEdge]) -> std::collections::HashMap<&str, std::collections::BTreeSet<&str>> {
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
                    return Err(format!(
                        "lifecycle.edges[{i}] kind=flow 不应携带 port"
                    ));
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
        return Err(format!(
            "entry_step_key `{entry_step_key}` 不应有入边"
        ));
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
    fn validate_workflow_definition_rejects_duplicate_constraint_keys() {
        let mut contract = sample_contract();
        contract.constraints = vec![
            WorkflowConstraintSpec {
                key: "a".to_string(),
                kind: WorkflowConstraintKind::Custom,
                description: "x".to_string(),
                payload: None,
            },
            WorkflowConstraintSpec {
                key: "a".to_string(),
                kind: WorkflowConstraintKind::Custom,
                description: "y".to_string(),
                payload: None,
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

        let error =
            validate_lifecycle_definition("lc", "Lifecycle", "missing", &steps, &[])
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
        let edges = vec![
            LifecycleEdge::flow("a", "b"),
            LifecycleEdge::flow("b", "c"),
        ];
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
            WorkflowBindingKind::from_owner_type(" task "),
            Some(WorkflowBindingKind::Task)
        );
        assert_eq!(
            WorkflowBindingKind::from_binding_scope("story"),
            Some(WorkflowBindingKind::Story)
        );
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
        assert_eq!(WorkflowBindingKind::Task.binding_scope_key(), "task");
    }

    #[test]
    fn capability_directive_add_remove() {
        let add = CapabilityDirective::add_simple("file_system");
        assert!(add.is_add());
        assert_eq!(add.key(), "file_system");

        let remove = CapabilityDirective::Remove("canvas".to_string());
        assert!(!remove.is_add());
        assert_eq!(remove.key(), "canvas");
    }

    #[test]
    fn capability_directive_serde_roundtrip() {
        let directives = vec![
            CapabilityDirective::add_simple("file_system"),
            CapabilityDirective::Remove("canvas".to_string()),
            CapabilityDirective::add_simple("mcp:code_analyzer"),
        ];
        let json = serde_json::to_string(&directives).unwrap();
        let deserialized: Vec<CapabilityDirective> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, directives);
    }

    #[test]
    fn capability_entry_simple_serde() {
        let entry = CapabilityEntry::simple("file_system");
        let json = serde_json::to_string(&entry).unwrap();
        assert_eq!(json, r#""file_system""#);
        let back: CapabilityEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.key(), "file_system");
        assert!(!back.has_tool_filter());
    }

    #[test]
    fn capability_entry_detailed_serde() {
        let entry = CapabilityEntry::with_excludes(
            "file_read",
            vec!["fs_grep".to_string()],
        );
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("file_read"));
        assert!(json.contains("fs_grep"));
        let back: CapabilityEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.key(), "file_read");
        assert!(back.has_tool_filter());
        assert_eq!(back.exclude_tools(), &["fs_grep".to_string()]);
    }

    #[test]
    fn capability_entry_from_string() {
        let entry: CapabilityEntry = "canvas".into();
        assert_eq!(entry.key(), "canvas");
        assert!(!entry.has_tool_filter());
    }

    #[test]
    fn compute_effective_empty_directives_inherits_baseline() {
        let baseline = vec![CapabilityEntry::simple("canvas"), CapabilityEntry::simple("file_system")];
        let effective = compute_effective_capabilities(&baseline, &[]);
        assert_eq!(effective.len(), 2);
        assert_eq!(effective[0].key(), "canvas");
    }

    #[test]
    fn compute_effective_add_extends_baseline() {
        let baseline = vec![CapabilityEntry::simple("file_system")];
        let directives = vec![
            CapabilityDirective::add_simple("canvas"),
            CapabilityDirective::add_simple("mcp:analyzer"),
        ];
        let effective = compute_effective_capabilities(&baseline, &directives);
        let keys: Vec<&str> = effective.iter().map(|e| e.key()).collect();
        assert!(keys.contains(&"canvas"));
        assert!(keys.contains(&"file_system"));
        assert!(keys.contains(&"mcp:analyzer"));
    }

    #[test]
    fn compute_effective_remove_shrinks_baseline() {
        let baseline = vec![
            CapabilityEntry::simple("canvas"),
            CapabilityEntry::simple("collaboration"),
            CapabilityEntry::simple("file_system"),
        ];
        let directives = vec![CapabilityDirective::Remove("canvas".to_string())];
        let effective = compute_effective_capabilities(&baseline, &directives);
        let keys: Vec<&str> = effective.iter().map(|e| e.key()).collect();
        assert_eq!(keys, vec!["collaboration", "file_system"]);
    }

    #[test]
    fn compute_effective_add_and_remove_combined() {
        let baseline = vec![CapabilityEntry::simple("canvas"), CapabilityEntry::simple("file_system")];
        let directives = vec![
            CapabilityDirective::Remove("canvas".to_string()),
            CapabilityDirective::add_simple("workflow"),
        ];
        let effective = compute_effective_capabilities(&baseline, &directives);
        let keys: Vec<&str> = effective.iter().map(|e| e.key()).collect();
        assert_eq!(keys, vec!["file_system", "workflow"]);
    }

    #[test]
    fn compute_effective_remove_nonexistent_is_noop() {
        let baseline = vec![CapabilityEntry::simple("file_system")];
        let directives = vec![CapabilityDirective::Remove("nonexistent".to_string())];
        let effective = compute_effective_capabilities(&baseline, &directives);
        assert_eq!(effective.len(), 1);
        assert_eq!(effective[0].key(), "file_system");
    }

    #[test]
    fn compute_effective_detailed_entry_preserved() {
        let baseline = vec![CapabilityEntry::simple("file_read")];
        let directives = vec![CapabilityDirective::Add(
            CapabilityEntry::with_excludes("file_write", vec!["fs_apply_patch".to_string()]),
        )];
        let effective = compute_effective_capabilities(&baseline, &directives);
        assert_eq!(effective.len(), 2);
        let write = effective.iter().find(|e| e.key() == "file_write").unwrap();
        assert_eq!(write.exclude_tools(), &["fs_apply_patch".to_string()]);
    }

    #[test]
    fn lifecycle_step_definition_roundtrip_without_capabilities() {
        let json = r#"{"key":"test","description":"","node_type":"agent_node"}"#;
        let step: LifecycleStepDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(step.key, "test");
    }

    #[test]
    fn workflow_contract_capabilities_default_empty() {
        let json = r#"{}"#;
        let contract: WorkflowContract = serde_json::from_str(json).unwrap();
        assert!(contract.capabilities.is_empty());

        let back = serde_json::to_string(&contract).unwrap();
        assert!(!back.contains("capabilities"), "空 capabilities 不应出现在序列化结果中: {back}");
    }

    #[test]
    fn workflow_contract_capabilities_simple_roundtrip() {
        let contract = WorkflowContract {
            capabilities: vec![
                CapabilityEntry::simple("file_system"),
                CapabilityEntry::simple("workflow_management"),
            ],
            ..WorkflowContract::default()
        };
        let json = serde_json::to_string(&contract).unwrap();
        let back: WorkflowContract = serde_json::from_str(&json).unwrap();
        assert_eq!(back.capabilities, contract.capabilities);
    }

    #[test]
    fn workflow_contract_capabilities_legacy_string_compat() {
        // 旧格式 `["file_system", "workflow_management"]` 必须能反序列化
        let json = r#"{"capabilities":["file_system","workflow_management"]}"#;
        let contract: WorkflowContract = serde_json::from_str(json).unwrap();
        assert_eq!(contract.capabilities.len(), 2);
        assert_eq!(contract.capabilities[0].key(), "file_system");
    }

    #[test]
    fn workflow_contract_capabilities_mixed_format() {
        let json = r#"{"capabilities":["file_system",{"key":"file_read","exclude_tools":["fs_grep"]}]}"#;
        let contract: WorkflowContract = serde_json::from_str(json).unwrap();
        assert_eq!(contract.capabilities.len(), 2);
        assert!(!contract.capabilities[0].has_tool_filter());
        assert!(contract.capabilities[1].has_tool_filter());
        assert_eq!(contract.capabilities[1].exclude_tools(), &["fs_grep".to_string()]);
    }
}
