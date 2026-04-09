import { api, type ApiHttpError } from "../api/client";
import { requireStringField, requireNumberField } from "../api/mappers";
import type { SessionNotification } from "@agentclientprotocol/sdk";
import type {
  AgentBinding,
  ContextSourceRef,
  ExecutionAddressSpace,
  HookSessionRuntimeInfo,
  ProjectSessionEntry,
  SessionBindingOwner,
  SessionContextSnapshot,
  SessionExecutionState,
  SessionExecutionStatus,
} from "../types";
import { isThinkingLevel } from "../types";

function normalizeSessionBindingOwnerType(value: unknown): SessionBindingOwner["owner_type"] {
  switch (value) {
    case "project":
    case "story":
    case "task":
      return value;
    default:
      throw new Error(`未知的 session owner_type: ${String(value ?? "")}`);
  }
}

function mapSessionBindingOwner(raw: Record<string, unknown>): SessionBindingOwner {
  return {
    id: requireStringField(raw, "id"),
    session_id: requireStringField(raw, "session_id"),
    owner_type: normalizeSessionBindingOwnerType(raw.owner_type),
    owner_id: requireStringField(raw, "owner_id"),
    label: requireStringField(raw, "label"),
    created_at: requireStringField(raw, "created_at"),
    owner_title:
      raw.owner_title != null
        ? String(raw.owner_title)
        : null,
    project_id:
      String(raw.project_id ?? ""),
    story_id:
      raw.story_id != null
        ? String(raw.story_id)
        : null,
    task_id:
      raw.task_id != null
        ? String(raw.task_id)
        : null,
  };
}

export interface SessionMeta {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  lastEventSeq?: number;
}

export interface PersistedSessionEvent {
  session_id: string;
  event_seq: number;
  occurred_at_ms: number;
  committed_at_ms: number;
  session_update_type: string;
  turn_id: string | null;
  entry_index: number | null;
  tool_call_id: string | null;
  notification: SessionNotification;
}

export interface SessionEventsPage {
  snapshot_seq: number;
  events: PersistedSessionEvent[];
  has_more: boolean;
  next_after_seq: number;
}

export interface FetchSessionsOptions {
  excludeBound?: boolean;
}

export async function fetchSessions(options?: FetchSessionsOptions): Promise<SessionMeta[]> {
  const params = new URLSearchParams();
  if (options?.excludeBound) {
    params.set("exclude_bound", "true");
  }
  const query = params.toString();
  const path = query ? `/sessions?${query}` : "/sessions";
  return api.get<SessionMeta[]>(path);
}

export async function createSession(title?: string): Promise<SessionMeta> {
  return api.post<SessionMeta>("/sessions", { title });
}

export async function fetchSessionMeta(id: string): Promise<SessionMeta> {
  return api.get<SessionMeta>(`/sessions/${encodeURIComponent(id)}`);
}

function mapPersistedSessionEvent(raw: Record<string, unknown>): PersistedSessionEvent {
  return {
    session_id: requireStringField(raw, "session_id"),
    event_seq: requireNumberField(raw, "event_seq"),
    occurred_at_ms: requireNumberField(raw, "occurred_at_ms"),
    committed_at_ms: requireNumberField(raw, "committed_at_ms"),
    session_update_type: requireStringField(raw, "session_update_type"),
    turn_id: raw.turn_id != null ? String(raw.turn_id) : null,
    entry_index: raw.entry_index != null ? Number(raw.entry_index) : null,
    tool_call_id: raw.tool_call_id != null ? String(raw.tool_call_id) : null,
    notification: raw.notification as SessionNotification,
  };
}

export async function fetchSessionEvents(
  sessionId: string,
  afterSeq = 0,
  limit = 500,
): Promise<SessionEventsPage> {
  const params = new URLSearchParams();
  params.set("after_seq", String(afterSeq));
  params.set("limit", String(limit));
  const raw = await api.get<Record<string, unknown>>(
    `/sessions/${encodeURIComponent(sessionId)}/events?${params.toString()}`,
  );
  if (!Array.isArray(raw.events)) {
    throw new Error("会话事件响应缺少 events 数组");
  }
  const eventList = raw.events as Record<string, unknown>[];
  return {
    snapshot_seq: requireNumberField(raw, "snapshot_seq"),
    events: eventList.map(mapPersistedSessionEvent),
    has_more: Boolean(raw.has_more),
    next_after_seq: requireNumberField(raw, "next_after_seq"),
  };
}

export async function deleteSession(id: string): Promise<void> {
  await api.delete<void>(`/sessions/${encodeURIComponent(id)}`);
}

function mapSessionContextAgentBinding(raw: unknown): AgentBinding | null {
  if (raw == null || typeof raw !== "object") {
    return null;
  }
  const binding = raw as Record<string, unknown>;
  return {
    agent_type: binding.agent_type != null ? String(binding.agent_type) : null,
    agent_pid: binding.agent_pid != null ? String(binding.agent_pid) : null,
    preset_name: binding.preset_name != null ? String(binding.preset_name) : null,
    prompt_template: binding.prompt_template != null ? String(binding.prompt_template) : null,
    initial_context: binding.initial_context != null ? String(binding.initial_context) : null,
    thinking_level:
      binding.thinking_level == null
        ? null
        : isThinkingLevel(binding.thinking_level)
          ? binding.thinking_level
          : (() => {
              throw new Error(`未知的 thinking_level: ${String(binding.thinking_level)}`);
            })(),
    context_sources: Array.isArray(binding.context_sources)
      ? binding.context_sources as ContextSourceRef[]
      : [],
  };
}

export interface SessionContextPayload {
  workspace_id: string | null;
  agent_binding: AgentBinding | null;
  address_space: ExecutionAddressSpace | null;
  context_snapshot: SessionContextSnapshot | null;
}

/** GET /sessions/{id}/context — 与旧版 task/story/project 分端点行为对齐（由后端按绑定解析） */
export async function fetchSessionContext(sessionId: string): Promise<SessionContextPayload | null> {
  let raw: Record<string, unknown>;
  try {
    raw = await api.get<Record<string, unknown>>(`/sessions/${encodeURIComponent(sessionId)}/context`);
  } catch (err) {
    if ((err as ApiHttpError).status === 404) return null;
    throw err;
  }
  return {
    workspace_id: raw.workspace_id != null ? String(raw.workspace_id) : null,
    agent_binding: mapSessionContextAgentBinding(raw.agent_binding),
    address_space: (raw.address_space as ExecutionAddressSpace | undefined) ?? null,
    context_snapshot: (raw.context_snapshot as SessionContextSnapshot | undefined) ?? null,
  };
}

export async function fetchSessionBindings(id: string): Promise<SessionBindingOwner[]> {
  const raw = await api.get<Record<string, unknown>[]>(`/sessions/${encodeURIComponent(id)}/bindings`);
  return raw.map(mapSessionBindingOwner);
}

export async function fetchSessionHookRuntime(id: string): Promise<HookSessionRuntimeInfo | null> {
  try {
    return await api.get<HookSessionRuntimeInfo>(`/sessions/${encodeURIComponent(id)}/hook-runtime`);
  } catch (err) {
    if ((err as ApiHttpError).status === 404) return null;
    throw err;
  }
}

function normalizeSessionExecutionStatus(value: unknown): SessionExecutionStatus {
  switch (value) {
    case "idle":
    case "running":
    case "completed":
    case "failed":
    case "interrupted":
      return value;
    default:
      throw new Error(`未知的会话执行状态: ${String(value ?? "")}`);
  }
}

function mapSessionExecutionState(raw: Record<string, unknown>): SessionExecutionState {
  return {
    session_id: requireStringField(raw, "session_id"),
    status: normalizeSessionExecutionStatus(raw.status),
    turn_id: raw.turn_id != null ? String(raw.turn_id) : null,
    message: raw.message != null ? String(raw.message) : null,
  };
}

export async function fetchSessionExecutionState(id: string): Promise<SessionExecutionState> {
  const raw = await api.get<Record<string, unknown>>(`/sessions/${encodeURIComponent(id)}/state`);
  return mapSessionExecutionState(raw);
}

export async function cancelSession(id: string): Promise<void> {
  await api.post<void>(`/sessions/${encodeURIComponent(id)}/cancel`, {});
}

// ─── 项目会话列表 ─────────────────────────────────────
// 获取指定项目的所有活跃会话（包含 agent / story / task 层级）

function normalizeProjectSessionEntryStatus(
  value: unknown,
): ProjectSessionEntry["execution_status"] {
  switch (value) {
    case "idle":
    case "running":
    case "completed":
    case "failed":
    case "interrupted":
      return value;
    default:
      throw new Error(`未知的项目会话执行状态: ${String(value ?? "")}`);
  }
}

function normalizeOwnerType(value: unknown): ProjectSessionEntry["owner_type"] {
  switch (value) {
    case "project":
    case "story":
    case "task":
      return value;
    default:
      throw new Error(`未知的项目会话 owner_type: ${String(value ?? "")}`);
  }
}

function mapProjectSessionEntry(raw: Record<string, unknown>): ProjectSessionEntry {
  return {
    session_id: requireStringField(raw, "session_id"),
    session_title: raw.session_title != null ? String(raw.session_title) : null,
    last_activity: raw.last_activity != null ? Number(raw.last_activity) : null,
    execution_status: normalizeProjectSessionEntryStatus(raw.execution_status),
    owner_type: normalizeOwnerType(raw.owner_type),
    owner_id: requireStringField(raw, "owner_id"),
    owner_title: raw.owner_title != null ? String(raw.owner_title) : null,
    story_id: raw.story_id != null ? String(raw.story_id) : null,
    story_title: raw.story_title != null ? String(raw.story_title) : null,
    agent_key: raw.agent_key != null ? String(raw.agent_key) : null,
    agent_display_name: raw.agent_display_name != null ? String(raw.agent_display_name) : null,
    parent_session_id: raw.parent_session_id != null ? String(raw.parent_session_id) : null,
  };
}

export async function fetchProjectSessions(projectId: string): Promise<ProjectSessionEntry[]> {
  const raw = await api.get<Record<string, unknown>[]>(`/projects/${encodeURIComponent(projectId)}/sessions`);
  return raw.map(mapProjectSessionEntry);
}
