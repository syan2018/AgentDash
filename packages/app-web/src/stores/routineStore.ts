import { create } from "zustand";
import { api } from "../api/client";
import type {
  Routine,
  RoutineCreationResponse,
  RoutineExecution,
  RegenerateTokenResponse,
} from "../types";

interface RoutineState {
  // ── Data ──
  routinesByProjectId: Record<string, Routine[]>;
  executionsByRoutineId: Record<string, RoutineExecution[]>;
  isLoading: boolean;
  error: string | null;

  // ── Actions ──
  fetchRoutines: (projectId: string) => Promise<void>;
  createRoutine: (
    projectId: string,
    payload: {
      name: string;
      prompt_template: string;
      agent_id: string;
      trigger_config: Record<string, unknown>;
      session_strategy?: Record<string, unknown>;
    },
  ) => Promise<RoutineCreationResponse | null>;
  updateRoutine: (
    id: string,
    payload: Record<string, unknown>,
  ) => Promise<Routine | null>;
  deleteRoutine: (id: string, projectId: string) => Promise<boolean>;
  enableRoutine: (
    id: string,
    enabled: boolean,
    projectId: string,
  ) => Promise<Routine | null>;
  regenerateToken: (id: string) => Promise<RegenerateTokenResponse | null>;
  fetchExecutions: (
    routineId: string,
    limit?: number,
    offset?: number,
  ) => Promise<void>;
}

export const useRoutineStore = create<RoutineState>((set) => ({
  routinesByProjectId: {},
  executionsByRoutineId: {},
  isLoading: false,
  error: null,

  fetchRoutines: async (projectId) => {
    set({ isLoading: true, error: null });
    try {
      const routines = await api.get<Routine[]>(
        `/projects/${projectId}/routines`,
      );
      set((s) => ({
        routinesByProjectId: {
          ...s.routinesByProjectId,
          [projectId]: routines,
        },
        isLoading: false,
      }));
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  createRoutine: async (projectId, payload) => {
    try {
      const result = await api.post<RoutineCreationResponse>(
        `/projects/${projectId}/routines`,
        payload,
      );
      // 从 flattened response 中提取 Routine 字段
      const routine: Routine = {
        id: result.id,
        project_id: result.project_id,
        name: result.name,
        prompt_template: result.prompt_template,
        agent_id: result.agent_id,
        trigger_config: result.trigger_config,
        session_strategy: result.session_strategy,
        enabled: result.enabled,
        created_at: result.created_at,
        updated_at: result.updated_at,
        last_fired_at: result.last_fired_at,
      };
      set((s) => {
        const existing = s.routinesByProjectId[projectId] ?? [];
        return {
          routinesByProjectId: {
            ...s.routinesByProjectId,
            [projectId]: [...existing, routine],
          },
          error: null,
        };
      });
      return result;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  updateRoutine: async (id, payload) => {
    try {
      const routine = await api.put<Routine>(`/routines/${id}`, payload);
      set((s) => {
        const pid = routine.project_id;
        const existing = s.routinesByProjectId[pid] ?? [];
        return {
          routinesByProjectId: {
            ...s.routinesByProjectId,
            [pid]: existing.map((r) => (r.id === id ? routine : r)),
          },
          error: null,
        };
      });
      return routine;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  deleteRoutine: async (id, projectId) => {
    try {
      await api.delete(`/routines/${id}`);
      set((s) => {
        const existing = s.routinesByProjectId[projectId] ?? [];
        return {
          routinesByProjectId: {
            ...s.routinesByProjectId,
            [projectId]: existing.filter((r) => r.id !== id),
          },
          error: null,
        };
      });
      return true;
    } catch (e) {
      set({ error: (e as Error).message });
      return false;
    }
  },

  enableRoutine: async (id, enabled, projectId) => {
    try {
      const routine = await api.patch<Routine>(`/routines/${id}/enable`, {
        enabled,
      });
      set((s) => {
        const existing = s.routinesByProjectId[projectId] ?? [];
        return {
          routinesByProjectId: {
            ...s.routinesByProjectId,
            [projectId]: existing.map((r) => (r.id === id ? routine : r)),
          },
          error: null,
        };
      });
      return routine;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  regenerateToken: async (id) => {
    try {
      const result = await api.post<RegenerateTokenResponse>(
        `/routines/${id}/regenerate-token`,
        {},
      );
      return result;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  fetchExecutions: async (routineId, limit = 20, offset = 0) => {
    try {
      const executions = await api.get<RoutineExecution[]>(
        `/routines/${routineId}/executions?limit=${limit}&offset=${offset}`,
      );
      set((s) => ({
        executionsByRoutineId: {
          ...s.executionsByRoutineId,
          [routineId]:
            offset === 0
              ? executions
              : [...(s.executionsByRoutineId[routineId] ?? []), ...executions],
        },
        error: null,
      }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },
}));
