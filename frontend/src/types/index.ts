// ─── 基础枚举 ─────────────────────────────────────────

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
  | "assigned"
  | "running"
  | "awaiting_verification"
  | "completed"
  | "failed";

export type BackendType = "local" | "remote";
export type WorkspaceType = "git_worktree" | "static" | "ephemeral";
export type WorkspaceStatus = "pending" | "preparing" | "ready" | "active" | "archived" | "error";

// ─── Project ──────────────────────────────────────────

export interface AgentPreset {
  name: string;
  agent_type: string;
  config: Record<string, unknown>;
}

export interface ProjectConfig {
  default_agent_type?: string | null;
  default_workspace_id?: string | null;
  agent_presets: AgentPreset[];
}

export interface Project {
  id: string;
  name: string;
  description: string;
  backend_id: string;
  config: ProjectConfig;
  created_at: string;
  updated_at: string;
}

// ─── Workspace ────────────────────────────────────────

export interface GitConfig {
  source_repo: string;
  branch: string;
  commit_hash?: string | null;
}

export interface Workspace {
  id: string;
  project_id: string;
  name: string;
  container_ref: string;
  workspace_type: WorkspaceType;
  status: WorkspaceStatus;
  git_config?: GitConfig | null;
  created_at: string;
  updated_at: string;
}

// ─── Story ────────────────────────────────────────────

export interface ResourceRef {
  name: string;
  uri: string;
  resource_type: string;
}

export interface StoryContext {
  prd_doc?: string | null;
  spec_refs: string[];
  resource_list: ResourceRef[];
}

export interface Story {
  id: string;
  project_id: string;
  backend_id: string;
  title: string;
  description?: string;
  status: StoryStatus;
  context: StoryContext;
  created_at: string;
  updated_at: string;
}

// ─── Task ─────────────────────────────────────────────

export interface AgentBinding {
  agent_type?: string | null;
  agent_pid?: string | null;
  preset_name?: string | null;
  prompt_template?: string | null;
  initial_context?: string | null;
}

export interface Task {
  id: string;
  story_id: string;
  workspace_id?: string | null;
  session_id?: string | null;
  title: string;
  description?: string;
  status: TaskStatus;
  agent_binding: AgentBinding;
  artifacts: Artifact[];
  created_at: string;
  updated_at: string;
}

// ─── Artifact / ACP 展示类型 ──────────────────────────

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

// ─── Backend ──────────────────────────────────────────

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

// ─── SSE 事件 ──────────────────────────────────────────

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
