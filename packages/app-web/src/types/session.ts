import type { AgentBinding } from "./index";

// ─── Session Types ─────────────────────────────────

export type CapabilityScope = "project" | "story" | "task";

export interface SessionRunContext {
  project_id: string;
  story_id?: string | null;
  task_id?: string | null;
  story_title?: string | null;
  task_title?: string | null;
  scope: CapabilityScope;
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
  source: string;
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
  activity_key: string;
  activity_title: string;
  primary_workflow_id: string;
  workflow_key?: string | null;
  primary_workflow_name: string;
}

export interface HookRuntimeMetadata {
  active_workflow?: ActiveWorkflowHookMetadata | null;
}

export interface SessionHookSnapshot {
  session_id: string;
  run_context?: SessionRunContext | null;
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
  open_workspace_panel?: boolean;
}

export interface StoryNavigationState {
  open_task_id?: string;
}
