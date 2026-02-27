import { create } from 'zustand';
import type { Story, Task, StoryContext, AgentBinding } from '../types';
import { api } from '../api/client';

export interface CreateTaskInput {
  title: string;
  description?: string;
  workspace_id?: string | null;
  agent_binding?: {
    agent_type?: string | null;
    agent_pid?: string | null;
    preset_name?: string | null;
  };
}

interface StoryState {
  stories: Story[];
  tasksByStoryId: Record<string, Task[]>;
  selectedStoryId: string | null;
  selectedTaskId: string | null;
  isLoading: boolean;
  error: string | null;

  fetchStoriesByProject: (projectId: string) => Promise<void>;
  fetchStoriesByBackend: (backendId: string) => Promise<void>;
  createStory: (projectId: string, backendId: string, title: string, description?: string) => Promise<void>;
  createTask: (storyId: string, payload: CreateTaskInput) => Promise<Task | null>;
  selectStory: (id: string | null) => void;
  selectTask: (id: string | null) => void;
  fetchTasks: (storyId: string) => Promise<void>;
}

const normalizeStoryStatus = (value: string): Story['status'] => {
  switch (value) {
    case 'created':
    case 'draft':
      return 'draft';
    case 'context_ready':
      return 'ready';
    case 'decomposed':
    case 'ready':
      return 'ready';
    case 'executing':
      return 'running';
    case 'awaiting_verification':
      return 'review';
    case 'completed':
      return 'completed';
    case 'failed':
      return 'failed';
    case 'cancelled':
      return 'cancelled';
    default:
      return 'draft';
  }
};

const normalizeTaskStatus = (value: string): Task['status'] => {
  switch (value) {
    case 'assigned':
    case 'pending':
      return 'pending';
    case 'queued':
      return 'queued';
    case 'running':
      return 'running';
    case 'completed':
    case 'succeeded':
      return 'succeeded';
    case 'failed':
      return 'failed';
    case 'skipped':
      return 'skipped';
    case 'cancelled':
      return 'cancelled';
    default:
      return 'pending';
  }
};

const defaultContext: StoryContext = { prd_doc: null, spec_refs: [], resource_list: [] };

const mapStory = (raw: Record<string, unknown>): Story => {
  let context: StoryContext = defaultContext;
  if (raw.context && typeof raw.context === 'object') {
    const ctx = raw.context as Record<string, unknown>;
    if ('spec_refs' in ctx || 'prd_doc' in ctx || 'resource_list' in ctx) {
      context = {
        prd_doc: (ctx.prd_doc as string) ?? null,
        spec_refs: Array.isArray(ctx.spec_refs) ? ctx.spec_refs as string[] : [],
        resource_list: Array.isArray(ctx.resource_list) ? ctx.resource_list as StoryContext['resource_list'] : [],
      };
    }
  }

  return {
    id: String(raw.id ?? ''),
    project_id: String(raw.project_id ?? ''),
    backend_id: String(raw.backend_id ?? ''),
    title: String(raw.title ?? '未命名 Story'),
    description: raw.description ? String(raw.description) : '',
    status: normalizeStoryStatus(String(raw.status ?? 'draft')),
    context,
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? raw.created_at ?? new Date().toISOString()),
  };
};

const defaultBinding: AgentBinding = { agent_type: null, agent_pid: null, preset_name: null };

const mapTask = (raw: Record<string, unknown>): Task => {
  let agentBinding: AgentBinding = defaultBinding;
  if (raw.agent_binding && typeof raw.agent_binding === 'object') {
    const ab = raw.agent_binding as Record<string, unknown>;
    agentBinding = {
      agent_type: ab.agent_type ? String(ab.agent_type) : null,
      agent_pid: ab.agent_pid ? String(ab.agent_pid) : null,
      preset_name: ab.preset_name ? String(ab.preset_name) : null,
    };
  }

  return {
    id: String(raw.id ?? ''),
    story_id: String(raw.story_id ?? ''),
    workspace_id: raw.workspace_id ? String(raw.workspace_id) : null,
    title: String(raw.title ?? raw.name ?? '未命名 Task'),
    description: raw.description ? String(raw.description) : '',
    status: normalizeTaskStatus(String(raw.status ?? 'pending')),
    agent_binding: agentBinding,
    artifacts: Array.isArray(raw.artifacts) ? (raw.artifacts as Task['artifacts']) : [],
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? raw.created_at ?? new Date().toISOString()),
  };
};

export const useStoryStore = create<StoryState>((set) => ({
  stories: [],
  tasksByStoryId: {},
  selectedStoryId: null,
  selectedTaskId: null,
  isLoading: false,
  error: null,

  fetchStoriesByProject: async (projectId) => {
    set({ isLoading: true, error: null });
    try {
      const response = await api.get<Record<string, unknown>[]>(`/stories?project_id=${projectId}`);
      const stories = response.map(mapStory);
      set({ stories, isLoading: false });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  fetchStoriesByBackend: async (backendId) => {
    set({ isLoading: true, error: null });
    try {
      const response = await api.get<Record<string, unknown>[]>(`/stories?backend_id=${backendId}`);
      const stories = response.map(mapStory);
      set({ stories, isLoading: false });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  createStory: async (projectId, backendId, title, description) => {
    try {
      const raw = await api.post<Record<string, unknown>>('/stories', {
        project_id: projectId,
        backend_id: backendId,
        title,
        description,
      });
      const story = mapStory(raw);
      set((s) => ({ stories: [story, ...s.stories] }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  createTask: async (storyId, payload) => {
    try {
      const raw = await api.post<Record<string, unknown>>(`/stories/${storyId}/tasks`, payload);
      const task = mapTask(raw);
      set((s) => {
        const existing = s.tasksByStoryId[storyId] ?? [];
        return {
          tasksByStoryId: {
            ...s.tasksByStoryId,
            [storyId]: [task, ...existing],
          },
        };
      });
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  selectStory: (id) => set({ selectedStoryId: id }),
  selectTask: (id) => set({ selectedTaskId: id }),

  fetchTasks: async (storyId) => {
    try {
      const response = await api.get<Record<string, unknown>[]>(`/stories/${storyId}/tasks`);
      const tasks = response.map(mapTask);
      set((s) => ({
        tasksByStoryId: { ...s.tasksByStoryId, [storyId]: tasks },
      }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },
}));
