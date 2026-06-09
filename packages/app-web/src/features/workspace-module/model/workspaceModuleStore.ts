import { create } from "zustand";

import { fetchProjectWorkspaceModules } from "../../../services/workspaceModule";
import type { ProjectWorkspaceModulesState } from "./types";
import { idleProjectWorkspaceModulesState } from "./types";

interface WorkspaceModuleStoreState {
  byProjectId: Record<string, ProjectWorkspaceModulesState>;
  fetchProject: (projectId: string) => Promise<void>;
  resetProject: (projectId: string) => void;
}

const inflight = new Map<string, Promise<void>>();

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Workspace module 加载失败";
}

function loadingState(
  projectId: string,
  current: ProjectWorkspaceModulesState | undefined,
): ProjectWorkspaceModulesState {
  return {
    project_id: projectId,
    status: current?.status === "ready" ? "refreshing" : "loading",
    modules: current?.modules ?? [],
    error: null,
  };
}

export const useWorkspaceModuleStore = create<WorkspaceModuleStoreState>()((set) => ({
  byProjectId: {},

  async fetchProject(projectId) {
    const trimmed = projectId.trim();
    if (!trimmed) return;
    const existing = inflight.get(trimmed);
    if (existing) {
      await existing;
      return;
    }

    set((state) => ({
      byProjectId: {
        ...state.byProjectId,
        [trimmed]: loadingState(trimmed, state.byProjectId[trimmed]),
      },
    }));

    const request = fetchProjectWorkspaceModules(trimmed)
      .then((modules) => {
        set((state) => ({
          byProjectId: {
            ...state.byProjectId,
            [trimmed]: {
              project_id: trimmed,
              status: "ready",
              modules,
              error: null,
            },
          },
        }));
      })
      .catch((error: unknown) => {
        set((state) => ({
          byProjectId: {
            ...state.byProjectId,
            [trimmed]: {
              project_id: trimmed,
              status: "error",
              modules: state.byProjectId[trimmed]?.modules ?? [],
              error: errorMessage(error),
            },
          },
        }));
      })
      .finally(() => {
        inflight.delete(trimmed);
      });

    inflight.set(trimmed, request);
    await request;
  },

  resetProject(projectId) {
    const trimmed = projectId.trim();
    if (!trimmed) return;
    inflight.delete(trimmed);
    set((state) => {
      const next = { ...state.byProjectId };
      delete next[trimmed];
      return { byProjectId: next };
    });
  },
}));

export function selectProjectWorkspaceModulesState(
  projectId: string | null,
): ProjectWorkspaceModulesState {
  if (!projectId) return idleProjectWorkspaceModulesState();
  return (
    useWorkspaceModuleStore.getState().byProjectId[projectId] ?? {
      project_id: projectId,
      status: "idle",
      modules: [],
      error: null,
    }
  );
}
