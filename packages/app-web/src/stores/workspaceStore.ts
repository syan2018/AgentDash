import { create } from 'zustand';
import type {
  Workspace,
  WorkspaceBinding,
  WorkspaceDetectionResult,
  WorkspaceIdentityKind,
  WorkspaceResolutionPolicy,
  WorkspaceStatus,
  ContextContainerCapability,
} from '../types';
import * as workspaceService from '../services/workspace';
import type { WorkspaceBindingInput, CreateWorkspaceOpts } from '../services/workspace';

export type { WorkspaceBindingInput, CreateWorkspaceOpts } from '../services/workspace';

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
      mount_capabilities?: ContextContainerCapability[];
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
      const workspaces = await workspaceService.fetchWorkspaces(projectId);
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
      return await workspaceService.detectWorkspace(projectId, backendId, rootRef);
    } catch (error) {
      set({ error: (error as Error).message });
      return null;
    }
  },

  createWorkspace: async (projectId, name, opts) => {
    try {
      set({ error: null });
      const workspace = await workspaceService.createWorkspace(projectId, name, opts);
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
      const workspace = await workspaceService.updateWorkspace(id, payload);
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
      await workspaceService.updateWorkspaceStatus(id, status);
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
      await workspaceService.deleteWorkspace(id);
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
