// ─── SessionBinding ─────────────────────────────────

export type SessionOwnerType = "project" | "story" | "task";

export interface SessionBinding {
  id: string;
  project_id: string;
  session_id: string;
  owner_type: SessionOwnerType;
  owner_id: string;
  label: string;
  created_at: string;
  session_title?: string;
  session_updated_at?: number;
}

export interface SessionTaskContext {
  task_id: string;
  agent_binding?: AgentBinding;
}

export type SessionReturnTarget =
  | {
      owner_type: "project";
      project_id: string;
    }
  | {
      owner_type: "story";
      story_id: string;
    }
  | {
      owner_type: "task";
      story_id: string;
      task_id: string;
    };

export interface SessionBindingOwner {
  id: string;
  project_id: string;
  session_id: string;
  owner_type: SessionOwnerType;
  owner_id: string;
  label: string;
  created_at: string;
  owner_title?: string | null;
  story_id?: string | null;
  task_id?: string | null;
}

export interface HookOwnerSummary {
  owner_type: string;
  owner_id: string;
  label?: string | null;
  project_id?: string | null;
  story_id?: string | null;
  task_id?: string | null;
}

export interface HookInjection {
  slot: string;
  content: string;
  source: string;
}

export interface HookDiagnosticEntry {
  code: string;
  message: string;
}

export interface HookCompletionStatus {
  mode: string;
  satisfied: boolean;
  advanced: boolean;
  reason: string;
}

export interface HookTraceEntry {
  sequence: number;
  timestamp_ms: number;
  revision: number;
  trigger: string;
  decision: string;
  tool_name?: string | null;
  tool_call_id?: string | null;
  subagent_type?: string | null;
  matched_rule_keys: string[];
  refresh_snapshot: boolean;
  block_reason?: string | null;
  completion?: HookCompletionStatus | null;
  diagnostics: HookDiagnosticEntry[];
}

export interface HookPendingAction {
  id: string;
  created_at_ms: number;
  title: string;
  summary: string;
  action_type: string;
  turn_id?: string | null;
  source_trigger: string;
  status: "pending" | "resolved";
  last_injected_at_ms?: number | null;
  resolved_at_ms?: number | null;
  resolution_kind?: "adopted" | "dismissed" | null;
  resolution_note?: string | null;
  resolution_turn_id?: string | null;
  injections: HookInjection[];
}

export type SessionExecutionStatus = "idle" | "running" | "completed" | "failed" | "interrupted";

export interface SessionExecutionState {
  session_id: string;
  status: SessionExecutionStatus;
  turn_id?: string | null;
  message?: string | null;
}

export interface ActiveWorkflowHookMetadata {
  lifecycle_id: string;
  lifecycle_key: string;
  lifecycle_name: string;
  run_id: string;
  run_status: string;
  step_key: string;
  step_title: string;
  primary_workflow_id: string;
  /** Bound workflow key when step is workflow-driven; omit or null for manual steps. */
  workflow_key?: string | null;
  /** @deprecated Prefer workflow_key; may still appear from older API payloads. */
  primary_workflow_key?: string | null;
  primary_workflow_name: string;
}

export interface HookRuntimeMetadata {
  active_workflow?: ActiveWorkflowHookMetadata | null;
}

export interface SessionHookSnapshot {
  session_id: string;
  owners: HookOwnerSummary[];
  sources: string[];
  tags: string[];
  injections: HookInjection[];
  diagnostics: HookDiagnosticEntry[];
  metadata?: HookRuntimeMetadata | null;
}

export interface HookSessionRuntimeInfo {
  session_id: string;
  revision: number;
  snapshot: SessionHookSnapshot;
  diagnostics: HookDiagnosticEntry[];
  trace: HookTraceEntry[];
  pending_actions: HookPendingAction[];
}

export interface ProjectSessionAgentContext {
  agent_key: string;
  display_name: string;
  executor_hint?: string | null;
}

export interface SessionNavigationState {
  task_context?: SessionTaskContext;
  project_agent?: ProjectSessionAgentContext;
  return_to?: SessionReturnTarget;
}

export interface StoryNavigationState {
  open_task_id?: string;
}