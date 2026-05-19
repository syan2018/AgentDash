import { create } from 'zustand';
import type {
  ContextContainerDefinition,
  OpenProjectAgentSessionResult,
  ProjectAgent,
  ProjectAgentSession,
  ProjectSessionInfo,
  ProjectRole,
  ProjectSubjectGrant,
  SessionContextSnapshot,
  ProjectAgentSummary,
  Project,
  ProjectConfig,
} from '../types';
import { isThinkingLevel } from '../types';
import { api } from '../api/client';

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
    default_workflow_key?: string;
    is_default_for_story?: boolean;
    is_default_for_task?: boolean;
  }) => Promise<ProjectAgent | null>;
  updateProjectAgent: (projectId: string, agentId: string, payload: {
    name?: string;
    agent_type?: string;
    config?: Record<string, unknown>;
    default_lifecycle_key?: string;
    default_workflow_key?: string;
    is_default_for_story?: boolean;
    is_default_for_task?: boolean;
    knowledge_enabled?: boolean;
    project_container_ids?: string[];
  }) => Promise<ProjectAgent | null>;
  deleteProjectAgent: (projectId: string, agentId: string) => Promise<boolean>;

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
  fetchProjectSessionInfo: (projectId: string, bindingId: string) => Promise<ProjectSessionInfo | null>;
  selectProject: (id: string | null) => void;
  deleteProject: (id: string) => Promise<boolean>;
}

function mapProjectAgentSummary(raw: Record<string, unknown>): ProjectAgentSummary {
  const rawExecutor = raw.executor && typeof raw.executor === 'object'
    ? raw.executor as Record<string, unknown>
    : {};
  const rawSession = raw.session && typeof raw.session === 'object'
    ? raw.session as Record<string, unknown>
    : null;
  const thinkingLevel = isThinkingLevel(rawExecutor.thinking_level)
    ? rawExecutor.thinking_level
    : null;

  return {
    key: String(raw.key ?? ''),
    display_name: String(raw.display_name ?? '未命名 Agent'),
    description: String(raw.description ?? ''),
    executor: {
      executor: String(rawExecutor.executor ?? ''),
      provider_id: rawExecutor.provider_id != null ? String(rawExecutor.provider_id) : null,
      model_id: rawExecutor.model_id != null ? String(rawExecutor.model_id) : null,
      agent_id: rawExecutor.agent_id != null ? String(rawExecutor.agent_id) : null,
      thinking_level: thinkingLevel,
      permission_policy: rawExecutor.permission_policy != null ? String(rawExecutor.permission_policy) : null,
    },
    preset_name: raw.preset_name != null
      ? String(raw.preset_name)
      : null,
    source: String(raw.source ?? ''),
    session: rawSession
      ? {
          binding_id: requireStringField(rawSession, 'binding_id'),
          session_id: String(rawSession.session_id ?? ''),
          session_title: rawSession.session_title != null
            ? String(rawSession.session_title)
            : null,
          last_activity: rawSession.last_activity != null
            ? Number(rawSession.last_activity)
            : null,
        }
      : null,
  };
}

function mapOpenProjectAgentSessionResult(raw: Record<string, unknown>): OpenProjectAgentSessionResult {
  const rawAgent = raw.agent && typeof raw.agent === 'object'
    ? raw.agent as Record<string, unknown>
    : {};

  return {
    created: Boolean(raw.created),
    session_id: String(raw.session_id ?? ''),
    binding_id: requireStringField(raw, 'binding_id'),
    agent: mapProjectAgentSummary(rawAgent),
  };
}

function requireStringField(raw: Record<string, unknown>, field: string): string {
  const value = raw[field];
  if (typeof value === 'string' && value.length > 0) {
    return value;
  }
  throw new Error(`ProjectSessionInfo 缺少必填字段: ${field}`);
}

function mapProjectSessionInfo(raw: Record<string, unknown>): ProjectSessionInfo {
  const contextSnapshot = raw.context_snapshot && typeof raw.context_snapshot === 'object'
    ? (raw.context_snapshot as SessionContextSnapshot)
    : null;

  return {
    binding_id: requireStringField(raw, 'binding_id'),
    session_id: String(raw.session_id ?? ''),
    session_title: raw.session_title != null ? String(raw.session_title) : null,
    last_activity: raw.last_activity == null ? null : Number(raw.last_activity),
    vfs: (raw.vfs as ProjectSessionInfo['vfs']) ?? null,
    runtime_surface: (raw.runtime_surface as ProjectSessionInfo['runtime_surface']) ?? null,
    context_snapshot: contextSnapshot,
  };
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

  fetchProjectAgentConfigs: async (projectId) => {
    try {
      const projectAgents = await api.get<ProjectAgent[]>(`/projects/${projectId}/agents`);
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
      const projectAgent = await api.post<ProjectAgent>(`/projects/${projectId}/agents`, payload);
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
      const projectAgent = await api.put<ProjectAgent>(`/projects/${projectId}/agents/${agentId}`, payload);
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
      await api.delete(`/projects/${projectId}/agents/${agentId}`);
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
      const projects = await api.get<Project[]>('/projects');
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
      const project = await api.post<Project>('/projects', {
        name,
        description,
        config: config ?? {
          agent_presets: [],
          context_containers: [],
        },
      });
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
      const project = await api.put<Project>(`/projects/${id}`, payload);
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
      const project = await api.put<Project>(`/projects/${id}`, {
        config,
      });
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
      const grants = await api.get<ProjectSubjectGrant[]>(`/projects/${projectId}/grants`);
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
      const grant = await api.put<ProjectSubjectGrant>(`/projects/${projectId}/grants/users/${encodeURIComponent(userId)}`, {
        role,
      });
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
      await api.delete(`/projects/${projectId}/grants/users/${encodeURIComponent(userId)}`);
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
      const grant = await api.put<ProjectSubjectGrant>(`/projects/${projectId}/grants/groups/${encodeURIComponent(groupId)}`, {
        role,
      });
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
      await api.delete(`/projects/${projectId}/grants/groups/${encodeURIComponent(groupId)}`);
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
      const project = await api.post<Project>(`/projects/${projectId}/clone`, payload ?? {});
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
      const response = await api.get<Record<string, unknown>[]>(
        `/projects/${projectId}/agents/summary`,
      );
      const agents = response.map(mapProjectAgentSummary);
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
      const response = await api.post<Record<string, unknown>>(
        `/projects/${projectId}/agents/${encodeURIComponent(agentKey)}/session`,
        {},
      );
      const result = mapOpenProjectAgentSessionResult(response);
      set((state) => {
        const existing = state.agentsByProjectId[projectId] ?? [];
        const hasExisting = existing.some((item) => item.key === result.agent.key);
        const nextAgents = hasExisting
          ? existing.map((item) => (item.key === result.agent.key ? result.agent : item))
          : [...existing, result.agent];
        return {
          agentsByProjectId: {
            ...state.agentsByProjectId,
            [projectId]: nextAgents,
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

  forceNewProjectAgentSession: async (projectId, agentKey) => {
    try {
      const response = await api.post<Record<string, unknown>>(
        `/projects/${projectId}/agents/${encodeURIComponent(agentKey)}/session?force_new=true`,
        {},
      );
      const result = mapOpenProjectAgentSessionResult(response);
      set((state) => {
        const existing = state.agentsByProjectId[projectId] ?? [];
        const hasExisting = existing.some((item) => item.key === result.agent.key);
        const nextAgents = hasExisting
          ? existing.map((item) => (item.key === result.agent.key ? result.agent : item))
          : [...existing, result.agent];
        return {
          agentsByProjectId: {
            ...state.agentsByProjectId,
            [projectId]: nextAgents,
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

  fetchProjectAgentSessions: async (projectId, agentKey) => {
    try {
      const response = await api.get<Record<string, unknown>[]>(
        `/projects/${projectId}/agents/${encodeURIComponent(agentKey)}/sessions`,
      );
      return response.map((raw) => ({
        binding_id: requireStringField(raw, 'binding_id'),
        session_id: String(raw.session_id ?? ''),
        session_title: raw.session_title != null ? String(raw.session_title) : null,
        last_activity: raw.last_activity != null ? Number(raw.last_activity) : null,
      }));
    } catch (e) {
      set({ error: (e as Error).message });
      return [];
    }
  },

  fetchProjectSessionInfo: async (projectId, bindingId) => {
    try {
      const raw = await api.get<Record<string, unknown>>(`/projects/${projectId}/sessions/${bindingId}`);
      return mapProjectSessionInfo(raw);
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  selectProject: (id) => set({ currentProjectId: id }),

  deleteProject: async (id) => {
    try {
      await api.delete(`/projects/${id}`);
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
