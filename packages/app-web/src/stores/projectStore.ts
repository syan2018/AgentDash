import { create } from 'zustand';
import type {
  ContextContainerDefinition,
  OpenProjectAgentSessionResult,
  ProjectAgent,
  ProjectAgentSession,
  ProjectSessionInfo,
  ProjectRole,
  ProjectSubjectGrant,
  ProjectAgentSummary,
  Project,
  ProjectConfig,
} from '../types';
import * as projectService from '../services/project';

interface ProjectState {
  projects: Project[];
  agentsByProjectId: Record<string, ProjectAgentSummary[]>;
  grantsByProjectId: Record<string, ProjectSubjectGrant[]>;
  currentProjectId: string | null;
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
    default_procedure_key?: string;
    is_default_for_story?: boolean;
    is_default_for_task?: boolean;
  }) => Promise<ProjectAgent | null>;
  updateProjectAgent: (projectId: string, agentId: string, payload: {
    name?: string;
    agent_type?: string;
    config?: Record<string, unknown>;
    default_lifecycle_key?: string;
    default_procedure_key?: string;
    is_default_for_story?: boolean;
    is_default_for_task?: boolean;
    knowledge_enabled?: boolean;
  }) => Promise<ProjectAgent | null>;
  deleteProjectAgent: (projectId: string, agentId: string) => Promise<boolean>;

  // Project VFS Mount Binding 失效信号：UI 操作后 bump 版本号，
  // VfsAccessPicker 等订阅方据此重新 fetch
  vfsMountsRevision: Record<string, number>;
  bumpVfsMountsRevision: (projectId: string) => void;

  // 既有接口
  fetchProjects: () => Promise<void>;
  createProject: (name: string, description: string, config?: Partial<ProjectConfig>) => Promise<Project | null>;
  updateProject: (id: string, payload: {
    name?: string;
    description?: string;
    config?: ProjectConfig;
    context_containers?: ContextContainerDefinition[];
    visibility?: Project["visibility"];
    is_template?: boolean;
  }) => Promise<Project | null>;
  updateProjectConfig: (id: string, config: Partial<ProjectConfig>) => Promise<Project | null>;
  fetchProjectGrants: (projectId: string) => Promise<ProjectSubjectGrant[]>;
  grantProjectUser: (projectId: string, userId: string, role: ProjectRole) => Promise<ProjectSubjectGrant | null>;
  revokeProjectUser: (projectId: string, userId: string) => Promise<boolean>;
  grantProjectGroup: (projectId: string, groupId: string, role: ProjectRole) => Promise<ProjectSubjectGrant | null>;
  revokeProjectGroup: (projectId: string, groupId: string) => Promise<boolean>;
  cloneProject: (projectId: string, payload?: { name?: string; description?: string }) => Promise<Project | null>;
  fetchProjectAgents: (projectId: string) => Promise<ProjectAgentSummary[]>;
  openProjectAgentSession: (projectId: string, agentKey: string) => Promise<OpenProjectAgentSessionResult | null>;
  forceNewProjectAgentSession: (projectId: string, agentKey: string) => Promise<OpenProjectAgentSessionResult | null>;
  fetchProjectAgentSessions: (projectId: string, agentKey: string) => Promise<ProjectAgentSession[]>;
  fetchProjectSessionInfo: (projectId: string, sessionId: string) => Promise<ProjectSessionInfo | null>;
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

export const useProjectStore = create<ProjectState>((set, get) => ({
  projects: [],
  agentsByProjectId: {},
  grantsByProjectId: {},
  currentProjectId: null,
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
      return null;
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
      return null;
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
      const projects = await projectService.fetchProjects();
      set({ projects, isLoading: false });
      if (!get().currentProjectId && projects.length > 0) {
        set({ currentProjectId: projects[0].id });
      }
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  createProject: async (name, description, config) => {
    try {
      const project = await projectService.createProject(name, description, config);
      set((s) => ({
        projects: [project, ...s.projects],
        currentProjectId: project.id,
      }));
      return project;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
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
      return null;
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
      return null;
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
      return null;
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
      return null;
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
      set((state) => ({
        projects: [project, ...state.projects],
        currentProjectId: project.id,
        error: null,
      }));
      return project;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
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

  openProjectAgentSession: async (projectId, agentKey) => {
    try {
      const result = await projectService.openProjectAgentSession(projectId, agentKey);
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
      return null;
    }
  },

  forceNewProjectAgentSession: async (projectId, agentKey) => {
    try {
      const result = await projectService.forceNewProjectAgentSession(projectId, agentKey);
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
      return null;
    }
  },

  fetchProjectAgentSessions: async (projectId, agentKey) => {
    try {
      return await projectService.fetchProjectAgentSessions(projectId, agentKey);
    } catch (e) {
      set({ error: (e as Error).message });
      return [];
    }
  },

  fetchProjectSessionInfo: async (projectId, sessionId) => {
    try {
      return await projectService.fetchProjectSessionInfo(projectId, sessionId);
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  selectProject: (id) => set({ currentProjectId: id }),

  deleteProject: async (id) => {
    try {
      await projectService.deleteProject(id);
      set((s) => {
        const remaining = s.projects.filter((p) => p.id !== id);
        const nextCurrentProjectId =
          s.currentProjectId === id ? (remaining[0]?.id ?? null) : s.currentProjectId;
        const nextAgentsByProjectId = { ...s.agentsByProjectId };
        const nextGrantsByProjectId = { ...s.grantsByProjectId };
        const nextProjectAgentConfigsByProjectId = { ...s.projectAgentConfigsByProjectId };
        delete nextAgentsByProjectId[id];
        delete nextGrantsByProjectId[id];
        delete nextProjectAgentConfigsByProjectId[id];
        return {
          projects: remaining,
          agentsByProjectId: nextAgentsByProjectId,
          grantsByProjectId: nextGrantsByProjectId,
          projectAgentConfigsByProjectId: nextProjectAgentConfigsByProjectId,
          currentProjectId: nextCurrentProjectId,
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
