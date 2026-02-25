export type StoryStatus =
  | "draft"
  | "ready"
  | "running"
  | "review"
  | "completed"
  | "failed"
  | "cancelled";

export type TaskStatus =
  | "pending"
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "skipped"
  | "cancelled";

export type AgentType = "session" | "planner" | "worker" | "reviewer" | "researcher";
export type BackendType = "local" | "remote";

export interface ContextItem {
  id: string;
  sourceKind: "spec" | "skill" | "preset" | "content" | "practice" | "agent_output";
  reference: string;
  reason: string;
  displayName?: string;
  summary?: string;
}

export interface Context {
  id?: string;
  name?: string;
  description?: string;
  items?: ContextItem[];
  metadata?: Record<string, unknown>;
}

export interface AgentBinding {
  agentType: AgentType;
  agentPid?: string | null;
  workspacePath?: string | null;
}

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "image"; data: string; mimeType: string }
  | { type: "resource_link"; uri: string; name: string; description?: string; mimeType?: string; size?: number }
  | { type: "resource"; resource: { uri: string; mimeType?: string; text?: string } };

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

export type Artifact =
  | { type: "text"; title?: string; content: string }
  | { type: "json"; title?: string; data: unknown }
  | { type: "content_block"; title?: string; blocks: ContentBlock[] };

export interface Story {
  id: string;
  backendId: string;
  title: string;
  description?: string;
  status: StoryStatus;
  context: Context;
  taskIds: string[];
  createdAt: string;
  updatedAt: string;
}

export interface Task {
  id: string;
  storyId: string;
  title: string;
  description?: string;
  agentType: AgentType;
  status: TaskStatus;
  context: Context;
  agentBinding: AgentBinding | null;
  artifacts: Artifact[];
  executionTrace: SessionUpdate[];
  createdAt: string;
  updatedAt: string;
}

export interface BackendConfig {
  id: string;
  name: string;
  endpoint: string;
  auth_token: string | null;
  enabled: boolean;
  backend_type: BackendType;
}

export interface ViewConfig {
  id: string;
  name: string;
  backend_ids: string[];
  filters: Record<string, unknown>;
  sort_by: string | null;
}

export interface StateChange {
  id: number;
  entity_id: string;
  kind: string;
  payload: Record<string, unknown>;
  backend_id: string;
  created_at: string;
}

export type StreamEvent =
  | { type: "Connected"; data: { last_event_id: number } }
  | { type: "StateChanged"; data: StateChange }
  | { type: "Heartbeat"; data: { timestamp: number } };
