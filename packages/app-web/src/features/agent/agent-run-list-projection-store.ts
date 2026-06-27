import { useEffect } from "react";
import { create, type StoreApi } from "zustand";

import { fetchProjectAgentRuns } from "../../services/lifecycle";
import { subscribeProjectEvents } from "../../stores/eventStore";
import type { ProjectEventStreamEnvelope } from "../../generated/project-contracts";
import type { AgentRunWorkspaceListEntry } from "../../types";

export const AGENT_RUN_LIST_FIRST_PAGE_LIMIT = 30;

export type AgentRunListProjectionStatus =
  | "idle"
  | "loading"
  | "refreshing"
  | "ready"
  | "error";

export interface AgentRunListProjectionState {
  project_id: string | null;
  status: AgentRunListProjectionStatus;
  entries: AgentRunWorkspaceListEntry[];
  next_cursor: string | null;
  first_page_limit: number;
  is_loading_more: boolean;
  error: string | null;
}

interface AgentRunListProjectionStoreState {
  byProjectId: Record<string, AgentRunListProjectionState>;
  ensureFirstPage: (projectId: string, limit?: number) => Promise<void>;
  refreshProject: (projectId: string, reason: string, limit?: number) => Promise<void>;
  invalidateProject: (projectId: string, reason: string, limit?: number) => Promise<void>;
  loadMore: (projectId: string, limit?: number) => Promise<void>;
  resetProject: (projectId: string) => void;
}

type AgentRunListProjectionSet = StoreApi<AgentRunListProjectionStoreState>["setState"];
type AgentRunListProjectionGet = StoreApi<AgentRunListProjectionStoreState>["getState"];

const firstPageInflight = new Map<string, Promise<void>>();
const loadMoreInflight = new Map<string, Promise<void>>();

export function idleAgentRunListProjectionState(
  projectId: string | null = null,
): AgentRunListProjectionState {
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
  current: AgentRunListProjectionState | undefined,
  limit: number,
  force: boolean,
): AgentRunListProjectionState {
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
  set: AgentRunListProjectionSet,
  get: AgentRunListProjectionGet,
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

export const useAgentRunListProjectionStore = create<AgentRunListProjectionStoreState>()((set, get) => ({
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
      const previous = state.byProjectId[trimmed] ?? idleAgentRunListProjectionState(trimmed);
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
          const previous = state.byProjectId[trimmed] ?? idleAgentRunListProjectionState(trimmed);
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
          const previous = state.byProjectId[trimmed] ?? idleAgentRunListProjectionState(trimmed);
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

export function selectAgentRunListProjectionState(
  projectId: string | null,
): AgentRunListProjectionState {
  if (!projectId) return idleAgentRunListProjectionState(null);
  return useAgentRunListProjectionStore.getState().byProjectId[projectId]
    ?? idleAgentRunListProjectionState(projectId);
}

export function shouldRefreshAgentRunListProjectionForProjectEvent(
  event: ProjectEventStreamEnvelope,
  projectId: string,
): boolean {
  return event.type === "StateChanged" && event.data.project_id === projectId;
}

export async function invalidateAgentRunListProjectionForProjectEvent(
  event: ProjectEventStreamEnvelope,
  projectId: string,
): Promise<void> {
  if (!shouldRefreshAgentRunListProjectionForProjectEvent(event, projectId)) return;
  await useAgentRunListProjectionStore
    .getState()
    .invalidateProject(projectId, `project_event:${event.type}`);
}

export function refreshAgentRunListProjection(
  projectId: string | null,
  reason: string,
): void {
  if (!projectId) return;
  void useAgentRunListProjectionStore.getState().refreshProject(projectId, reason);
}

export function useAgentRunListProjection(
  projectId: string | null,
  firstPageLimit = AGENT_RUN_LIST_FIRST_PAGE_LIMIT,
): AgentRunListProjectionState {
  const projection = useAgentRunListProjectionStore((state) => (
    projectId ? state.byProjectId[projectId] : undefined
  ));
  const ensureFirstPage = useAgentRunListProjectionStore((state) => state.ensureFirstPage);

  useEffect(() => {
    if (!projectId) return;
    void ensureFirstPage(projectId, firstPageLimit);
  }, [ensureFirstPage, firstPageLimit, projectId]);

  useEffect(() => {
    if (!projectId) return;
    return subscribeProjectEvents((event) => {
      void invalidateAgentRunListProjectionForProjectEvent(event, projectId);
    });
  }, [projectId]);

  return projection ?? idleAgentRunListProjectionState(projectId);
}
