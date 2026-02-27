import { create } from 'zustand';
import type { Project, ProjectConfig } from '../types';
import { api } from '../api/client';

interface ProjectState {
  projects: Project[];
  currentProjectId: string | null;
  isLoading: boolean;
  error: string | null;

  fetchProjects: () => Promise<void>;
  createProject: (name: string, description: string, backendId: string, config?: Partial<ProjectConfig>) => Promise<Project | null>;
  updateProject: (id: string, payload: { name?: string; description?: string; backend_id?: string; config?: ProjectConfig }) => Promise<Project | null>;
  updateProjectConfig: (id: string, config: ProjectConfig) => Promise<Project | null>;
  selectProject: (id: string | null) => void;
  deleteProject: (id: string) => Promise<void>;
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  projects: [],
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

  createProject: async (name, description, backendId, config) => {
    try {
      const project = await api.post<Project>('/projects', {
        name,
        description,
        backend_id: backendId,
        config: config ?? { agent_presets: [] },
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

  selectProject: (id) => set({ currentProjectId: id }),

  deleteProject: async (id) => {
    try {
      await api.delete(`/projects/${id}`);
      set((s) => ({
        projects: s.projects.filter((p) => p.id !== id),
        currentProjectId: s.currentProjectId === id ? null : s.currentProjectId,
      }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },
}));
