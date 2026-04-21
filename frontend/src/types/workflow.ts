// ─── Workflow ─────────────────────────────────────────

export type WorkflowTargetKind = "project" | "story" | "task";

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

export type WorkflowConstraintKind =
  | "block_stop_until_checks_pass"
  | "custom";

export type WorkflowCheckKind =
  | "artifact_exists"
  | "artifact_count_gte"
  | "session_terminal_in"
  | "checklist_evidence_present"
  | "explicit_action_received"
  | "custom";

export interface WorkflowContextBinding {
  locator: string;
  reason: string;
  required: boolean;
  title?: string | null;
}

export interface WorkflowConstraintSpec {
  key: string;
  kind: WorkflowConstraintKind;
  description: string;
  payload?: Record<string, unknown> | null;
}

export interface WorkflowCheckSpec {
  key: string;
  kind: WorkflowCheckKind;
  description: string;
  payload?: Record<string, unknown> | null;
}

export interface WorkflowInjectionSpec {
  goal?: string | null;
  instructions: string[];
  context_bindings: WorkflowContextBinding[];
}

export interface WorkflowCompletionSpec {
  checks: WorkflowCheckSpec[];
}

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
 * 工具能力的结构化声明。
 *
 * 支持简写（纯 key string）和结构化（带工具级裁剪 object）两种形式。
 */
export type CapabilityEntry =
  | string
  | {
      key: string;
      /** 白名单：仅启用此 capability 下属的指定工具。为空数组或省略表示启用全部。 */
      include_tools?: string[];
      /** 黑名单：从此 capability 下属工具中排除指定工具。 */
      exclude_tools?: string[];
    };

/** 提取 CapabilityEntry 的 key */
export function capabilityEntryKey(entry: CapabilityEntry): string {
  return typeof entry === "string" ? entry : entry.key;
}

/**
 * 平台 well-known capability key 常量。
 *
 * - `file_system` 是别名，自动展开为 file_read + file_write + shell_execute
 * - `file_read` / `file_write` / `shell_execute` 是拆分后的细粒度 key
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

export const CAPABILITY_ALIASES: Record<string, string[]> = {
  file_system: ["file_read", "file_write", "shell_execute"],
};

// ─── Tool Descriptor（统一工具元数据）──────────────────

export type ToolSourceType = "platform" | "mcp";

export interface ToolDescriptor {
  name: string;
  display_name: string;
  description: string;
  source:
    | { type: "platform"; cluster: string }
    | { type: "mcp"; server_name: string };
  capability_key: string;
}

// ─── Workflow Contract ─────────────────────────────────

export interface WorkflowContract {
  injection: WorkflowInjectionSpec;
  hook_rules: WorkflowHookRuleSpec[];
  constraints: WorkflowConstraintSpec[];
  completion: WorkflowCompletionSpec;
  /**
   * Workflow 级基线能力集合。
   *
   * 每个条目可以是：
   * - 简写形式：`"file_read"` — 启用整个能力的全部工具
   * - 结构化形式：`{ key: "file_read", exclude_tools: ["fs_grep"] }` — 带工具级裁剪
   * - 平台别名：`"file_system"` — 自动展开为 file_read + file_write + shell_execute
   * - 自定义 MCP：`"mcp:<preset_name>"` — 指向 project 级 McpPreset
   */
  capabilities: CapabilityEntry[];
  /** 推荐 ports（模板用途，运行时产出约束由 step 级 ports 定义） */
  recommended_output_ports?: OutputPortDefinition[];
  recommended_input_ports?: InputPortDefinition[];
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
  | "constraint_blocked"
  | "completion_evaluated"
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