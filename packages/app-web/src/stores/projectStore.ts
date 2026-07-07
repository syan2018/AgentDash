import { create } from 'zustand';
import type {
  ContextContainerDefinition,
  CreateProjectAgentRunRequest,
  ProjectAgent,
  ProjectAgentRunStartResult,
  ProjectRole,
  ProjectSubjectGrant,
  ProjectAgentSummary,
  Project,
  ProjectConfig,
} from '../types';
import * as projectService from '../services/project';
import {
  createEmptyUserWorkspaceState,
  loadUserWorkspaceState,
  resolveWorkspaceProjectSelection,
  saveUserWorkspaceState,
  setCurrentProjectInUserWorkspaceState,
  type UserWorkspaceState,
} from '../services/userWorkspaceState';

interface ProjectState {
  projects: Project[];
  agentsByProjectId: Record<string, ProjectAgentSummary[]>;
  grantsByProjectId: Record<string, ProjectSubjectGrant[]>;
  currentProjectId: string | null;
  userWorkspaceState: UserWorkspaceState;
  isLoading: boolean;
  error: string | null;

  // Project Agent 管理
  projectAgentConfigsByProjectId: Record<string, ProjectAgent[]>;
  fetchProjectAgentConfigs: (projectId: string) => Promise<ProjectAgent[]>;
  createProjectAgent: (projectId: string, payload: {
    name: string;
    agent_type: string;
    config?: Record<string, unknown>;
    default_lifecycle_key?: string;
  }) => Promise<ProjectAgent>;
  updateProjectAgent: (projectId: string, agentId: string, payload: {
    name?: string;
    agent_type?: string;
    config?: Record<string, unknown>;
    default_lifecycle_key?: string;
    knowledge_enabled?: boolean;
  }) => Promise<ProjectAgent>;
  deleteProjectAgent: (projectId: string, agentId: string) => Promise<boolean>;

  // Project VFS Mount Binding 失效信号：UI 操作后 bump 版本号，
  // ProjectVfsMountExposurePicker 等订阅方据此重新 fetch
  vfsMountsRevision: Record<string, number>;
  bumpVfsMountsRevision: (projectId: string) => void;

  // 既有接口
  fetchProjects: () => Promise<void>;
  createProject: (name: string, description: string, config?: Partial<ProjectConfig>) => Promise<Project>;
  updateProject: (id: string, payload: {
    name?: string;
    description?: string;
    config?: ProjectConfig;
    context_containers?: ContextContainerDefinition[];
    visibility?: Project["visibility"];
    is_template?: boolean;
  }) => Promise<Project>;
  updateProjectConfig: (id: string, config: Partial<ProjectConfig>) => Promise<Project>;
  fetchProjectGrants: (projectId: string) => Promise<ProjectSubjectGrant[]>;
  grantProjectUser: (projectId: string, userId: string, role: ProjectRole) => Promise<ProjectSubjectGrant>;
  revokeProjectUser: (projectId: string, userId: string) => Promise<boolean>;
  grantProjectGroup: (projectId: string, groupId: string, role: ProjectRole) => Promise<ProjectSubjectGrant>;
  revokeProjectGroup: (projectId: string, groupId: string) => Promise<boolean>;
  cloneProject: (projectId: string, payload?: { name?: string; description?: string }) => Promise<Project>;
  fetchProjectAgents: (projectId: string) => Promise<ProjectAgentSummary[]>;
  createProjectAgentRun: (
    projectId: string,
    agentKey: string,
    payload: CreateProjectAgentRunRequest,
  ) => Promise<ProjectAgentRunStartResult>;
  selectProject: (id: string | null) => void;
  deleteProject: (id: string) => Promise<boolean>;
}

function upsertAgentSummary(
  existing: ProjectAgentSummary[],
  agent: ProjectAgentSummary,
): ProjectAgentSummary[] {
  const hasExisting = existing.some((item) => item.key === agent.key);
  return hasExisting
    ? existing.map((item) => (item.key === agent.key ? agent : item))
    : [...existing, agent];
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function loadUserWorkspaceStateResult(): Promise<{
  state: UserWorkspaceState;
  error: string | null;
}> {
  try {
    return {
      state: await loadUserWorkspaceState(),
      error: null,
    };
  } catch (e) {
    return {
      state: createEmptyUserWorkspaceState(),
      error: errorMessage(e),
    };
  }
}

function persistUserWorkspaceState(
  state: UserWorkspaceState,
  set: (partial: Partial<ProjectState>) => void,
): void {
  void saveUserWorkspaceState(state).catch((e) => {
    set({ error: errorMessage(e) });
  });
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  projects: [],
  agentsByProjectId: {},
  grantsByProjectId: {},
  currentProjectId: null,
  userWorkspaceState: createEmptyUserWorkspaceState(),
  isLoading: false,
  error: null,

  // ─── Project Agent 管理 ───
  projectAgentConfigsByProjectId: {},

  vfsMountsRevision: {},
  bumpVfsMountsRevision: (projectId) => {
    set((s) => ({
      vfsMountsRevision: {
        ...s.vfsMountsRevision,
        [projectId]: (s.vfsMountsRevision[projectId] ?? 0) + 1,
      },
    }));
  },

  fetchProjectAgentConfigs: async (projectId) => {
    try {
      const projectAgents = await projectService.fetchProjectAgentConfigs(projectId);
      set((s) => ({
        projectAgentConfigsByProjectId: { ...s.projectAgentConfigsByProjectId, [projectId]: projectAgents },
        error: null,
      }));
      return projectAgents;
    } catch (e) {
      set({ error: (e as Error).message });
      return [];
    }
  },

  createProjectAgent: async (projectId, payload) => {
    try {
      const projectAgent = await projectService.createProjectAgent(projectId, payload);
      set((s) => {
        const existing = s.projectAgentConfigsByProjectId[projectId] ?? [];
        return {
          projectAgentConfigsByProjectId: {
            ...s.projectAgentConfigsByProjectId,
            [projectId]: [...existing, projectAgent],
          },
          error: null,
        };
      });
      return projectAgent;
    } catch (e) {
      set({ error: (e as Error).message });
      throw e;
    }
  },

  updateProjectAgent: async (projectId, agentId, payload) => {
    try {
      const projectAgent = await projectService.updateProjectAgent(projectId, agentId, payload);
      set((s) => {
        const existing = s.projectAgentConfigsByProjectId[projectId] ?? [];
        return {
          projectAgentConfigsByProjectId: {
            ...s.projectAgentConfigsByProjectId,
            [projectId]: existing.map((agent) => (agent.id === agentId ? projectAgent : agent)),
          },
          error: null,
        };
      });
      return projectAgent;
    } catch (e) {
      set({ error: (e as Error).message });
      throw e;
    }
  },

  deleteProjectAgent: async (projectId, agentId) => {
    try {
      await projectService.deleteProjectAgent(projectId, agentId);
      set((s) => {
        const existing = s.projectAgentConfigsByProjectId[projectId] ?? [];
        return {
          projectAgentConfigsByProjectId: {
            ...s.projectAgentConfigsByProjectId,
            [projectId]: existing.filter((l) => l.id !== agentId),
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

  // ─── 既有接口 ───

  fetchProjects: async () => {
    set({ isLoading: true, error: null });
    try {
      const [projects, workspaceStateResult] = await Promise.all([
        projectService.fetchProjects(),
        loadUserWorkspaceStateResult(),
      ]);
      const selection = resolveWorkspaceProjectSelection(
        projects,
        get().currentProjectId,
        workspaceStateResult.state,
      );
      set({
        projects,
        currentProjectId: selection.currentProjectId,
        userWorkspaceState: selection.workspaceState,
        error: workspaceStateResult.error,
        isLoading: false,
      });
      if (selection.shouldPersist) {
        persistUserWorkspaceState(selection.workspaceState, set);
      }
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  createProject: async (name, description, config) => {
    try {
      const project = await projectService.createProject(name, description, config);
      set((s) => {
        const userWorkspaceState = setCurrentProjectInUserWorkspaceState(s.userWorkspaceState, project.id);
        persistUserWorkspaceState(userWorkspaceState, set);
        return {
          projects: [project, ...s.projects],
          currentProjectId: project.id,
          userWorkspaceState,
        };
      });
      return project;
    } catch (e) {
      set({ error: (e as Error).message });
      throw e;
    }
  },

  updateProject: async (id, payload) => {
    try {
      const project = await projectService.updateProject(id, payload);
      set((s) => ({
        projects: s.projects.map((item) => (item.id === id ? project : item)),
      }));
      return project;
    } catch (e) {
      set({ error: (e as Error).message });
      throw e;
    }
  },

  updateProjectConfig: async (id, config) => {
    try {
      const project = await projectService.updateProjectConfig(id, config);
      set((s) => ({
        projects: s.projects.map((item) => (item.id === id ? project : item)),
      }));
      return project;
    } catch (e) {
      set({ error: (e as Error).message });
      throw e;
    }
  },

  fetchProjectGrants: async (projectId) => {
    try {
      const grants = await projectService.fetchProjectGrants(projectId);
      set((state) => ({
        grantsByProjectId: {
          ...state.grantsByProjectId,
          [projectId]: grants,
        },
        error: null,
      }));
      return grants;
    } catch (e) {
      set({ error: (e as Error).message });
      return [];
    }
  },

  grantProjectUser: async (projectId, userId, role) => {
    try {
      const grant = await projectService.grantProjectUser(projectId, userId, role);
      set((state) => {
        const current = state.grantsByProjectId[projectId] ?? [];
        const next = current.filter((item) => !(item.subject_type === "user" && item.subject_id === userId));
        next.push(grant);
        next.sort((left, right) => `${left.subject_type}:${left.subject_id}`.localeCompare(`${right.subject_type}:${right.subject_id}`));
        return {
          grantsByProjectId: {
            ...state.grantsByProjectId,
            [projectId]: next,
          },
          error: null,
        };
      });
      return grant;
    } catch (e) {
      set({ error: (e as Error).message });
      throw e;
    }
  },

  revokeProjectUser: async (projectId, userId) => {
    try {
      await projectService.revokeProjectUser(projectId, userId);
      set((state) => ({
        grantsByProjectId: {
          ...state.grantsByProjectId,
          [projectId]: (state.grantsByProjectId[projectId] ?? []).filter(
            (item) => !(item.subject_type === "user" && item.subject_id === userId),
          ),
        },
        error: null,
      }));
      return true;
    } catch (e) {
      set({ error: (e as Error).message });
      return false;
    }
  },

  grantProjectGroup: async (projectId, groupId, role) => {
    try {
      const grant = await projectService.grantProjectGroup(projectId, groupId, role);
      set((state) => {
        const current = state.grantsByProjectId[projectId] ?? [];
        const next = current.filter((item) => !(item.subject_type === "group" && item.subject_id === groupId));
        next.push(grant);
        next.sort((left, right) => `${left.subject_type}:${left.subject_id}`.localeCompare(`${right.subject_type}:${right.subject_id}`));
        return {
          grantsByProjectId: {
            ...state.grantsByProjectId,
            [projectId]: next,
          },
          error: null,
        };
      });
      return grant;
    } catch (e) {
      set({ error: (e as Error).message });
      throw e;
    }
  },

  revokeProjectGroup: async (projectId, groupId) => {
    try {
      await projectService.revokeProjectGroup(projectId, groupId);
      set((state) => ({
        grantsByProjectId: {
          ...state.grantsByProjectId,
          [projectId]: (state.grantsByProjectId[projectId] ?? []).filter(
            (item) => !(item.subject_type === "group" && item.subject_id === groupId),
          ),
        },
        error: null,
      }));
      return true;
    } catch (e) {
      set({ error: (e as Error).message });
      return false;
    }
  },

  cloneProject: async (projectId, payload) => {
    try {
      const project = await projectService.cloneProject(projectId, payload);
      set((state) => {
        const userWorkspaceState = setCurrentProjectInUserWorkspaceState(state.userWorkspaceState, project.id);
        persistUserWorkspaceState(userWorkspaceState, set);
        return {
          projects: [project, ...state.projects],
          currentProjectId: project.id,
          userWorkspaceState,
          error: null,
        };
      });
      return project;
    } catch (e) {
      set({ error: (e as Error).message });
      throw e;
    }
  },

  fetchProjectAgents: async (projectId) => {
    try {
      const agents = await projectService.fetchProjectAgents(projectId);
      set((state) => ({
        agentsByProjectId: {
          ...state.agentsByProjectId,
          [projectId]: agents,
        },
        error: null,
      }));
      return agents;
    } catch (e) {
      set({ error: (e as Error).message });
      return [];
    }
  },

  createProjectAgentRun: async (projectId, agentKey, payload) => {
    try {
      const result = await projectService.createProjectAgentRun(projectId, agentKey, payload);
      set((state) => ({
        agentsByProjectId: {
          ...state.agentsByProjectId,
          [projectId]: upsertAgentSummary(state.agentsByProjectId[projectId] ?? [], result.agent),
        },
        error: null,
      }));
      return result;
    } catch (e) {
      set({ error: (e as Error).message });
      throw e;
    }
  },

  selectProject: (id) => {
    set((state) => {
      const userWorkspaceState = setCurrentProjectInUserWorkspaceState(state.userWorkspaceState, id);
      persistUserWorkspaceState(userWorkspaceState, set);
      return {
        currentProjectId: id,
        userWorkspaceState,
      };
    });
  },

  deleteProject: async (id) => {
    try {
      await projectService.deleteProject(id);
      set((s) => {
        const remaining = s.projects.filter((p) => p.id !== id);
        const nextCurrentProjectId =
          s.currentProjectId === id ? (remaining[0]?.id ?? null) : s.currentProjectId;
        const nextWorkspaceStateSeed = setCurrentProjectInUserWorkspaceState(
          s.userWorkspaceState,
          nextCurrentProjectId,
        );
        const nextSelection = resolveWorkspaceProjectSelection(
          remaining,
          nextCurrentProjectId,
          nextWorkspaceStateSeed,
        );
        const nextAgentsByProjectId = { ...s.agentsByProjectId };
        const nextGrantsByProjectId = { ...s.grantsByProjectId };
        const nextProjectAgentConfigsByProjectId = { ...s.projectAgentConfigsByProjectId };
        delete nextAgentsByProjectId[id];
        delete nextGrantsByProjectId[id];
        delete nextProjectAgentConfigsByProjectId[id];
        if (nextSelection.shouldPersist) {
          persistUserWorkspaceState(nextSelection.workspaceState, set);
        }
        return {
          projects: remaining,
          agentsByProjectId: nextAgentsByProjectId,
          grantsByProjectId: nextGrantsByProjectId,
          projectAgentConfigsByProjectId: nextProjectAgentConfigsByProjectId,
          currentProjectId: nextSelection.currentProjectId,
          userWorkspaceState: nextSelection.workspaceState,
          error: null,
        };
      });
      return true;
    } catch (e) {
      set({ error: (e as Error).message });
      return false;
    }
  },
}));
