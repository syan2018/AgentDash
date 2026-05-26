import { api, type ApiHttpError } from "../api/client";
import { requireStringField, requireNumberField } from "../api/mappers";
import type { BackboneEnvelope } from "../generated/backbone-protocol";
import type {
  SessionProjectionMessageRefResponse,
  SessionProjectionSegmentProvenanceResponse,
  SessionProjectionSegmentViewResponse,
  SessionProjectionSourceRangeResponse,
  SessionProjectionViewResponse,
} from "../generated/session-contracts";
import type {
  AgentBinding,
  ContextSourceRef,
  ExecutionVfs,
  HookSessionRuntimeInfo,
  ProjectSessionEntry,
  ResolvedVfsSurface,
  SessionBaselineCapabilities,
  SessionBindingOwner,
  SessionContextSnapshot,
  SessionExecutionState,
  SessionExecutionStatus,
} from "../types";
import { isThinkingLevel } from "../types";
import type { SessionTabLayout } from "../features/workspace-panel/tab-type-registry";

function asRecordOrThrow(value: unknown, label: string): Record<string, unknown> {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} 不是对象`);
  }
  return value as Record<string, unknown>;
}

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

export type TitleSource = "auto" | "source" | "user";

export interface SessionMeta {
  id: string;
  title: string;
  title_source?: TitleSource;
  createdAt: number;
  updatedAt: number;
  lastEventSeq?: number;
  tabLayout?: SessionTabLayout | null;
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
  notification: BackboneEnvelope;
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

export async function createSession(title: string | undefined, projectId: string): Promise<SessionMeta> {
  return api.post<SessionMeta>("/sessions", { title, project_id: projectId });
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
    notification: raw.notification as BackboneEnvelope,
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

export async function updateSessionTitle(id: string, title: string): Promise<SessionMeta> {
  return api.patch<SessionMeta>(`/sessions/${encodeURIComponent(id)}/meta`, { title });
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
  vfs: ExecutionVfs | null;
  runtime_surface: ResolvedVfsSurface | null;
  context_snapshot: SessionContextSnapshot | null;
  session_capabilities: SessionBaselineCapabilities | null;
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
    vfs: (raw.vfs as ExecutionVfs | undefined) ?? null,
    runtime_surface: (raw.runtime_surface as ResolvedVfsSurface | undefined) ?? null,
    context_snapshot: (raw.context_snapshot as SessionContextSnapshot | undefined) ?? null,
    session_capabilities: (raw.session_capabilities as SessionBaselineCapabilities | undefined) ?? null,
  };
}

function mapProjectionSourceRange(raw: unknown): SessionProjectionSourceRangeResponse | undefined {
  if (raw == null) return undefined;
  const value = asRecordOrThrow(raw, "projection source_range");
  return {
    start_event_seq: requireNumberField(value, "start_event_seq"),
    end_event_seq: requireNumberField(value, "end_event_seq"),
  };
}

function mapProjectionMessageRef(raw: unknown): SessionProjectionMessageRefResponse {
  const value = asRecordOrThrow(raw, "projection message_ref");
  return {
    turn_id: requireStringField(value, "turn_id"),
    entry_index: requireNumberField(value, "entry_index"),
  };
}

function mapProjectionProvenance(
  raw: unknown,
): SessionProjectionSegmentProvenanceResponse {
  const value = asRecordOrThrow(raw, "projection provenance");
  return {
    compaction_id: value.compaction_id != null ? String(value.compaction_id) : undefined,
    projection_version:
      typeof value.projection_version === "number" ? value.projection_version : undefined,
    segment_type: value.segment_type != null ? String(value.segment_type) : undefined,
    strategy: value.strategy != null ? String(value.strategy) : undefined,
    trigger: value.trigger != null ? String(value.trigger) : undefined,
    phase: value.phase != null ? String(value.phase) : undefined,
  };
}

function mapProjectionSegment(raw: unknown): SessionProjectionSegmentViewResponse {
  const value = asRecordOrThrow(raw, "projection segment");
  return {
    id: requireStringField(value, "id"),
    sort_order: requireNumberField(value, "sort_order"),
    segment_type: requireStringField(value, "segment_type"),
    role: requireStringField(value, "role"),
    origin: requireStringField(value, "origin"),
    synthetic: Boolean(value.synthetic),
    projection_kind: requireStringField(value, "projection_kind"),
    message_ref: mapProjectionMessageRef(value.message_ref),
    source_event_seq:
      typeof value.source_event_seq === "number" ? value.source_event_seq : undefined,
    source_range: mapProjectionSourceRange(value.source_range),
    projection_segment_id:
      value.projection_segment_id != null ? String(value.projection_segment_id) : undefined,
    preview: typeof value.preview === "string" ? value.preview : "",
    provenance: mapProjectionProvenance(value.provenance),
  };
}

function mapSessionProjectionView(raw: Record<string, unknown>): SessionProjectionViewResponse {
  if (!Array.isArray(raw.segments)) {
    throw new Error("会话投影视图响应缺少 segments 数组");
  }
  return {
    session_id: requireStringField(raw, "session_id"),
    branch_id: raw.branch_id != null ? String(raw.branch_id) : undefined,
    projection_kind: requireStringField(raw, "projection_kind"),
    projection_version: requireNumberField(raw, "projection_version"),
    head_event_seq: requireNumberField(raw, "head_event_seq"),
    active_compaction_id:
      raw.active_compaction_id != null ? String(raw.active_compaction_id) : undefined,
    token_estimate:
      typeof raw.token_estimate === "number" ? raw.token_estimate : undefined,
    message_count: requireNumberField(raw, "message_count"),
    segments: raw.segments.map(mapProjectionSegment),
  };
}

/** GET /sessions/{id}/context/projection — 返回当前模型可见上下文投影。 */
export async function fetchSessionContextProjection(
  sessionId: string,
): Promise<SessionProjectionViewResponse | null> {
  let raw: Record<string, unknown>;
  try {
    raw = await api.get<Record<string, unknown>>(
      `/sessions/${encodeURIComponent(sessionId)}/context/projection`,
    );
  } catch (err) {
    if ((err as ApiHttpError).status === 404) return null;
    throw err;
  }
  return mapSessionProjectionView(raw);
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

// ─── Tab 布局持久化 ──────────────────────────────────

function isSessionTabLayout(value: unknown): value is SessionTabLayout {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return Array.isArray(record.tabs)
    && (record.active_tab_uri == null || typeof record.active_tab_uri === "string");
}

/**
 * 保存 Tab 布局到 session meta。
 */
export async function saveSessionTabLayout(
  sessionId: string,
  layout: SessionTabLayout,
): Promise<void> {
  await api.patch<unknown>(
    `/sessions/${encodeURIComponent(sessionId)}/meta`,
    { tab_layout: layout },
  );
}

/**
 * 从 session meta 加载 Tab 布局。
 * 返回 null 表示无已保存布局。
 */
export async function loadSessionTabLayout(
  sessionId: string,
): Promise<SessionTabLayout | null> {
  const meta = await fetchSessionMeta(sessionId);
  return isSessionTabLayout(meta.tabLayout) ? meta.tabLayout : null;
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
