import { api, type ApiHttpError } from "../api/client";
import { requireStringField, requireNumberField } from "../api/mappers";
import type {
  CreateSessionForkRequest,
  RollbackSessionProjectionRequest,
  SessionEventResponse,
  SessionEventsPageResponse,
  SessionForkResponse,
  SessionLineageViewResponse,
  SessionProjectionRollbackResponse,
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

export type PersistedSessionEvent = SessionEventResponse;
export type SessionEventsPage = SessionEventsPageResponse;

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

export async function fetchSessionEvents(
  sessionId: string,
  afterSeq = 0,
  limit = 500,
): Promise<SessionEventsPage> {
  const params = new URLSearchParams();
  params.set("after_seq", String(afterSeq));
  params.set("limit", String(limit));
  return api.get<SessionEventsPageResponse>(
    `/sessions/${encodeURIComponent(sessionId)}/events?${params.toString()}`,
  );
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

/** GET /sessions/{id}/context/projection — 返回当前模型可见上下文投影。 */
export async function fetchSessionContextProjection(
  sessionId: string,
): Promise<SessionProjectionViewResponse | null> {
  try {
    return await api.get<SessionProjectionViewResponse>(
      `/sessions/${encodeURIComponent(sessionId)}/context/projection`,
    );
  } catch (err) {
    if ((err as ApiHttpError).status === 404) return null;
    throw err;
  }
}

export async function forkSession(
  sessionId: string,
  request: CreateSessionForkRequest = {},
): Promise<SessionForkResponse> {
  return api.post<SessionForkResponse>(
    `/sessions/${encodeURIComponent(sessionId)}/fork`,
    request,
  );
}

export async function fetchSessionLineage(sessionId: string): Promise<SessionLineageViewResponse> {
  return api.get<SessionLineageViewResponse>(`/sessions/${encodeURIComponent(sessionId)}/lineage`);
}

export async function rollbackSessionProjection(
  sessionId: string,
  request: RollbackSessionProjectionRequest,
): Promise<SessionProjectionRollbackResponse> {
  return api.post<SessionProjectionRollbackResponse>(
    `/sessions/${encodeURIComponent(sessionId)}/projection/rollback`,
    request,
  );
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

function normalizeParentRelationKind(value: unknown): ProjectSessionEntry["parent_relation_kind"] {
  if (value == null) return null;
  switch (value) {
    case "fork":
    case "companion":
    case "spawned_agent":
    case "rollback_branch":
      return value;
    default:
      throw new Error(`未知的项目会话 parent_relation_kind: ${String(value ?? "")}`);
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
    parent_relation_kind: normalizeParentRelationKind(raw.parent_relation_kind),
  };
}

export async function fetchProjectSessions(projectId: string): Promise<ProjectSessionEntry[]> {
  const raw = await api.get<Record<string, unknown>[]>(`/projects/${encodeURIComponent(projectId)}/sessions`);
  return raw.map(mapProjectSessionEntry);
}
