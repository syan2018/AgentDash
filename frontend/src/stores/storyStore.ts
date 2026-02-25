import { create } from 'zustand';
import type { Story, Task } from '../types';
import { api } from '../api/client';

interface StoryState {
  stories: Story[];
  tasksByStoryId: Record<string, Task[]>;
  selectedStoryId: string | null;
  selectedTaskId: string | null;
  isLoading: boolean;
  error: string | null;

  fetchStories: (backendId: string) => Promise<void>;
  createStory: (backendId: string, title: string, description?: string) => Promise<void>;
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

const mapStory = (raw: Record<string, unknown>): Story => {
  const taskIdsRaw = raw.task_ids;
  return {
    id: String(raw.id ?? ''),
    backendId: String(raw.backend_id ?? ''),
    title: String(raw.title ?? '未命名 Story'),
    description: raw.description ? String(raw.description) : '',
    status: normalizeStoryStatus(String(raw.status ?? 'draft')),
    context:
      raw.context && typeof raw.context === 'object'
        ? (raw.context as Story['context'])
        : { items: [], metadata: {} },
    taskIds: Array.isArray(taskIdsRaw) ? taskIdsRaw.map((id) => String(id)) : [],
    createdAt: String(raw.created_at ?? new Date().toISOString()),
    updatedAt: String(raw.updated_at ?? raw.created_at ?? new Date().toISOString()),
  };
};

const mapTask = (raw: Record<string, unknown>): Task => {
  return {
    id: String(raw.id ?? ''),
    storyId: String(raw.story_id ?? ''),
    title: String(raw.title ?? raw.name ?? '未命名 Task'),
    description: raw.description ? String(raw.description) : '',
    agentType: (String(raw.agent_type ?? 'worker') as Task['agentType']),
    status: normalizeTaskStatus(String(raw.status ?? 'pending')),
    context:
      raw.context && typeof raw.context === 'object'
        ? (raw.context as Task['context'])
        : { items: [], metadata: {} },
    agentBinding: raw.agent_type
      ? {
          agentType: String(raw.agent_type) as Task['agentType'],
          agentPid: raw.agent_pid ? String(raw.agent_pid) : null,
          workspacePath: raw.workspace_path ? String(raw.workspace_path) : null,
        }
      : null,
    artifacts: Array.isArray(raw.artifacts) ? (raw.artifacts as Task['artifacts']) : [],
    executionTrace: Array.isArray(raw.execution_trace)
      ? (raw.execution_trace as Task['executionTrace'])
      : [],
    createdAt: String(raw.created_at ?? new Date().toISOString()),
    updatedAt: String(raw.updated_at ?? raw.created_at ?? new Date().toISOString()),
  };
};

export const useStoryStore = create<StoryState>((set) => ({
  stories: [],
  tasksByStoryId: {},
  selectedStoryId: null,
  selectedTaskId: null,
  isLoading: false,
  error: null,

  fetchStories: async (backendId) => {
    set({ isLoading: true, error: null });
    try {
      const response = await api.get<Record<string, unknown>[]>(`/stories?backend_id=${backendId}`);
      const stories = response.map(mapStory);
      set({ stories, isLoading: false });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  createStory: async (backendId, title, description) => {
    try {
      const raw = await api.post<Record<string, unknown>>('/stories', {
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
