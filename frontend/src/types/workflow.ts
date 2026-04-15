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

export type WorkflowRecordArtifactType =
  | "session_summary"
  | "journal_update"
  | "archive_suggestion"
  | "phase_note"
  | "checklist_evidence"
  | "execution_trace"
  | "decision_record"
  | "context_snapshot";

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
  default_artifact_type?: WorkflowRecordArtifactType | null;
  default_artifact_title?: string | null;
}

export type WorkflowHookTrigger =
  | "before_tool"
  | "after_tool"
  | "after_turn"
  | "before_stop"
  | "session_terminal"
  | "before_subagent_dispatch"
  | "after_subagent_dispatch"
  | "subagent_result";

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

export interface WorkflowContract {
  injection: WorkflowInjectionSpec;
  hook_rules: WorkflowHookRuleSpec[];
  constraints: WorkflowConstraintSpec[];
  completion: WorkflowCompletionSpec;
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
  /** DAG 依赖：前驱 node key 列表。空数组表示无依赖。 */
  depends_on?: string[];
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
}

export interface WorkflowRecordArtifact {
  id: string;
  step_key: string;
  artifact_type: WorkflowRecordArtifactType;
  title: string;
  content: string;
  created_at: string;
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
  /** 兼容字段：线性模式下的当前活跃 step key */
  current_step_key?: string | null;
  /** DAG 模式下所有当前可执行的 node key 集合 */
  active_node_keys?: string[];
  step_states: WorkflowStepState[];
  record_artifacts: WorkflowRecordArtifact[];
  execution_log: LifecycleExecutionEntry[];
  created_at: string;
  updated_at: string;
  last_activity_at: string;
}