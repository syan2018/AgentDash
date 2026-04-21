import type { BackendType } from "./index";

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
  /** WebSocket 中继在线状态（由 API 附加） */
  online?: boolean;
  /** 在线后端的可访问根路径 */
  accessible_roots?: string[];
  /** 在线后端的执行器能力 */
  capabilities?: {
    executors: Array<{
      id: string;
      name: string;
      variants: string[];
      available: boolean;
    }>;
    supports_cancel: boolean;
    supports_discover_options: boolean;
  };
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
  | { type: "Heartbeat"; data: { timestamp: number } };

// ─── 项目活跃会话条目 ──────────────────────────────────
// 对应后端 GET /api/projects/{id}/sessions 响应体中的单条记录

export interface ProjectSessionEntry {
  session_id: string;
  session_title: string | null;
  last_activity: number | null;
  execution_status: "idle" | "running" | "completed" | "failed" | "interrupted";
  owner_type: "project" | "story" | "task";
  owner_id: string;
  owner_title: string | null;
  story_id: string | null;
  story_title: string | null;
  agent_key: string | null;
  agent_display_name: string | null;
  parent_session_id: string | null;
}