import { create } from 'zustand';
import type {
  Story,
  Task,
  StoryContext,
  AgentBinding,
  StateChange,
  SessionBinding,
  ContextSourceRef,
  ContextContainerDefinition,
  ExecutionAddressSpace,
  SessionComposition,
  StorySessionInfo,
  SessionContextSnapshot,
} from '../types';
import { isThinkingLevel } from '../types';
import { api } from '../api/client';

export interface CreateTaskInput {
  title: string;
  description?: string;
  workspace_id?: string | null;
  agent_binding?: AgentBinding;
}

export interface TaskSessionInfo {
  task_id: string;
  workspace_id: string | null;
  session_id: string | null;
  executor_session_id: string | null;
  task_status: Task["status"];
  agent_binding: AgentBinding;
  session_title: string | null;
  last_activity: number | null;
  address_space: ExecutionAddressSpace | null;
  context_snapshot: SessionContextSnapshot | null;
}

export interface CreateStorySessionInput {
  session_id?: string;
  title?: string;
  label?: string;
}

interface StoryState {
  storiesByProjectId: Record<string, Story[]>;
  tasksByStoryId: Record<string, Task[]>;
  sessionsByStoryId: Record<string, SessionBinding[]>;
  selectedStoryId: string | null;
  selectedTaskId: string | null;
  isLoading: boolean;
  error: string | null;

  fetchStoriesByProject: (projectId: string) => Promise<void>;
  fetchStoryById: (storyId: string) => Promise<Story | null>;
  createStory: (
    projectId: string,
    title: string,
    description?: string,
    options?: {
      priority?: Story["priority"];
      story_type?: Story["story_type"];
      tags?: string[];
    },
  ) => Promise<Story | null>;
  updateStory: (
    storyId: string,
    payload: {
      title?: string;
      description?: string;
      default_workspace_id?: string | null;
      status?: Story["status"];
      priority?: Story["priority"];
      story_type?: Story["story_type"];
      tags?: string[];
      context_source_refs?: ContextSourceRef[];
      context_containers?: ContextContainerDefinition[];
      disabled_container_ids?: string[];
      session_composition?: SessionComposition | null;
      clear_session_composition?: boolean;
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
  fetchStorySessionInfo: (storyId: string, bindingId: string) => Promise<StorySessionInfo | null>;
  deleteTask: (taskId: string, storyId: string) => Promise<void>;
  selectStory: (id: string | null) => void;
  selectTask: (id: string | null) => void;
  fetchTasks: (storyId: string) => Promise<void>;
  fetchStorySessions: (storyId: string) => Promise<void>;
  createStorySession: (storyId: string, input: CreateStorySessionInput) => Promise<SessionBinding | null>;
  unbindStorySession: (storyId: string, bindingId: string) => Promise<void>;
  handleStateChange: (change: StateChange) => void;
}

const requireStringField = (raw: Record<string, unknown>, field: string): string => {
  const value = raw[field];
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`缺少或非法的字段 ${field}`);
  }
  return value;
};

const readNullableStringField = (raw: Record<string, unknown>, field: string): string | null => {
  const value = raw[field];
  if (value == null) {
    return null;
  }
  if (typeof value === 'string') {
    return value;
  }
  throw new Error(`字段 ${field} 必须是字符串或 null`);
};

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
    case 'canceled':
      return 'cancelled';
    default:
      throw new Error(`未知 Story 状态: ${value}`);
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
    default:
      throw new Error(`未知 Task 状态: ${value}`);
  }
};

const normalizeTaskExecutionMode = (value: unknown): Task['execution_mode'] => {
  switch (value) {
    case 'standard':
      return 'standard';
    case 'auto_retry':
      return 'auto_retry';
    case 'one_shot':
      return 'one_shot';
    default:
      throw new Error(`未知 Task execution_mode: ${String(value)}`);
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
      return "cancelled";
    default:
      throw new Error(`未知 Story 状态: ${String(status)}`);
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
      throw new Error(`未知 Task 状态: ${String(status)}`);
  }
};

const defaultContext: StoryContext = {
  prd_doc: null,
  spec_refs: [],
  resource_list: [],
  source_refs: [],
  context_containers: [],
  disabled_container_ids: [],
  session_composition: null,
};

const normalizeStoryPriority = (value: string): Story['priority'] => {
  switch (value) {
    case 'p0':
    case 'critical':
      return 'p0';
    case 'p1':
    case 'high':
      return 'p1';
    case 'p2':
    case 'medium':
      return 'p2';
    case 'p3':
    case 'low':
      return 'p3';
    default:
      throw new Error(`未知 Story priority: ${value}`);
  }
};

const normalizeStoryType = (value: string): Story['story_type'] => {
  switch (value) {
    case 'feature':
      return 'feature';
    case 'bugfix':
    case 'bug':
      return 'bugfix';
    case 'refactor':
      return 'refactor';
    case 'docs':
    case 'documentation':
      return 'docs';
    case 'test':
      return 'test';
    default:
      throw new Error(`未知 Story story_type: ${value}`);
  }
};

const mapStory = (raw: Record<string, unknown>): Story => {
  let context: StoryContext = defaultContext;
  if (raw.context != null) {
    if (typeof raw.context !== 'object') {
      throw new Error('Story context 必须是对象');
    }
    const ctx = raw.context as Record<string, unknown>;
    if (ctx.spec_refs != null && !Array.isArray(ctx.spec_refs)) {
      throw new Error('Story context.spec_refs 必须是数组');
    }
    if (ctx.resource_list != null && !Array.isArray(ctx.resource_list)) {
      throw new Error('Story context.resource_list 必须是数组');
    }
    if (ctx.source_refs != null && !Array.isArray(ctx.source_refs)) {
      throw new Error('Story context.source_refs 必须是数组');
    }
    if (ctx.context_containers != null && !Array.isArray(ctx.context_containers)) {
      throw new Error('Story context.context_containers 必须是数组');
    }
    if (ctx.disabled_container_ids != null && !Array.isArray(ctx.disabled_container_ids)) {
      throw new Error('Story context.disabled_container_ids 必须是数组');
    }
    context = {
      prd_doc: ctx.prd_doc == null ? null : requireStringField({ prd_doc: ctx.prd_doc }, 'prd_doc'),
      spec_refs: ctx.spec_refs == null ? [] : ctx.spec_refs as string[],
      resource_list: ctx.resource_list == null ? [] : ctx.resource_list as StoryContext['resource_list'],
      source_refs: ctx.source_refs == null ? [] : ctx.source_refs as ContextSourceRef[],
      context_containers: ctx.context_containers == null
        ? []
        : ctx.context_containers as ContextContainerDefinition[],
      disabled_container_ids: ctx.disabled_container_ids == null
        ? []
        : ctx.disabled_container_ids as string[],
      session_composition: ctx.session_composition == null
        ? null
        : ctx.session_composition as SessionComposition,
    };
  }

  return {
    id: requireStringField(raw, 'id'),
    project_id: requireStringField(raw, 'project_id'),
    default_workspace_id: raw.default_workspace_id != null ? String(raw.default_workspace_id) : null,
    title: requireStringField(raw, 'title'),
    description: raw.description ? String(raw.description) : '',
    status: normalizeStoryStatus(requireStringField(raw, 'status')),
    priority: normalizeStoryPriority(requireStringField(raw, 'priority')),
    story_type: normalizeStoryType(requireStringField(raw, 'story_type')),
    tags: Array.isArray(raw.tags) ? raw.tags.filter((t): t is string => typeof t === 'string') : [],
    task_count: Number.isFinite(Number(raw.task_count ?? 0)) ? Number(raw.task_count ?? 0) : 0,
    context,
    created_at: requireStringField(raw, 'created_at'),
    updated_at: requireStringField(raw, 'updated_at'),
  };
};

const mapAgentBinding = (raw: unknown): AgentBinding => {
  if (!raw || typeof raw !== 'object') {
    throw new Error('Task 缺少 agent_binding');
  }

  const binding = raw as Record<string, unknown>;
  return {
    agent_type: binding.agent_type ? String(binding.agent_type) : null,
    agent_pid: binding.agent_pid ? String(binding.agent_pid) : null,
    preset_name: binding.preset_name ? String(binding.preset_name) : null,
    prompt_template: binding.prompt_template ? String(binding.prompt_template) : null,
    initial_context: binding.initial_context ? String(binding.initial_context) : null,
    thinking_level: binding.thinking_level == null
      ? null
      : isThinkingLevel(binding.thinking_level)
        ? binding.thinking_level
        : (() => { throw new Error(`未知 thinking_level: ${String(binding.thinking_level)}`); })(),
    context_sources: Array.isArray(binding.context_sources)
      ? binding.context_sources as ContextSourceRef[]
      : (() => { throw new Error('agent_binding.context_sources 必须是数组'); })(),
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
      throw new Error(`未知 artifact_type: ${value}`);
  }
};

const mapArtifact = (raw: Record<string, unknown>): Task['artifacts'][number] => {
  return {
    id: requireStringField(raw, 'id'),
    artifact_type: normalizeArtifactType(requireStringField(raw, 'artifact_type')),
    content: raw.content ?? null,
    created_at: requireStringField(raw, 'created_at'),
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

const upsertStoryInList = (stories: Story[], story: Story): Story[] => {
  const existingIndex = stories.findIndex((item) => item.id === story.id);
  if (existingIndex >= 0) {
    const nextStories = [...stories];
    nextStories[existingIndex] = story;
    return nextStories;
  }
  return [story, ...stories];
};

const upsertStoryInProjectMap = (
  storiesByProjectId: Record<string, Story[]>,
  story: Story,
): Record<string, Story[]> => {
  const next = { ...storiesByProjectId };
  const projectStories = next[story.project_id] ?? [];
  next[story.project_id] = upsertStoryInList(projectStories, story);
  return next;
};

const removeStoryFromProjectMap = (
  storiesByProjectId: Record<string, Story[]>,
  storyId: string,
  projectIdHint?: string | null,
): Record<string, Story[]> => {
  const next = { ...storiesByProjectId };
  if (projectIdHint) {
    const existing = next[projectIdHint] ?? [];
    next[projectIdHint] = existing.filter((story) => story.id !== storyId);
    return next;
  }

  for (const [projectId, list] of Object.entries(next)) {
    const filtered = list.filter((story) => story.id !== storyId);
    if (filtered.length !== list.length) {
      next[projectId] = filtered;
    }
  }
  return next;
};

export const flattenStoriesMap = (storiesByProjectId: Record<string, Story[]>): Story[] => {
  const merged = Object.values(storiesByProjectId).flat();
  const dedup = new Map<string, Story>();
  for (const story of merged) {
    if (!dedup.has(story.id)) {
      dedup.set(story.id, story);
    }
  }
  return Array.from(dedup.values());
};

export const findStoryById = (
  storiesByProjectId: Record<string, Story[]>,
  storyId: string,
): Story | null => {
  for (const stories of Object.values(storiesByProjectId)) {
    const hit = stories.find((story) => story.id === storyId);
    if (hit) return hit;
  }
  return null;
};

const mapSessionBinding = (raw: Record<string, unknown>): SessionBinding => ({
  id: requireStringField(raw, 'id'),
  project_id: requireStringField(raw, 'project_id'),
  session_id: requireStringField(raw, 'session_id'),
  owner_type: requireStringField(raw, 'owner_type') as SessionBinding['owner_type'],
  owner_id: requireStringField(raw, 'owner_id'),
  label: requireStringField(raw, 'label'),
  created_at: requireStringField(raw, 'created_at'),
  session_title: raw.session_title != null
    ? String(raw.session_title)
    : undefined,
  session_updated_at: raw.session_updated_at != null
    ? Number(raw.session_updated_at)
    : undefined,
});

const requireStorySessionField = (raw: Record<string, unknown>, field: string): string => {
  const value = raw[field];
  if (typeof value === 'string' && value.length > 0) {
    return value;
  }
  throw new Error(`StorySessionInfo 缺少必填字段: ${field}`);
};

const storyRefreshInFlight = new Set<string>();
const taskRefreshInFlight = new Set<string>();

const mapTask = (raw: Record<string, unknown>): Task => {
  return {
    id: requireStringField(raw, 'id'),
    project_id: requireStringField(raw, 'project_id'),
    story_id: requireStringField(raw, 'story_id'),
    workspace_id: raw.workspace_id ? String(raw.workspace_id) : null,
    executor_session_id: raw.executor_session_id ? String(raw.executor_session_id) : null,
    title: requireStringField(raw, 'title'),
    description: raw.description ? String(raw.description) : '',
    status: normalizeTaskStatus(requireStringField(raw, 'status')),
    execution_mode: normalizeTaskExecutionMode(raw.execution_mode),
    agent_binding: mapAgentBinding(raw.agent_binding),
    artifacts: raw.artifacts == null
      ? []
      : Array.isArray(raw.artifacts)
        ? raw.artifacts.map((item, index) => {
            if (!item || typeof item !== 'object') {
              throw new Error(`Task artifact[${index}] 必须是对象`);
            }
            return mapArtifact(item as Record<string, unknown>);
          })
        : (() => { throw new Error('Task artifacts 必须是数组'); })(),
    created_at: requireStringField(raw, 'created_at'),
    updated_at: requireStringField(raw, 'updated_at'),
  };
};

const canMapStoryFromPayload = (payload: Record<string, unknown>): boolean => {
  return (
    typeof payload.id === 'string' &&
    typeof payload.title === 'string' &&
    typeof payload.project_id === 'string' &&
    typeof payload.status === 'string' &&
    payload.task_count !== undefined
  );
};

const canMapTaskFromPayload = (payload: Record<string, unknown>): boolean => {
  return (
    typeof payload.id === 'string' &&
    typeof payload.title === 'string' &&
    typeof payload.story_id === 'string' &&
    typeof payload.status === 'string'
  );
};

export const useStoryStore = create<StoryState>((set) => ({
  storiesByProjectId: {},
  tasksByStoryId: {},
  sessionsByStoryId: {},
  selectedStoryId: null,
  selectedTaskId: null,
  isLoading: false,
  error: null,

  fetchStoriesByProject: async (projectId) => {
    set({ isLoading: true, error: null });
    try {
      const response = await api.get<Record<string, unknown>[]>(`/stories?project_id=${projectId}`);
      const stories = response.map(mapStory);
      set((s) => {
        const storiesByProjectId = {
          ...s.storiesByProjectId,
          [projectId]: stories,
        };
        return {
          storiesByProjectId,
          isLoading: false,
        };
      });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  fetchStoryById: async (storyId) => {
    try {
      const raw = await api.get<Record<string, unknown>>(`/stories/${storyId}`);
      const story = mapStory(raw);
      set((s) => {
        const storiesByProjectId = upsertStoryInProjectMap(s.storiesByProjectId, story);
        return {
          storiesByProjectId,
        };
      });
      return story;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  createStory: async (projectId, title, description, options) => {
    try {
      const raw = await api.post<Record<string, unknown>>('/stories', {
        project_id: projectId,
        title,
        description,
        priority: options?.priority,
        story_type: options?.story_type,
        tags: options?.tags,
      });
      const story = mapStory(raw);
      set((s) => {
        const storiesByProjectId = upsertStoryInProjectMap(s.storiesByProjectId, story);
        return {
          storiesByProjectId,
        };
      });
      return story;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  updateStory: async (storyId, payload) => {
    try {
      const requestPayload: Record<string, unknown> = {
        ...payload,
      };
      if (payload.status) {
        requestPayload.status = toBackendStoryStatus(payload.status);
      }
      const raw = await api.put<Record<string, unknown>>(`/stories/${storyId}`, requestPayload);
      const story = mapStory(raw);
      set((s) => {
        const withoutOld = removeStoryFromProjectMap(s.storiesByProjectId, storyId);
        const storiesByProjectId = upsertStoryInProjectMap(withoutOld, story);
        return {
          storiesByProjectId,
        };
      });
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
        const deletedStory = findStoryById(s.storiesByProjectId, storyId);
        const storiesByProjectId = removeStoryFromProjectMap(
          s.storiesByProjectId,
          storyId,
          deletedStory?.project_id ?? null,
        );
        const nextTasks = { ...s.tasksByStoryId };
        delete nextTasks[storyId];
        const nextSessions = { ...s.sessionsByStoryId };
        delete nextSessions[storyId];
        return {
          storiesByProjectId,
          tasksByStoryId: nextTasks,
          sessionsByStoryId: nextSessions,
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
        workspace_id: payload.workspace_id,
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
        task_id: requireStringField(raw, 'task_id'),
        workspace_id: readNullableStringField(raw, 'workspace_id'),
        session_id: readNullableStringField(raw, 'session_id'),
        executor_session_id: readNullableStringField(raw, 'executor_session_id'),
        task_status: normalizeTaskStatus(requireStringField(raw, 'task_status')),
        agent_binding: mapAgentBinding(raw.agent_binding),
        session_title: readNullableStringField(raw, 'session_title'),
        last_activity: raw.last_activity == null ? null : Number(raw.last_activity),
        address_space: (raw.address_space as ExecutionAddressSpace) ?? null,
        context_snapshot: (raw.context_snapshot as SessionContextSnapshot) ?? null,
      };
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  fetchStorySessionInfo: async (storyId, bindingId) => {
    try {
      const raw = await api.get<Record<string, unknown>>(`/stories/${storyId}/sessions/${bindingId}`);
      return {
        binding_id: requireStorySessionField(raw, 'binding_id'),
        session_id: requireStorySessionField(raw, 'session_id'),
        session_title: readNullableStringField(raw, 'session_title'),
        last_activity: raw.last_activity == null ? null : Number(raw.last_activity),
        address_space: (raw.address_space as ExecutionAddressSpace) ?? null,
        context_snapshot: (raw.context_snapshot as StorySessionInfo['context_snapshot']) ?? null,
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

  fetchStorySessions: async (storyId) => {
    try {
      const response = await api.get<Record<string, unknown>[]>(`/stories/${storyId}/sessions`);
      const bindings = response.map(mapSessionBinding);
      set((s) => ({
        sessionsByStoryId: { ...s.sessionsByStoryId, [storyId]: bindings },
      }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  createStorySession: async (storyId, input) => {
    try {
      const raw = await api.post<Record<string, unknown>>(`/stories/${storyId}/sessions`, input);
      const binding = mapSessionBinding(raw);
      set((s) => {
        const existing = s.sessionsByStoryId[storyId] ?? [];
        return {
          sessionsByStoryId: {
            ...s.sessionsByStoryId,
            [storyId]: [...existing, binding],
          },
        };
      });
      return binding;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  unbindStorySession: async (storyId, bindingId) => {
    try {
      await api.delete(`/stories/${storyId}/sessions/${bindingId}`);
      set((s) => {
        const existing = s.sessionsByStoryId[storyId] ?? [];
        return {
          sessionsByStoryId: {
            ...s.sessionsByStoryId,
            [storyId]: existing.filter((b) => b.id !== bindingId),
          },
        };
      });
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  handleStateChange: (change: StateChange) => {
    const entityId = change.entity_id;
    const payload = (change.payload && typeof change.payload === 'object'
      ? change.payload
      : {}) as Record<string, unknown>;

    const refreshStoryById = (storyId: string) => {
      if (storyRefreshInFlight.has(storyId)) return;
      storyRefreshInFlight.add(storyId);
      api
        .get<Record<string, unknown>>(`/stories/${storyId}`)
        .then((raw) => {
          const story = mapStory(raw);
          set((s) => {
            const storiesByProjectId = upsertStoryInProjectMap(s.storiesByProjectId, story);
            return {
              storiesByProjectId,
            };
          });
        })
        .catch(() => {})
        .finally(() => {
          storyRefreshInFlight.delete(storyId);
        });
    };

    const refreshTaskById = (taskId: string) => {
      if (taskRefreshInFlight.has(taskId)) return;
      taskRefreshInFlight.add(taskId);
      api
        .get<Record<string, unknown>>(`/tasks/${taskId}`)
        .then((raw) => {
          const task = mapTask(raw);
          set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
        })
        .catch(() => {})
        .finally(() => {
          taskRefreshInFlight.delete(taskId);
        });
    };

    switch (change.kind) {
      case 'story_created':
      case 'story_updated':
      case 'story_status_changed': {
        if (canMapStoryFromPayload(payload)) {
          const story = mapStory(payload);
          set((s) => {
            const withoutOld = removeStoryFromProjectMap(s.storiesByProjectId, story.id);
            const storiesByProjectId = upsertStoryInProjectMap(withoutOld, story);
            return {
              storiesByProjectId,
            };
          });
          break;
        }
        refreshStoryById(entityId);
        break;
      }
      case 'story_deleted': {
        set((s) => {
          const payloadProjectId =
            typeof payload.project_id === 'string' ? payload.project_id : null;
          const deletedStory = findStoryById(s.storiesByProjectId, entityId);
          const storiesByProjectId = removeStoryFromProjectMap(
            s.storiesByProjectId,
            entityId,
            payloadProjectId ?? deletedStory?.project_id ?? null,
          );
          const nextTasks = { ...s.tasksByStoryId };
          delete nextTasks[entityId];
          const nextSessions = { ...s.sessionsByStoryId };
          delete nextSessions[entityId];
          return {
            storiesByProjectId,
            tasksByStoryId: nextTasks,
            sessionsByStoryId: nextSessions,
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
        if (canMapTaskFromPayload(payload)) {
          const task = mapTask(payload);
          set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
          break;
        }
        refreshTaskById(entityId);
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
