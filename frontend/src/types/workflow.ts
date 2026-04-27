// ─── Workflow ─────────────────────────────────────────

export type WorkflowTargetKind = "project" | "story";

export type WorkflowAgentRole = "project" | "story" | "task";

export type WorkflowRunStatus =
  | "draft"
  | "ready"
  | "running"
  | "blocked"
  | "completed"
  | "failed"
  | "cancelled";

export type WorkflowStepExecutionStatus =
  | "pending"
  | "ready"
  | "running"
  | "completed"
  | "failed"
  | "skipped";

export interface WorkflowContextBinding {
  locator: string;
  reason: string;
  required: boolean;
  title?: string | null;
}

export interface WorkflowInjectionSpec {
  goal?: string | null;
  instructions: string[];
  context_bindings: WorkflowContextBinding[];
}

export type StandaloneFulfillment = "none" | "text_input" | "file_upload";

export type WorkflowHookTrigger =
  | "user_prompt_submit"
  | "before_tool"
  | "after_tool"
  | "after_turn"
  | "before_stop"
  | "session_terminal"
  | "before_subagent_dispatch"
  | "after_subagent_dispatch"
  | "subagent_result"
  | "before_compact"
  | "after_compact"
  | "before_provider_request";

export interface WorkflowHookRuleSpec {
  key: string;
  trigger: WorkflowHookTrigger;
  description: string;
  preset?: string | null;
  params?: Record<string, unknown> | null;
  script?: string | null;
  enabled: boolean;
}

export interface HookRulePreset {
  key: string;
  trigger: WorkflowHookTrigger;
  label: string;
  description: string;
  param_schema?: Record<string, unknown> | null;
  script?: string;
  source?: "builtin" | "user_defined";
}

export type GateStrategy = "existence" | "schema" | "llm_judge";

export type ContextStrategy = "full" | "summary" | "metadata_only" | "custom";

export interface OutputPortDefinition {
  key: string;
  description: string;
  gate_strategy?: GateStrategy;
  gate_params?: Record<string, unknown> | null;
}

export interface InputPortDefinition {
  key: string;
  description: string;
  context_strategy?: ContextStrategy;
  context_template?: string | null;
  standalone_fulfillment?: StandaloneFulfillment;
}

export type LifecycleEdgeKind = "flow" | "artifact";

export type LifecycleEdge =
  | {
      kind: "flow";
      from_node: string;
      to_node: string;
      from_port?: null;
      to_port?: null;
    }
  | {
      kind: "artifact";
      from_node: string;
      to_node: string;
      from_port: string;
      to_port: string;
    };

/**
 * Capability 路径 —— 统一表达「能力级」和「工具级」两种寻址。
 *
 * - `capability` 是 capability key（如 `"file_read"` 或 `"mcp:code_analyzer"`）
 * - `tool` 为 `null` 表示短 path（整个能力），非空字符串表示长 path（能力下的某个工具）
 *
 * 分隔符统一为 `::`（与 Rust 模块路径同构），与 `mcp:<server>` 的单冒号前缀不冲突。
 *
 * JSON 形式序列化为 qualified string：`"file_read"` / `"file_read::fs_grep"`
 * / `"mcp:code_analyzer"` / `"mcp:workflow_management::upsert"`。
 */
export interface CapabilityPath {
  capability: string;
  /** null 表示短 path；非空字符串表示长 path（工具级） */
  tool: string | null;
}

/**
 * Workflow 级能力指令 —— 在 agent baseline 上 Add / Remove 能力或工具。
 *
 * JSON 形态（Rust serde externally-tagged enum, snake_case）：
 * ```json
 * { "add": "file_read" }
 * { "add": "file_read::fs_read" }
 * { "remove": "shell_execute" }
 * { "remove": "file_read::fs_grep" }
 * ```
 */
export type CapabilityDirective =
  | { add: string }
  | { remove: string };

const CAPABILITY_PATH_SEPARATOR = "::";

/** 序列化 CapabilityPath 为 qualified string —— `"cap"` 或 `"cap::tool"`。 */
export function toQualifiedString(path: CapabilityPath): string {
  return path.tool === null || path.tool === ""
    ? path.capability
    : `${path.capability}${CAPABILITY_PATH_SEPARATOR}${path.tool}`;
}

/**
 * 解析 qualified string —— 反向对应 `toQualifiedString`。
 *
 * 规则（与后端 Rust `CapabilityPath::parse` 对齐）：
 * - 空字符串 → throw
 * - 恰好一个 `::` → long path；两边均不得为空
 * - 多于一个 `::` → throw（不允许多级嵌套）
 * - 无 `::` → short path
 */
export function parseCapabilityPath(qualified: string): CapabilityPath {
  const trimmed = qualified.trim();
  if (trimmed.length === 0) {
    throw new Error("CapabilityPath 不能为空");
  }
  const parts = trimmed.split(CAPABILITY_PATH_SEPARATOR);
  if (parts.length === 1) {
    return { capability: trimmed, tool: null };
  }
  if (parts.length === 2) {
    const [cap, tool] = parts;
    if (cap.length === 0 || tool.length === 0) {
      throw new Error(`CapabilityPath 非法：${qualified}`);
    }
    return { capability: cap, tool };
  }
  throw new Error(`CapabilityPath 不允许多级嵌套：${qualified}`);
}

/** 提取 Directive 携带的 qualified path 字符串。 */
export function directivePath(directive: CapabilityDirective): string {
  return "add" in directive ? directive.add : directive.remove;
}

/** 判定 Directive 种类。 */
export function directiveKind(directive: CapabilityDirective): "add" | "remove" {
  return "add" in directive ? "add" : "remove";
}

/**
 * 平台 well-known capability key 常量。
 *
 * - `file_read` / `file_write` / `shell_execute` 是细粒度文件系统能力
 * - 所有 key 的权威列表以后端 `crates/agentdash-spi/src/tool_capability.rs`
 *   的 WELL_KNOWN_KEYS 为准
 */
export const WELL_KNOWN_CAPABILITY_KEYS = [
  "file_read",
  "file_write",
  "shell_execute",
  "canvas",
  "workflow",
  "collaboration",
  "story_management",
  "task_management",
  "relay_management",
  "workflow_management",
] as const;

// ─── Tool Descriptor（统一工具元数据）──────────────────

export type ToolSourceType = "platform" | "platform_mcp" | "mcp";

/** 平台 MCP scope —— 与后端 `PlatformMcpScope` 对齐。 */
export type PlatformMcpScope = "relay" | "story" | "task" | "workflow";

export interface ToolDescriptor {
  name: string;
  display_name: string;
  description: string;
  source:
    | { type: "platform"; cluster: string }
    | { type: "platform_mcp"; scope: PlatformMcpScope }
    | { type: "mcp"; server_name: string };
  capability_key: string;
}

// ─── Workflow Contract ─────────────────────────────────

export interface WorkflowContract {
  injection: WorkflowInjectionSpec;
  hook_rules: WorkflowHookRuleSpec[];
  /**
   * Workflow 级能力指令序列 —— 在 agent baseline 上 Add / Remove 能力或工具。
   *
   * 每条指令：
   * - `{ add: "<path>" }` —— 追加能力（短 path）或启用某个工具（长 path）
   * - `{ remove: "<path>" }` —— 屏蔽能力（短 path）或屏蔽某个工具（长 path）
   *
   * Path 语法：
   * - 短 path（能力级）：`"file_read"` / `"mcp:code_analyzer"`
   * - 长 path（工具级）：`"file_read::fs_grep"` / `"mcp:workflow_management::upsert"`
   *
   * 运行时 hook 可叠加 delta 指令；后端 `compute_effective_capabilities` 走同一条归约路径。
   */
  capability_directives: CapabilityDirective[];
  /** Output ports — 同时作为完成门禁（全部交付才可推进） */
  output_ports: OutputPortDefinition[];
  /** Input ports — 声明本 workflow 所需的外部数据 */
  input_ports: InputPortDefinition[];
}

export type WorkflowDefinitionSource =
  | "builtin_seed"
  | "user_authored"
  | "cloned";

export type ValidationSeverity = "error" | "warning";

export interface ValidationIssue {
  code: string;
  message: string;
  field_path: string;
  severity: ValidationSeverity;
}

export interface WorkflowValidationResult {
  valid: boolean;
  issues: ValidationIssue[];
}

export interface WorkflowTemplateWorkflow {
  key: string;
  name: string;
  description: string;
  contract: WorkflowContract;
}

export type LifecycleNodeType = "agent_node" | "phase_node";

export interface LifecycleStepDefinition {
  key: string;
  description: string;
  workflow_key?: string | null;
  node_type?: LifecycleNodeType;
  /** Step 级产出约束 */
  output_ports: OutputPortDefinition[];
  /** Step 级消费声明 */
  input_ports: InputPortDefinition[];
}

export interface WorkflowTemplate {
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_roles: WorkflowAgentRole[];
  workflows: WorkflowTemplateWorkflow[];
  lifecycle: {
    key: string;
    name: string;
    description: string;
    entry_step_key: string;
    steps: LifecycleStepDefinition[];
    edges: LifecycleEdge[];
  };
}

export interface WorkflowDefinition {
  id: string;
  project_id: string;
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_roles: WorkflowAgentRole[];
  source: WorkflowDefinitionSource;
  version: number;
  contract: WorkflowContract;
  created_at: string;
  updated_at: string;
}

export interface LifecycleDefinition {
  id: string;
  project_id: string;
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_roles: WorkflowAgentRole[];
  source: WorkflowDefinitionSource;
  version: number;
  entry_step_key: string;
  steps: LifecycleStepDefinition[];
  edges: LifecycleEdge[];
  created_at: string;
  updated_at: string;
}

export interface WorkflowStepState {
  step_key: string;
  status: WorkflowStepExecutionStatus;
  session_id?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  summary?: string | null;
  context_snapshot?: Record<string, unknown> | null;
  gate_collision_count?: number;
}

export type LifecycleExecutionEventKind =
  | "step_activated"
  | "step_completed"
  | "artifact_appended"
  | "context_injected";

export interface LifecycleExecutionEntry {
  timestamp: string;
  step_key: string;
  event_kind: LifecycleExecutionEventKind;
  summary: string;
  detail?: Record<string, unknown> | null;
}

export interface WorkflowRun {
  id: string;
  project_id: string;
  lifecycle_id: string;
  /** 父 session ID — lifecycle run 跟着 session 走 */
  session_id: string;
  status: WorkflowRunStatus;
  /** 当前所有可执行（Ready/Running）的 node key 集合 */
  active_node_keys?: string[];
  step_states: WorkflowStepState[];
  execution_log: LifecycleExecutionEntry[];
  created_at: string;
  updated_at: string;
  last_activity_at: string;
}
