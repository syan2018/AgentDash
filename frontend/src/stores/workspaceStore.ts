import { create } from 'zustand';
import type { Workspace, WorkspaceType, WorkspaceStatus, GitConfig } from '../types';
import { api } from '../api/client';

export interface CreateWorkspaceOpts {
  workspace_type?: WorkspaceType;
  container_ref?: string;
  git_config?: GitConfig;
}

export interface DetectedGitInfo {
  is_git_repo: boolean;
  source_repo?: string;
  branch?: string;
  commit_hash?: string;
}

interface WorkspaceState {
  workspacesByProjectId: Record<string, Workspace[]>;
  isLoading: boolean;
  error: string | null;

  fetchWorkspaces: (projectId: string) => Promise<void>;
  createWorkspace: (projectId: string, name: string, opts?: CreateWorkspaceOpts) => Promise<Workspace | null>;
  detectGitInfo: (containerRef: string) => Promise<DetectedGitInfo | null>;
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
    try {
      const workspace = await api.post<Workspace>(`/projects/${projectId}/workspaces`, {
        name,
        container_ref: opts?.container_ref ?? '',
        workspace_type: opts?.workspace_type ?? 'static',
        git_config: opts?.git_config ?? null,
      });
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

  detectGitInfo: async (containerRef) => {
    const value = containerRef.trim();
    if (!value) {
      set({ error: "目录路径不能为空" });
      return null;
    }

    try {
      const result = await api.post<DetectedGitInfo>("/workspaces/detect-git", {
        container_ref: value,
      });
      return result;
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
