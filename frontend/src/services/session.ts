import { buildApiPath } from "../api/origin";
import { authenticatedFetch } from "../api/client";
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

function mapSessionBindingOwner(raw: Record<string, unknown>): SessionBindingOwner {
  return {
    id: String(raw.id ?? ""),
    session_id: String(raw.session_id ?? ""),
    owner_type: String(raw.owner_type ?? "story") as SessionBindingOwner["owner_type"],
    owner_id: String(raw.owner_id ?? ""),
    label: String(raw.label ?? ""),
    created_at: String(raw.created_at ?? new Date().toISOString()),
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
  const url = query ? `${buildApiPath("/sessions")}?${query}` : buildApiPath("/sessions");
  const res = await authenticatedFetch(url);
  if (!res.ok) throw new Error(`获取会话列表失败: HTTP ${res.status}`);
  return res.json();
}

export async function createSession(title?: string): Promise<SessionMeta> {
  const res = await authenticatedFetch(buildApiPath("/sessions"), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ title }),
  });
  if (!res.ok) throw new Error(`创建会话失败: HTTP ${res.status}`);
  return res.json();
}

export async function fetchSessionMeta(id: string): Promise<SessionMeta> {
  const res = await authenticatedFetch(buildApiPath(`/sessions/${encodeURIComponent(id)}`));
  if (!res.ok) throw new Error(`获取会话详情失败: HTTP ${res.status}`);
  return res.json();
}

function mapPersistedSessionEvent(raw: Record<string, unknown>): PersistedSessionEvent {
  return {
    session_id: String(raw.session_id ?? ""),
    event_seq: Number(raw.event_seq ?? 0),
    occurred_at_ms: Number(raw.occurred_at_ms ?? 0),
    committed_at_ms: Number(raw.committed_at_ms ?? 0),
    session_update_type: String(raw.session_update_type ?? ""),
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
  const res = await authenticatedFetch(
    `${buildApiPath(`/sessions/${encodeURIComponent(sessionId)}/events`)}?${params.toString()}`,
  );
  if (!res.ok) throw new Error(`获取会话事件失败: HTTP ${res.status}`);
  const raw = await res.json() as Record<string, unknown>;
  const eventList = Array.isArray(raw.events) ? raw.events as Record<string, unknown>[] : [];
  return {
    snapshot_seq: Number(raw.snapshot_seq ?? afterSeq),
    events: eventList.map(mapPersistedSessionEvent),
    has_more: Boolean(raw.has_more),
    next_after_seq: Number(raw.next_after_seq ?? afterSeq),
  };
}

export async function deleteSession(id: string): Promise<void> {
  const res = await authenticatedFetch(buildApiPath(`/sessions/${encodeURIComponent(id)}`), {
    method: "DELETE",
  });
  if (!res.ok) throw new Error(`删除会话失败: HTTP ${res.status}`);
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
    thinking_level: isThinkingLevel(binding.thinking_level) ? binding.thinking_level : null,
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
  try {
    const res = await authenticatedFetch(buildApiPath(`/sessions/${encodeURIComponent(sessionId)}/context`));
    if (!res.ok) return null;
    const raw = (await res.json()) as Record<string, unknown>;
    return {
      workspace_id: raw.workspace_id != null ? String(raw.workspace_id) : null,
      agent_binding: mapSessionContextAgentBinding(raw.agent_binding),
      address_space: (raw.address_space as ExecutionAddressSpace | undefined) ?? null,
      context_snapshot: (raw.context_snapshot as SessionContextSnapshot | undefined) ?? null,
    };
  } catch {
    return null;
  }
}

export async function fetchSessionBindings(id: string): Promise<SessionBindingOwner[]> {
  const res = await authenticatedFetch(buildApiPath(`/sessions/${encodeURIComponent(id)}/bindings`));
  if (!res.ok) throw new Error(`获取会话绑定失败: HTTP ${res.status}`);
  const raw = await res.json() as Record<string, unknown>[];
  return raw.map(mapSessionBindingOwner);
}

export async function fetchSessionHookRuntime(id: string): Promise<HookSessionRuntimeInfo | null> {
  const res = await authenticatedFetch(buildApiPath(`/sessions/${encodeURIComponent(id)}/hook-runtime`));
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`获取 Hook Runtime 失败: HTTP ${res.status}`);
  return res.json();
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
      return "idle";
  }
}

function mapSessionExecutionState(raw: Record<string, unknown>): SessionExecutionState {
  return {
    session_id: String(raw.session_id ?? ""),
    status: normalizeSessionExecutionStatus(raw.status),
    turn_id: raw.turn_id != null ? String(raw.turn_id) : null,
    message: raw.message != null ? String(raw.message) : null,
  };
}

export async function fetchSessionExecutionState(id: string): Promise<SessionExecutionState> {
  const res = await authenticatedFetch(buildApiPath(`/sessions/${encodeURIComponent(id)}/state`));
  if (!res.ok) throw new Error(`获取会话运行状态失败: HTTP ${res.status}`);
  const raw = await res.json() as Record<string, unknown>;
  return mapSessionExecutionState(raw);
}

export async function cancelSession(id: string): Promise<void> {
  const res = await authenticatedFetch(buildApiPath(`/sessions/${encodeURIComponent(id)}/cancel`), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
  });
  if (!res.ok) throw new Error(`取消会话失败: HTTP ${res.status}`);
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
    session_id: String(raw.session_id ?? ""),
    session_title: raw.session_title != null ? String(raw.session_title) : null,
    last_activity: raw.last_activity != null ? Number(raw.last_activity) : null,
    execution_status: normalizeProjectSessionEntryStatus(raw.execution_status),
    owner_type: normalizeOwnerType(raw.owner_type),
    owner_id: String(raw.owner_id ?? ""),
    owner_title: raw.owner_title != null ? String(raw.owner_title) : null,
    story_id: raw.story_id != null ? String(raw.story_id) : null,
    story_title: raw.story_title != null ? String(raw.story_title) : null,
    agent_key: raw.agent_key != null ? String(raw.agent_key) : null,
    agent_display_name: raw.agent_display_name != null ? String(raw.agent_display_name) : null,
    parent_session_id: raw.parent_session_id != null ? String(raw.parent_session_id) : null,
  };
}

export async function fetchProjectSessions(projectId: string): Promise<ProjectSessionEntry[]> {
  const res = await authenticatedFetch(buildApiPath(`/projects/${encodeURIComponent(projectId)}/sessions`));
  if (!res.ok) throw new Error(`获取项目会话列表失败: HTTP ${res.status}`);
  const raw = (await res.json()) as Record<string, unknown>[];
  return raw.map(mapProjectSessionEntry);
}
