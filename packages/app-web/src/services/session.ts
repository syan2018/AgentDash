import { api, type ApiHttpError } from "../api/client";
import { requireStringField } from "../api/mappers";
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
import type { SessionExecutionState, SessionExecutionStatus } from "../types";
import type { SessionTabLayout } from "../features/workspace-panel/tab-type-registry";

export type TitleSource = "auto" | "source" | "user";

export type SessionExecutionStatusValue =
  | "idle"
  | "running"
  | "completed"
  | "failed"
  | "interrupted";

export interface SessionMeta {
  id: string;
  title: string;
  title_source?: TitleSource;
  createdAt: number;
  updatedAt: number;
  lastEventSeq?: number;
  lastExecutionStatus?: SessionExecutionStatusValue;
  tabLayout?: SessionTabLayout | null;
}

export type PersistedSessionEvent = SessionEventResponse;
export type SessionEventsPage = SessionEventsPageResponse;

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
