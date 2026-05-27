import { create } from "zustand";

import { fetchProjectExtensionRuntime } from "../../../services/extensionRuntime";
import type { ProjectExtensionRuntimeState } from "./types";
import { emptyExtensionRuntimeProjection, idleProjectExtensionRuntimeState } from "./types";

interface ExtensionRuntimeStoreState {
  byProjectId: Record<string, ProjectExtensionRuntimeState>;
  fetchProject: (projectId: string) => Promise<void>;
  resetProject: (projectId: string) => void;
}

const inflight = new Map<string, Promise<void>>();

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Extension runtime 加载失败";
}

function loadingState(
  projectId: string,
  current: ProjectExtensionRuntimeState | undefined,
): ProjectExtensionRuntimeState {
  return {
    project_id: projectId,
    status: current?.status === "ready" ? "refreshing" : "loading",
    projection: current?.projection ?? emptyExtensionRuntimeProjection(),
    error: null,
  };
}

export const useExtensionRuntimeStore = create<ExtensionRuntimeStoreState>()((set) => ({
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

    const request = fetchProjectExtensionRuntime(trimmed)
      .then((projection) => {
        set((state) => ({
          byProjectId: {
            ...state.byProjectId,
            [trimmed]: {
              project_id: trimmed,
              status: "ready",
              projection,
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
              projection: state.byProjectId[trimmed]?.projection ?? emptyExtensionRuntimeProjection(),
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

export function selectProjectExtensionRuntimeState(
  projectId: string | null,
): ProjectExtensionRuntimeState {
  if (!projectId) return idleProjectExtensionRuntimeState();
  return useExtensionRuntimeStore.getState().byProjectId[projectId] ?? {
    project_id: projectId,
    status: "idle",
    projection: emptyExtensionRuntimeProjection(),
    error: null,
  };
}
