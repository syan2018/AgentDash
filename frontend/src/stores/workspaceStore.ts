import { create } from 'zustand';
import type {
  Workspace,
  WorkspaceBinding,
  WorkspaceBindingStatus,
  WorkspaceDetectionResult,
  WorkspaceIdentityKind,
  WorkspaceResolutionPolicy,
  WorkspaceStatus,
} from '../types';
import { api } from '../api/client';

export interface WorkspaceBindingInput {
  id?: string;
  backend_id: string;
  root_ref: string;
  status?: WorkspaceBindingStatus;
  detected_facts?: Record<string, unknown>;
  priority?: number;
}

export interface CreateWorkspaceOpts {
  identity_kind?: WorkspaceIdentityKind;
  identity_payload?: Record<string, unknown>;
  resolution_policy?: WorkspaceResolutionPolicy;
  bindings?: WorkspaceBindingInput[];
  shortcut_binding?: WorkspaceBindingInput;
}

interface WorkspaceState {
  workspacesByProjectId: Record<string, Workspace[]>;
  isLoading: boolean;
  error: string | null;

  fetchWorkspaces: (projectId: string) => Promise<void>;
  detectWorkspace: (projectId: string, backendId: string, rootRef: string) => Promise<WorkspaceDetectionResult | null>;
  createWorkspace: (projectId: string, name: string, opts?: CreateWorkspaceOpts) => Promise<Workspace | null>;
  updateWorkspace: (
    id: string,
    projectId: string,
    payload: {
      name?: string;
      identity_kind?: WorkspaceIdentityKind;
      identity_payload?: Record<string, unknown>;
      resolution_policy?: WorkspaceResolutionPolicy;
      default_binding_id?: string | null;
      bindings?: WorkspaceBindingInput[];
    },
  ) => Promise<Workspace | null>;
  updateStatus: (id: string, status: WorkspaceStatus) => Promise<void>;
  deleteWorkspace: (id: string, projectId: string) => Promise<void>;
}

function upsertWorkspace(
  existing: Workspace[],
  workspace: Workspace,
): Workspace[] {
  const hasExisting = existing.some((item) => item.id === workspace.id);
  if (!hasExisting) {
    return [workspace, ...existing];
  }
  return existing.map((item) => (item.id === workspace.id ? workspace : item));
}

export const useWorkspaceStore = create<WorkspaceState>((set) => ({
  workspacesByProjectId: {},
  isLoading: false,
  error: null,

  fetchWorkspaces: async (projectId) => {
    set({ isLoading: true, error: null });
    try {
      const workspaces = await api.get<Workspace[]>(`/projects/${projectId}/workspaces`);
      set((state) => ({
        workspacesByProjectId: { ...state.workspacesByProjectId, [projectId]: workspaces },
        isLoading: false,
      }));
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
    }
  },

  detectWorkspace: async (projectId, backendId, rootRef) => {
    try {
      set({ error: null });
      return await api.post<WorkspaceDetectionResult>(
        `/projects/${projectId}/workspaces/detect`,
        { backend_id: backendId, root_ref: rootRef },
      );
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  createWorkspace: async (projectId, name, opts) => {
    try {
      set({ error: null });
      const workspace = await api.post<Workspace>(`/projects/${projectId}/workspaces`, {
        name,
        identity_kind: opts?.identity_kind,
        identity_payload: opts?.identity_payload,
        resolution_policy: opts?.resolution_policy ?? 'prefer_online',
        bindings: opts?.bindings,
        shortcut_binding: opts?.shortcut_binding,
      });
      set((state) => {
        const existing = state.workspacesByProjectId[projectId] ?? [];
        return {
          workspacesByProjectId: {
            ...state.workspacesByProjectId,
            [projectId]: upsertWorkspace(existing, workspace),
          },
        };
      });
      return workspace;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  updateWorkspace: async (id, projectId, payload) => {
    try {
      set({ error: null });
      const workspace = await api.put<Workspace>(`/workspaces/${id}`, payload);
      set((state) => {
        const existing = state.workspacesByProjectId[projectId] ?? [];
        return {
          workspacesByProjectId: {
            ...state.workspacesByProjectId,
            [projectId]: upsertWorkspace(existing, workspace),
          },
        };
      });
      return workspace;
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  updateStatus: async (id, status) => {
    try {
      await api.patch(`/workspaces/${id}/status`, { status });
      set((state) => {
        const updated = { ...state.workspacesByProjectId };
        for (const projectId in updated) {
          updated[projectId] = updated[projectId].map((workspace) =>
            workspace.id === id ? { ...workspace, status } : workspace,
          );
        }
        return { workspacesByProjectId: updated };
      });
    } catch (error) {
      set({ error: (error as Error).message });
    }
  },

  deleteWorkspace: async (id, projectId) => {
    try {
      await api.delete(`/workspaces/${id}`);
      set((state) => ({
        workspacesByProjectId: {
          ...state.workspacesByProjectId,
          [projectId]: (state.workspacesByProjectId[projectId] ?? []).filter(
            (workspace) => workspace.id !== id,
          ),
        },
      }));
    } catch (error) {
      set({ error: (error as Error).message });
    }
  },
}));

export function findWorkspaceBinding(
  workspace: Workspace,
  bindingId?: string | null,
): WorkspaceBinding | null {
  if (bindingId) {
    const matched = workspace.bindings.find((binding) => binding.id === bindingId);
    if (matched) return matched;
  }
  if (workspace.default_binding_id) {
    const defaultBinding = workspace.bindings.find((binding) => binding.id === workspace.default_binding_id);
    if (defaultBinding) return defaultBinding;
  }
  return workspace.bindings[0] ?? null;
}
