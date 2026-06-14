// ─── Artifact / Session 展示类型 ──────────────────────────

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "image"; data: string; mimeType: string }
  | { type: "resource_link"; uri: string; name: string; description?: string; mimeType?: string; size?: number }
  | { type: "resource"; resource: { uri: string; mimeType?: string; text?: string } };

export type ArtifactType =
  | "code_change"
  | "test_result"
  | "log_output"
  | "file"
  | "tool_execution";

export interface Artifact {
  id: string;
  artifact_type: ArtifactType;
  content: unknown;
  created_at: string;
}

export interface ToolCall {
  title: string;
  kind:
    | "read"
    | "edit"
    | "delete"
    | "move"
    | "search"
    | "execute"
    | "think"
    | "fetch"
    | "switch_mode"
    | "other";
  status?: "pending" | "in_progress" | "completed" | "failed";
  rawInput?: unknown;
  rawOutput?: unknown;
}

export interface PlanEntry {
  content: string;
  priority: "high" | "medium" | "low";
  status: "pending" | "in_progress" | "completed";
}

export interface ConfirmationRequest {
  stagedTaskId: string;
  title: string;
  description?: string;
  requestKind: string;
  createdAt: string;
  projectId?: string;
}

export type SessionUpdate =
  | { type: "content"; blocks: ContentBlock[] }
  | { type: "tool_call"; toolCall: ToolCall }
  | { type: "plan"; entries: PlanEntry[] }
  | { type: "confirmation_request"; request: ConfirmationRequest };

export type RuntimeHealthStatus = import("../generated/backend-contracts").RuntimeHealthStatus;
export type RuntimeHealth = import("../generated/backend-contracts").BackendRuntimeHealthResponse;

export type BackendExecutionSelectionMode = "explicit" | "auto_idle" | "workspace_binding";
export type BackendExecutionLeaseState = "claimed" | "running" | "released" | "lost" | "failed";

export interface BackendActiveSession {
  lease_id: string;
  session_id: string;
  turn_id: string;
  executor_id: string;
  workspace_id: string | null;
  root_ref: string | null;
  selection_mode: BackendExecutionSelectionMode;
  state: BackendExecutionLeaseState;
  claimed_at: string;
  activated_at: string | null;
  last_seen_at: string;
}

export interface BackendRuntimeExecutorSummary {
  executor_id: string;
  name: string;
  variants: string[];
  available: boolean;
  active_session_count: number;
  allocatable: boolean;
}

export interface BackendRuntimeSummary {
  backend_id: string;
  name: string;
  enabled: boolean;
  online: boolean;
  runtime_health: RuntimeHealth | null;
  executors: BackendRuntimeExecutorSummary[];
  active_session_count: number;
  active_sessions: BackendActiveSession[];
  allocatable: boolean;
}

export interface ViewConfig {
  id: string;
  name: string;
  backend_ids: string[];
  filters: Record<string, unknown>;
  sort_by: string | null;
}

// ─── 项目事件流 ────────────────────────────────────────

export interface StateChange {
  id: number;
  project_id: string;
  entity_id: string;
  kind: string;
  payload: Record<string, unknown>;
  backend_id: string | null;
  created_at: string;
}

export type StreamEvent =
  | { type: "Connected"; data: { last_event_id: number } }
  | { type: "StateChanged"; data: StateChange }
  | { type: "BackendRuntimeChanged"; data: { backend_id: string } }
  | { type: "Heartbeat"; data: { timestamp: number } };
