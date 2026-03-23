import { create } from 'zustand';
import type {
  ContextContainerDefinition,
  MountDerivationPolicy,
  OpenProjectAgentSessionResult,
  ProjectAgentSession,
  ProjectSessionInfo,
  SessionContextSnapshot,
  ProjectAgentSummary,
  Project,
  ProjectConfig,
  SessionComposition,
} from '../types';
import { isThinkingLevel } from '../types';
import { api } from '../api/client';

interface ProjectState {
  projects: Project[];
  agentsByProjectId: Record<string, ProjectAgentSummary[]>;
  currentProjectId: string | null;
  isLoading: boolean;
  error: string | null;

  fetchProjects: () => Promise<void>;
  createProject: (name: string, description: string, config?: Partial<ProjectConfig>) => Promise<Project | null>;
  updateProject: (id: string, payload: {
    name?: string;
    description?: string;
    config?: ProjectConfig;
    context_containers?: ContextContainerDefinition[];
    mount_policy?: MountDerivationPolicy;
    session_composition?: SessionComposition;
  }) => Promise<Project | null>;
  updateProjectConfig: (id: string, config: Partial<ProjectConfig>) => Promise<Project | null>;
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
  const rawMountsSource = raw.shared_context_mounts;
  const rawMounts = Array.isArray(rawMountsSource)
    ? rawMountsSource as Record<string, unknown>[]
    : [];
  const thinkingLevel = isThinkingLevel(rawExecutor.thinking_level)
    ? rawExecutor.thinking_level
    : null;

  return {
    key: String(raw.key ?? ''),
    display_name: String(raw.display_name ?? '未命名 Agent'),
    description: String(raw.description ?? ''),
    executor: {
      executor: String(rawExecutor.executor ?? ''),
      variant: rawExecutor.variant != null ? String(rawExecutor.variant) : null,
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
    writeback_mode: raw.writeback_mode === 'confirm_before_write' ? 'confirm_before_write' : 'read_only',
    shared_context_mounts: rawMounts.map((mount) => ({
      container_id: String(mount.container_id ?? ''),
      mount_id: String(mount.mount_id ?? ''),
      display_name: String(mount.display_name ?? mount.mount_id ?? ''),
      writable: Boolean(mount.writable),
    })),
    session: rawSession
      ? {
          binding_id: String(rawSession.binding_id ?? ''),
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
    binding_id: String(raw.binding_id ?? ''),
    agent: mapProjectAgentSummary(rawAgent),
  };
}

function mapProjectSessionInfo(raw: Record<string, unknown>, fallbackBindingId: string): ProjectSessionInfo {
  const contextSnapshot = raw.context_snapshot && typeof raw.context_snapshot === 'object'
    ? (raw.context_snapshot as SessionContextSnapshot)
    : null;

  return {
    binding_id: String(raw.binding_id ?? fallbackBindingId),
    session_id: String(raw.session_id ?? ''),
    session_title: raw.session_title != null ? String(raw.session_title) : null,
    last_activity: raw.last_activity == null ? null : Number(raw.last_activity),
    address_space: (raw.address_space as ProjectSessionInfo['address_space']) ?? null,
    context_snapshot: contextSnapshot,
  };
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  projects: [],
  agentsByProjectId: {},
  currentProjectId: null,
  isLoading: false,
  error: null,

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
          mount_policy: { include_local_workspace: true, local_workspace_capabilities: [] },
          session_composition: { workflow_steps: [], required_context_blocks: [] },
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

  fetchProjectAgents: async (projectId) => {
    try {
      const response = await api.get<Record<string, unknown>[]>(`/projects/${projectId}/agents`);
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
        binding_id: String(raw.binding_id ?? ''),
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
      return mapProjectSessionInfo(raw, bindingId);
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
        delete nextAgentsByProjectId[id];
        return {
          projects: remaining,
          agentsByProjectId: nextAgentsByProjectId,
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
