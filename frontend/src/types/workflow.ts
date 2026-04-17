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

export interface LifecycleEdge {
  from_node: string;
  from_port: string;
  to_node: string;
  to_port: string;
}

export interface WorkflowContract {
  injection: WorkflowInjectionSpec;
  hook_rules: WorkflowHookRuleSpec[];
  constraints: WorkflowConstraintSpec[];
  completion: WorkflowCompletionSpec;
  /** 推荐 ports（模板用途，运行时产出约束由 step 级 ports 定义） */
  recommended_output_ports?: OutputPortDefinition[];
  recommended_input_ports?: InputPortDefinition[];
}

export type WorkflowDefinitionSource =
  | "builtin_seed"
  | "user_authored"
  | "cloned";

export type WorkflowDefinitionStatus =
  | "draft"
  | "active"
  | "disabled";

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
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_roles: WorkflowAgentRole[];
  source: WorkflowDefinitionSource;
  status: WorkflowDefinitionStatus;
  version: number;
  contract: WorkflowContract;
  created_at: string;
  updated_at: string;
}

export interface LifecycleDefinition {
  id: string;
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_roles: WorkflowAgentRole[];
  source: WorkflowDefinitionSource;
  status: WorkflowDefinitionStatus;
  version: number;
  entry_step_key: string;
  steps: LifecycleStepDefinition[];
  edges: LifecycleEdge[];
  created_at: string;
  updated_at: string;
}

export interface WorkflowAssignment {
  id: string;
  project_id: string;
  lifecycle_id: string;
  role: WorkflowAgentRole;
  enabled: boolean;
  is_default: boolean;
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