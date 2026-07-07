import { useEffect } from "react";
import { create, type StoreApi } from "zustand";

import { fetchProjectAgentRuns } from "../../services/lifecycle";
import { subscribeProjectEvents } from "../../stores/eventStore";
import type { ProjectEventStreamEnvelope } from "../../generated/project-contracts";
import type { AgentRunWorkspaceListEntry } from "../../types";

export const AGENT_RUN_LIST_FIRST_PAGE_LIMIT = 30;

export type AgentRunListStateStatus =
  | "idle"
  | "loading"
  | "refreshing"
  | "ready"
  | "error";

export interface AgentRunListState {
  project_id: string | null;
  status: AgentRunListStateStatus;
  entries: AgentRunWorkspaceListEntry[];
  next_cursor: string | null;
  first_page_limit: number;
  is_loading_more: boolean;
  error: string | null;
}

interface AgentRunListStoreState {
  byProjectId: Record<string, AgentRunListState>;
  ensureFirstPage: (projectId: string, limit?: number) => Promise<void>;
  refreshProject: (projectId: string, reason: string, limit?: number) => Promise<void>;
  invalidateProject: (projectId: string, reason: string, limit?: number) => Promise<void>;
  loadMore: (projectId: string, limit?: number) => Promise<void>;
  resetProject: (projectId: string) => void;
}

type AgentRunListStateSet = StoreApi<AgentRunListStoreState>["setState"];
type AgentRunListStateGet = StoreApi<AgentRunListStoreState>["getState"];

const firstPageInflight = new Map<string, Promise<void>>();
const loadMoreInflight = new Map<string, Promise<void>>();

export function idleAgentRunListState(
  projectId: string | null = null,
): AgentRunListState {
  return {
    project_id: projectId,
    status: "idle",
    entries: [],
    next_cursor: null,
    first_page_limit: AGENT_RUN_LIST_FIRST_PAGE_LIMIT,
    is_loading_more: false,
    error: null,
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "AgentRun 列表加载失败";
}

function normalizeLimit(limit: number | undefined): number {
  if (!Number.isFinite(limit) || limit == null) return AGENT_RUN_LIST_FIRST_PAGE_LIMIT;
  return Math.max(1, Math.floor(limit));
}

function entryKey(entry: AgentRunWorkspaceListEntry): string {
  return `${entry.run_ref.run_id}:${entry.agent_ref.agent_id}`;
}

function appendPageEntries(
  current: AgentRunWorkspaceListEntry[],
  pageEntries: AgentRunWorkspaceListEntry[],
): AgentRunWorkspaceListEntry[] {
  const seen = new Set(current.map(entryKey));
  const next = current.slice();
  for (const entry of pageEntries) {
    const key = entryKey(entry);
    if (seen.has(key)) continue;
    seen.add(key);
    next.push(entry);
  }
  return next;
}

function loadingFirstPageState(
  projectId: string,
  current: AgentRunListState | undefined,
  limit: number,
  force: boolean,
): AgentRunListState {
  const hasEntries = (current?.entries.length ?? 0) > 0;
  return {
    project_id: projectId,
    status: force || hasEntries ? "refreshing" : "loading",
    entries: current?.entries ?? [],
    next_cursor: current?.next_cursor ?? null,
    first_page_limit: Math.max(limit, current?.first_page_limit ?? 0),
    is_loading_more: current?.is_loading_more ?? false,
    error: null,
  };
}

async function fetchFirstPage(
  set: AgentRunListStateSet,
  get: AgentRunListStateGet,
  projectId: string,
  limit: number,
  force: boolean,
): Promise<void> {
  const trimmed = projectId.trim();
  if (!trimmed) return;

  const current = get().byProjectId[trimmed];
  if (!force && current?.status === "ready" && current.first_page_limit >= limit) {
    return;
  }

  const existing = firstPageInflight.get(trimmed);
  if (existing) {
    await existing;
    return;
  }

  const requestLimit = Math.max(limit, current?.first_page_limit ?? 0, AGENT_RUN_LIST_FIRST_PAGE_LIMIT);
  set((state) => ({
    byProjectId: {
      ...state.byProjectId,
      [trimmed]: loadingFirstPageState(trimmed, state.byProjectId[trimmed], requestLimit, force),
    },
  }));

  const request = fetchProjectAgentRuns(trimmed, { limit: requestLimit })
    .then((view) => {
      set((state) => ({
        byProjectId: {
          ...state.byProjectId,
          [trimmed]: {
            project_id: trimmed,
            status: "ready",
            entries: view.agent_runs,
            next_cursor: view.next_cursor ?? null,
            first_page_limit: requestLimit,
            is_loading_more: false,
            error: null,
          },
        },
      }));
    })
    .catch((error: unknown) => {
      set((state) => {
        const previous = state.byProjectId[trimmed];
        return {
          byProjectId: {
            ...state.byProjectId,
            [trimmed]: {
              project_id: trimmed,
              status: "error",
              entries: previous?.entries ?? [],
              next_cursor: previous?.next_cursor ?? null,
              first_page_limit: previous?.first_page_limit ?? requestLimit,
              is_loading_more: false,
              error: errorMessage(error),
            },
          },
        };
      });
    })
    .finally(() => {
      firstPageInflight.delete(trimmed);
    });

  firstPageInflight.set(trimmed, request);
  await request;
}

export const useAgentRunListStateStore = create<AgentRunListStoreState>()((set, get) => ({
  byProjectId: {},

  async ensureFirstPage(projectId, limit) {
    await fetchFirstPage(set, get, projectId, normalizeLimit(limit), false);
  },

  async refreshProject(projectId, _reason, limit) {
    await fetchFirstPage(set, get, projectId, normalizeLimit(limit), true);
  },

  async invalidateProject(projectId, reason, limit) {
    await get().refreshProject(projectId, reason, limit);
  },

  async loadMore(projectId, limit) {
    const trimmed = projectId.trim();
    if (!trimmed) return;
    const current = get().byProjectId[trimmed];
    const cursor = current?.next_cursor ?? null;
    if (!cursor) return;

    const key = `${trimmed}:${cursor}`;
    const existing = loadMoreInflight.get(key);
    if (existing) {
      await existing;
      return;
    }

    const requestLimit = normalizeLimit(limit ?? current?.first_page_limit);
    set((state) => {
      const previous = state.byProjectId[trimmed] ?? idleAgentRunListState(trimmed);
      return {
        byProjectId: {
          ...state.byProjectId,
          [trimmed]: {
            ...previous,
            is_loading_more: true,
            error: null,
          },
        },
      };
    });

    const request = fetchProjectAgentRuns(trimmed, { limit: requestLimit, cursor })
      .then((view) => {
        set((state) => {
          const previous = state.byProjectId[trimmed] ?? idleAgentRunListState(trimmed);
          return {
            byProjectId: {
              ...state.byProjectId,
              [trimmed]: {
                ...previous,
                status: "ready",
                entries: appendPageEntries(previous.entries, view.agent_runs),
                next_cursor: view.next_cursor ?? null,
                first_page_limit: Math.max(previous.first_page_limit, requestLimit),
                is_loading_more: false,
                error: null,
              },
            },
          };
        });
      })
      .catch((error: unknown) => {
        set((state) => {
          const previous = state.byProjectId[trimmed] ?? idleAgentRunListState(trimmed);
          return {
            byProjectId: {
              ...state.byProjectId,
              [trimmed]: {
                ...previous,
                is_loading_more: false,
                error: errorMessage(error),
              },
            },
          };
        });
      })
      .finally(() => {
        loadMoreInflight.delete(key);
      });

    loadMoreInflight.set(key, request);
    await request;
  },

  resetProject(projectId) {
    const trimmed = projectId.trim();
    if (!trimmed) return;
    firstPageInflight.delete(trimmed);
    set((state) => {
      const next = { ...state.byProjectId };
      delete next[trimmed];
      return { byProjectId: next };
    });
  },
}));

export function selectAgentRunListState(
  projectId: string | null,
): AgentRunListState {
  if (!projectId) return idleAgentRunListState(null);
  return useAgentRunListStateStore.getState().byProjectId[projectId]
    ?? idleAgentRunListState(projectId);
}

export function shouldRefreshAgentRunListStateForProjectEvent(
  event: ProjectEventStreamEnvelope,
  projectId: string,
): boolean {
  if (event.type === "StateChanged") {
    return event.data.project_id === projectId;
  }
  if (event.type === "ControlPlaneProjectionChanged") {
    return (
      event.data.project_id === projectId
      && event.data.change.projection === "agent_run_list"
    );
  }
  return false;
}

export async function invalidateAgentRunListStateForProjectEvent(
  event: ProjectEventStreamEnvelope,
  projectId: string,
): Promise<void> {
  if (!shouldRefreshAgentRunListStateForProjectEvent(event, projectId)) return;
  const reason = event.type === "ControlPlaneProjectionChanged"
    ? `project_event:${event.type}:${event.data.change.projection}:${event.data.change.reason}`
    : `project_event:${event.type}`;
  await useAgentRunListStateStore
    .getState()
    .invalidateProject(projectId, reason);
}

export function refreshAgentRunListState(
  projectId: string | null,
  reason: string,
): void {
  if (!projectId) return;
  void useAgentRunListStateStore.getState().refreshProject(projectId, reason);
}

export function useAgentRunListState(
  projectId: string | null,
  firstPageLimit = AGENT_RUN_LIST_FIRST_PAGE_LIMIT,
): AgentRunListState {
  const listState = useAgentRunListStateStore((state) => (
    projectId ? state.byProjectId[projectId] : undefined
  ));
  const ensureFirstPage = useAgentRunListStateStore((state) => state.ensureFirstPage);

  useEffect(() => {
    if (!projectId) return;
    void ensureFirstPage(projectId, firstPageLimit);
  }, [ensureFirstPage, firstPageLimit, projectId]);

  useEffect(() => {
    if (!projectId) return;
    return subscribeProjectEvents((event) => {
      void invalidateAgentRunListStateForProjectEvent(event, projectId);
    });
  }, [projectId]);

  return listState ?? idleAgentRunListState(projectId);
}
