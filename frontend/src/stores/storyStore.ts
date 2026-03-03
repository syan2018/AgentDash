import { create } from 'zustand';
import type { Story, Task, StoryContext, AgentBinding, StateChange } from '../types';
import { api } from '../api/client';

export interface CreateTaskInput {
  title: string;
  description?: string;
  workspace_id?: string | null;
  agent_binding?: AgentBinding;
}

export interface TaskSessionInfo {
  task_id: string;
  session_id: string | null;
  executor_session_id: string | null;
  task_status: Task["status"];
  agent_binding: AgentBinding;
  session_title: string | null;
  last_activity: number | null;
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
  fetchStoryById: (storyId: string) => Promise<Story | null>;
  createStory: (
    projectId: string,
    backendId: string,
    title: string,
    description?: string,
  ) => Promise<Story | null>;
  updateStory: (
    storyId: string,
    payload: {
      title?: string;
      description?: string;
      backend_id?: string;
      status?: Story["status"];
    },
  ) => Promise<Story | null>;
  deleteStory: (storyId: string) => Promise<void>;
  createTask: (storyId: string, payload: CreateTaskInput) => Promise<Task | null>;
  updateTask: (
    taskId: string,
    payload: {
      title?: string;
      description?: string;
      workspace_id?: string | null;
      status?: Task["status"];
      agent_binding?: AgentBinding;
    },
  ) => Promise<Task | null>;
  startTaskExecution: (
    taskId: string,
    payload?: {
      override_prompt?: string;
      executor_config?: Record<string, unknown>;
    },
  ) => Promise<Task | null>;
  continueTaskExecution: (
    taskId: string,
    payload?: {
      additional_prompt?: string;
      executor_config?: Record<string, unknown>;
    },
  ) => Promise<Task | null>;
  cancelTaskExecution: (taskId: string) => Promise<Task | null>;
  refreshTask: (taskId: string) => Promise<Task | null>;
  fetchTaskSession: (taskId: string) => Promise<TaskSessionInfo | null>;
  deleteTask: (taskId: string, storyId: string) => Promise<void>;
  selectStory: (id: string | null) => void;
  selectTask: (id: string | null) => void;
  fetchTasks: (storyId: string) => Promise<void>;
  handleStateChange: (change: StateChange) => void;
}

const normalizeStoryStatus = (value: string): Story['status'] => {
  switch (value) {
    case 'created':
    case 'draft':
      return 'draft';
    case 'context_ready':
      return 'ready';
    case 'decomposed':
      return 'review';  // decomposed 映射到 review（待验收）
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
    case 'pending':
      return 'pending';
    case 'assigned':
      return 'assigned';
    case 'running':
      return 'running';
    case 'awaiting_verification':
      return 'awaiting_verification';
    case 'completed':
      return 'completed';
    case 'failed':
      return 'failed';
    // 兼容旧数据
    case 'queued':
      return 'assigned';
    case 'succeeded':
    case 'skipped':
      return 'completed';
    case 'cancelled':
      return 'failed';
    default:
      return 'pending';
  }
};

const toBackendStoryStatus = (status: Story["status"]): string => {
  switch (status) {
    case "draft":
      return "created";
    case "ready":
      return "context_ready";
    case "running":
      return "executing";
    case "completed":
      return "completed";
    case "failed":
      return "failed";
    case "review":
      return "decomposed";
    case "cancelled":
      return "failed";
    default:
      return "created";
  }
};

const toBackendTaskStatus = (status: Task["status"]): string => {
  switch (status) {
    case "pending":
      return "pending";
    case "assigned":
      return "assigned";
    case "running":
      return "running";
    case "awaiting_verification":
      return "awaiting_verification";
    case "completed":
      return "completed";
    case "failed":
      return "failed";
    default:
      return "pending";
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

const defaultBinding: AgentBinding = {
  agent_type: null,
  agent_pid: null,
  preset_name: null,
  prompt_template: null,
  initial_context: null,
};

const mapAgentBinding = (raw: unknown): AgentBinding => {
  if (!raw || typeof raw !== 'object') {
    return { ...defaultBinding };
  }

  const binding = raw as Record<string, unknown>;
  return {
    agent_type: binding.agent_type ? String(binding.agent_type) : null,
    agent_pid: binding.agent_pid ? String(binding.agent_pid) : null,
    preset_name: binding.preset_name ? String(binding.preset_name) : null,
    prompt_template: binding.prompt_template ? String(binding.prompt_template) : null,
    initial_context: binding.initial_context ? String(binding.initial_context) : null,
  };
};

const normalizeArtifactType = (value: string): Task['artifacts'][number]['artifact_type'] => {
  switch (value) {
    case 'code_change':
    case 'test_result':
    case 'log_output':
    case 'file':
    case 'tool_execution':
      return value;
    default:
      return 'log_output';
  }
};

const mapArtifact = (raw: Record<string, unknown>): Task['artifacts'][number] => {
  return {
    id: String(raw.id ?? ''),
    artifact_type: normalizeArtifactType(String(raw.artifact_type ?? 'log_output')),
    content: raw.content ?? null,
    created_at: String(raw.created_at ?? new Date().toISOString()),
  };
};

const upsertTaskInMap = (
  tasksByStoryId: Record<string, Task[]>,
  task: Task,
): Record<string, Task[]> => {
  const next = { ...tasksByStoryId };
  const existing = next[task.story_id] ?? [];
  if (existing.some((item) => item.id === task.id)) {
    next[task.story_id] = existing.map((item) => (item.id === task.id ? task : item));
  } else {
    next[task.story_id] = [task, ...existing];
  }
  return next;
};

const mapTask = (raw: Record<string, unknown>): Task => {
  return {
    id: String(raw.id ?? ''),
    story_id: String(raw.story_id ?? ''),
    workspace_id: raw.workspace_id ? String(raw.workspace_id) : null,
    session_id: raw.session_id ? String(raw.session_id) : null,
    executor_session_id: raw.executor_session_id ? String(raw.executor_session_id) : null,
    title: String(raw.title ?? raw.name ?? '未命名 Task'),
    description: raw.description ? String(raw.description) : '',
    status: normalizeTaskStatus(String(raw.status ?? 'pending')),
    agent_binding: mapAgentBinding(raw.agent_binding),
    artifacts: Array.isArray(raw.artifacts)
      ? raw.artifacts
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === 'object')
          .map((item) => mapArtifact(item))
      : [],
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

  fetchStoryById: async (storyId) => {
    try {
      const raw = await api.get<Record<string, unknown>>(`/stories/${storyId}`);
      const story = mapStory(raw);
      set((s) => {
        const existingIndex = s.stories.findIndex((item) => item.id === story.id);
        if (existingIndex >= 0) {
          const nextStories = [...s.stories];
          nextStories[existingIndex] = story;
          return { stories: nextStories };
        }
        return { stories: [story, ...s.stories] };
      });
      return story;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
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
      return story;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  updateStory: async (storyId, payload) => {
    try {
      const requestPayload = {
        ...payload,
        status: payload.status ? toBackendStoryStatus(payload.status) : undefined,
      };
      const raw = await api.put<Record<string, unknown>>(`/stories/${storyId}`, requestPayload);
      const story = mapStory(raw);
      set((s) => ({
        stories: s.stories.map((item) => (item.id === storyId ? story : item)),
      }));
      return story;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  deleteStory: async (storyId) => {
    try {
      await api.delete(`/stories/${storyId}`);
      set((s) => {
        const nextTasks = { ...s.tasksByStoryId };
        delete nextTasks[storyId];
        return {
          stories: s.stories.filter((story) => story.id !== storyId),
          tasksByStoryId: nextTasks,
          selectedStoryId: s.selectedStoryId === storyId ? null : s.selectedStoryId,
        };
      });
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

  updateTask: async (taskId, payload) => {
    try {
      const requestPayload = {
        ...payload,
        workspace_id: payload.workspace_id === null ? "" : payload.workspace_id,
        status: payload.status ? toBackendTaskStatus(payload.status) : undefined,
      };
      const raw = await api.put<Record<string, unknown>>(`/tasks/${taskId}`, requestPayload);
      const task = mapTask(raw);
      set((s) => {
        return { tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) };
      });
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  startTaskExecution: async (taskId, payload) => {
    try {
      await api.post<Record<string, unknown>>(`/tasks/${taskId}/start`, payload ?? {});
      const rawTask = await api.get<Record<string, unknown>>(`/tasks/${taskId}`);
      const task = mapTask(rawTask);
      set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  continueTaskExecution: async (taskId, payload) => {
    try {
      await api.post<Record<string, unknown>>(`/tasks/${taskId}/continue`, payload ?? {});
      const rawTask = await api.get<Record<string, unknown>>(`/tasks/${taskId}`);
      const task = mapTask(rawTask);
      set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  cancelTaskExecution: async (taskId) => {
    try {
      const rawTask = await api.post<Record<string, unknown>>(`/tasks/${taskId}/cancel`, {});
      const task = mapTask(rawTask);
      set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  refreshTask: async (taskId) => {
    try {
      const rawTask = await api.get<Record<string, unknown>>(`/tasks/${taskId}`);
      const task = mapTask(rawTask);
      set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  fetchTaskSession: async (taskId) => {
    try {
      const raw = await api.get<Record<string, unknown>>(`/tasks/${taskId}/session`);
      return {
        task_id: String(raw.task_id ?? taskId),
        session_id: raw.session_id ? String(raw.session_id) : null,
        executor_session_id: raw.executor_session_id ? String(raw.executor_session_id) : null,
        task_status: normalizeTaskStatus(String(raw.task_status ?? 'pending')),
        agent_binding: mapAgentBinding(raw.agent_binding),
        session_title: raw.session_title ? String(raw.session_title) : null,
        last_activity: raw.last_activity == null ? null : Number(raw.last_activity),
      };
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  deleteTask: async (taskId, storyId) => {
    try {
      await api.delete(`/tasks/${taskId}`);
      set((s) => {
        const existing = s.tasksByStoryId[storyId] ?? [];
        return {
          tasksByStoryId: {
            ...s.tasksByStoryId,
            [storyId]: existing.filter((task) => task.id !== taskId),
          },
          selectedTaskId: s.selectedTaskId === taskId ? null : s.selectedTaskId,
        };
      });
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

  handleStateChange: (change: StateChange) => {
    const entityId = change.entity_id;
    const payload = change.payload as Record<string, unknown>;

    switch (change.kind) {
      case 'story_created':
      case 'story_updated':
      case 'story_status_changed': {
        api
          .get<Record<string, unknown>>(`/stories/${entityId}`)
          .then((raw) => {
            const story = mapStory(raw);
            set((s) => {
              const idx = s.stories.findIndex((item) => item.id === story.id);
              if (idx >= 0) {
                const next = [...s.stories];
                next[idx] = story;
                return { stories: next };
              }
              return { stories: [story, ...s.stories] };
            });
          })
          .catch(() => {});
        break;
      }
      case 'story_deleted': {
        set((s) => {
          const nextTasks = { ...s.tasksByStoryId };
          delete nextTasks[entityId];
          return {
            stories: s.stories.filter((item) => item.id !== entityId),
            tasksByStoryId: nextTasks,
            selectedStoryId:
              s.selectedStoryId === entityId ? null : s.selectedStoryId,
          };
        });
        break;
      }
      case 'task_created':
      case 'task_updated':
      case 'task_status_changed':
      case 'task_artifact_added': {
        api
          .get<Record<string, unknown>>(`/tasks/${entityId}`)
          .then((raw) => {
            const task = mapTask(raw);
            set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
          })
          .catch(() => {});
        break;
      }
      case 'task_deleted': {
        const storyId = payload.story_id ? String(payload.story_id) : null;
        if (storyId) {
          set((s) => {
            const existing = s.tasksByStoryId[storyId] ?? [];
            return {
              tasksByStoryId: {
                ...s.tasksByStoryId,
                [storyId]: existing.filter((t) => t.id !== entityId),
              },
              selectedTaskId:
                s.selectedTaskId === entityId ? null : s.selectedTaskId,
            };
          });
        }
        break;
      }
    }
  },
}));
