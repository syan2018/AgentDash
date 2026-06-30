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

export type BackendExecutionSelectionMode = import("../generated/backend-contracts").BackendExecutionSelectionMode;
export type BackendExecutionLeaseState = import("../generated/backend-contracts").BackendExecutionLeaseState;
export type BackendActiveSession = import("../generated/backend-contracts").BackendActiveSessionResponse;
export type BackendRuntimeExecutorSummary = import("../generated/backend-contracts").BackendRuntimeExecutorResponse;
export type BackendRuntimeSummary = import("../generated/backend-contracts").BackendRuntimeSummaryResponse;

export interface ViewConfig {
  id: string;
  name: string;
  backend_ids: string[];
  filters: Record<string, unknown>;
  sort_by: string | null;
}
