import { api, type ApiHttpError } from "../api/client";
import { requireStringField } from "../api/mappers";
import { settingsApi } from "../api/settings";
import type {
  SessionEventResponse,
  SessionEventsPageResponse,
  SessionProjectionViewResponse,
} from "../generated/session-contracts";
import type { JsonValue } from "../generated/common-contracts";
import type { SessionTabLayout } from "../features/workspace-runtime";

export type TitleSource = "auto" | "source" | "user";

export type SessionExecutionStatusValue =
  | "idle"
  | "running"
  | "cancelling"
  | "completed"
  | "failed"
  | "interrupted";

// `/sessions/{id}/state` 是 RuntimeSession trace 的诊断/legacy 查询入口。
// AgentRun / workspace 控制 UI 使用 generated workspace/conversation DTO，不从这里派生命令事实。
export interface RouteLocalSessionExecutionState {
  session_id: string;
  status: SessionExecutionStatusValue;
  turn_id: string | null;
  message: string | null;
}

export interface SessionMeta {
  id: string;
  title: string;
  title_source?: TitleSource;
  createdAt: number;
  updatedAt: number;
  lastEventSeq?: number;
  lastExecutionStatus?: SessionExecutionStatusValue;
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

/** Runtime trace diagnostic fallback for current model-context projection. */
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

function normalizeSessionExecutionStatus(value: unknown): SessionExecutionStatusValue {
  switch (value) {
    case "idle":
    case "running":
    case "cancelling":
    case "completed":
    case "failed":
    case "interrupted":
      return value;
    default:
      throw new Error(`未知的会话执行状态: ${String(value ?? "")}`);
  }
}

function mapSessionExecutionState(raw: Record<string, unknown>): RouteLocalSessionExecutionState {
  return {
    session_id: requireStringField(raw, "session_id"),
    status: normalizeSessionExecutionStatus(raw.status),
    turn_id: raw.turn_id != null ? String(raw.turn_id) : null,
    message: raw.message != null ? String(raw.message) : null,
  };
}

export async function fetchSessionExecutionState(
  id: string,
): Promise<RouteLocalSessionExecutionState> {
  const raw = await api.get<Record<string, unknown>>(`/sessions/${encodeURIComponent(id)}/state`);
  return mapSessionExecutionState(raw);
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
  await settingsApi.update(
    { scope: "user" },
    [{ key: sessionTabLayoutSettingKey(sessionId), value: sessionTabLayoutToJson(layout) }],
  );
}

/**
 * 从 session meta 加载 Tab 布局。
 * 返回 null 表示无已保存布局。
 */
export async function loadSessionTabLayout(
  sessionId: string,
): Promise<SessionTabLayout | null> {
  const settings = await settingsApi.list({
    scope: "user",
    category: sessionTabLayoutSettingKey(sessionId),
  });
  const setting = settings.find((entry) => entry.key === sessionTabLayoutSettingKey(sessionId));
  return isSessionTabLayout(setting?.value) ? setting.value : null;
}

function sessionTabLayoutSettingKey(sessionId: string): string {
  return `ui.session_tab_layout.${sessionId}`;
}

function sessionTabLayoutToJson(layout: SessionTabLayout): JsonValue {
  return {
    tabs: layout.tabs.map((tab) => ({
      type_id: tab.type_id,
      uri: tab.uri,
      title: tab.title,
      pinned: tab.pinned,
    })),
    active_tab_uri: layout.active_tab_uri,
  };
}
