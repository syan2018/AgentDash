import { create } from 'zustand';
import type { Workspace, WorkspaceType, WorkspaceStatus } from '../types';
import { api } from '../api/client';

export interface CreateWorkspaceOpts {
  workspace_type?: WorkspaceType;
  container_ref?: string;
}

interface WorkspaceState {
  workspacesByProjectId: Record<string, Workspace[]>;
  isLoading: boolean;
  error: string | null;

  fetchWorkspaces: (projectId: string) => Promise<void>;
  createWorkspace: (projectId: string, name: string, opts?: CreateWorkspaceOpts) => Promise<Workspace | null>;
  updateWorkspace: (
    id: string,
    projectId: string,
    payload: { name?: string; container_ref?: string; workspace_type?: WorkspaceType },
  ) => Promise<Workspace | null>;
  updateStatus: (id: string, status: WorkspaceStatus) => Promise<void>;
  deleteWorkspace: (id: string, projectId: string) => Promise<void>;
}

export const useWorkspaceStore = create<WorkspaceState>((set) => ({
  workspacesByProjectId: {},
  isLoading: false,
  error: null,

  fetchWorkspaces: async (projectId) => {
    set({ isLoading: true, error: null });
    try {
      const workspaces = await api.get<Workspace[]>(`/projects/${projectId}/workspaces`);
      set((s) => ({
        workspacesByProjectId: { ...s.workspacesByProjectId, [projectId]: workspaces },
        isLoading: false,
      }));
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  createWorkspace: async (projectId, name, opts) => {
    const containerRef = opts?.container_ref?.trim();
    try {
      set({ error: null });
      const payload: Record<string, unknown> = {
        name,
        workspace_type: opts?.workspace_type ?? 'static',
      };
      if (containerRef) {
        payload.container_ref = containerRef;
      }

      const workspace = await api.post<Workspace>(`/projects/${projectId}/workspaces`, payload);
      set((s) => {
        const existing = s.workspacesByProjectId[projectId] ?? [];
        return {
          workspacesByProjectId: {
            ...s.workspacesByProjectId,
            [projectId]: [workspace, ...existing],
          },
        };
      });
      return workspace;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  updateWorkspace: async (id, projectId, payload) => {
    try {
      set({ error: null });
      const workspace = await api.put<Workspace>(`/workspaces/${id}`, payload);
      set((s) => {
        const existing = s.workspacesByProjectId[projectId] ?? [];
        return {
          workspacesByProjectId: {
            ...s.workspacesByProjectId,
            [projectId]: existing.map((item) => (item.id === id ? workspace : item)),
          },
        };
      });
      return workspace;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  updateStatus: async (id, status) => {
    try {
      await api.patch(`/workspaces/${id}/status`, { status });
      set((s) => {
        const updated = { ...s.workspacesByProjectId };
        for (const pid in updated) {
          updated[pid] = updated[pid].map((ws) =>
            ws.id === id ? { ...ws, status } : ws,
          );
        }
        return { workspacesByProjectId: updated };
      });
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  deleteWorkspace: async (id, projectId) => {
    try {
      await api.delete(`/workspaces/${id}`);
      set((s) => {
        const existing = s.workspacesByProjectId[projectId] ?? [];
        return {
          workspacesByProjectId: {
            ...s.workspacesByProjectId,
            [projectId]: existing.filter((ws) => ws.id !== id),
          },
        };
      });
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },
}));
