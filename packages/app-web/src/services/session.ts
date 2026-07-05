import { api, type ApiHttpError } from "../api/client";
import { settingsApi } from "../api/settings";
import type {
  SessionEventResponse,
  SessionEventsPageResponse,
  SessionProjectionViewResponse,
} from "../generated/session-contracts";
import type { JsonValue } from "../generated/common-contracts";
import type { WorkspaceTabLayout } from "../features/workspace-runtime";

export type TitleSource = "auto" | "source" | "user";

export type SessionExecutionStatusValue =
  | "idle"
  | "running"
  | "cancelling"
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

// ─── Tab 布局持久化 ──────────────────────────────────

function isWorkspaceTabLayout(value: unknown): value is WorkspaceTabLayout {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return Array.isArray(record.tabs)
    && (record.active_tab_uri == null || typeof record.active_tab_uri === "string");
}

/**
 * 保存 AgentRun workspace Tab 布局。
 */
export async function saveWorkspaceTabLayout(
  workspaceKey: string,
  layout: WorkspaceTabLayout,
): Promise<void> {
  await settingsApi.update(
    { scope: "user" },
    [{ key: workspaceTabLayoutSettingKey(workspaceKey), value: workspaceTabLayoutToJson(layout) }],
  );
}

/**
 * 加载 AgentRun workspace Tab 布局。
 * 返回 null 表示无已保存布局。
 */
export async function loadWorkspaceTabLayout(
  workspaceKey: string,
): Promise<WorkspaceTabLayout | null> {
  const settings = await settingsApi.list({
    scope: "user",
    category: workspaceTabLayoutSettingKey(workspaceKey),
  });
  const setting = settings.find((entry) => entry.key === workspaceTabLayoutSettingKey(workspaceKey));
  return isWorkspaceTabLayout(setting?.value) ? setting.value : null;
}

function workspaceTabLayoutSettingKey(workspaceKey: string): string {
  return `ui.agentrun_workspace_tab_layout.${workspaceKey}`;
}

function workspaceTabLayoutToJson(layout: WorkspaceTabLayout): JsonValue {
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
