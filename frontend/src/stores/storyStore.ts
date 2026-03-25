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
  MountDerivationPolicy,
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
  stories: Story[];
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
      mount_policy_override?: MountDerivationPolicy | null;
      clear_mount_policy_override?: boolean;
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
      return "cancelled";
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

const defaultContext: StoryContext = {
  prd_doc: null,
  spec_refs: [],
  resource_list: [],
  source_refs: [],
  context_containers: [],
  disabled_container_ids: [],
  mount_policy_override: null,
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
      return 'p2';
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
      return 'other';
  }
};

const mapStory = (raw: Record<string, unknown>): Story => {
  let context: StoryContext = defaultContext;
  if (raw.context && typeof raw.context === 'object') {
    const ctx = raw.context as Record<string, unknown>;
    if ('spec_refs' in ctx || 'prd_doc' in ctx || 'resource_list' in ctx) {
      context = {
        prd_doc: (ctx.prd_doc as string) ?? null,
        spec_refs: Array.isArray(ctx.spec_refs) ? ctx.spec_refs as string[] : [],
        resource_list: Array.isArray(ctx.resource_list) ? ctx.resource_list as StoryContext['resource_list'] : [],
        source_refs: Array.isArray(ctx.source_refs) ? ctx.source_refs as ContextSourceRef[] : [],
        context_containers: Array.isArray(ctx.context_containers)
          ? ctx.context_containers as ContextContainerDefinition[]
          : [],
        disabled_container_ids: Array.isArray(ctx.disabled_container_ids)
          ? ctx.disabled_container_ids as string[]
          : [],
        mount_policy_override: (ctx.mount_policy_override as MountDerivationPolicy) ?? null,
        session_composition: (ctx.session_composition as SessionComposition) ?? null,
      };
    }
  }

  return {
    id: String(raw.id ?? ''),
    project_id: String(raw.project_id ?? ''),
    default_workspace_id: raw.default_workspace_id != null ? String(raw.default_workspace_id) : null,
    title: String(raw.title ?? '未命名 Story'),
    description: raw.description ? String(raw.description) : '',
    status: normalizeStoryStatus(String(raw.status ?? 'draft')),
    priority: normalizeStoryPriority(String(raw.priority ?? 'p2')),
    story_type: normalizeStoryType(String(raw.story_type ?? 'feature')),
    tags: Array.isArray(raw.tags) ? raw.tags.filter((t): t is string => typeof t === 'string') : [],
    task_count: Number.isFinite(Number(raw.task_count ?? 0)) ? Number(raw.task_count ?? 0) : 0,
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
  thinking_level: null,
  context_sources: [],
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
    thinking_level: isThinkingLevel(binding.thinking_level) ? binding.thinking_level : null,
    context_sources: Array.isArray(binding.context_sources)
      ? binding.context_sources as ContextSourceRef[]
      : [],
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

const flattenStoriesMap = (storiesByProjectId: Record<string, Story[]>): Story[] => {
  const merged = Object.values(storiesByProjectId).flat();
  const dedup = new Map<string, Story>();
  for (const story of merged) {
    if (!dedup.has(story.id)) {
      dedup.set(story.id, story);
    }
  }
  return Array.from(dedup.values());
};

const mapSessionBinding = (raw: Record<string, unknown>): SessionBinding => ({
  id: String(raw.id ?? ''),
  project_id: String(raw.project_id ?? ''),
  session_id: String(raw.session_id ?? ''),
  owner_type: (raw.owner_type ?? 'story') as SessionBinding['owner_type'],
  owner_id: String(raw.owner_id ?? ''),
  label: String(raw.label ?? ''),
  created_at: String(raw.created_at ?? new Date().toISOString()),
  session_title: raw.session_title != null
    ? String(raw.session_title)
    : undefined,
  session_updated_at: raw.session_updated_at != null
    ? Number(raw.session_updated_at)
    : undefined,
});

const storyRefreshInFlight = new Set<string>();
const taskRefreshInFlight = new Set<string>();

const mapTask = (raw: Record<string, unknown>): Task => {
  return {
    id: String(raw.id ?? ''),
    project_id: String(raw.project_id ?? ''),
    story_id: String(raw.story_id ?? ''),
    workspace_id: raw.workspace_id ? String(raw.workspace_id) : null,
    session_id: raw.session_id ? String(raw.session_id) : null,
    executor_session_id: raw.executor_session_id ? String(raw.executor_session_id) : null,
    title: String(raw.title ?? raw.name ?? '未命名 Task'),
    description: raw.description ? String(raw.description) : '',
    status: normalizeTaskStatus(String(raw.status ?? 'pending')),
    execution_mode:
      raw.execution_mode === 'auto_retry' || raw.execution_mode === 'one_shot'
        ? raw.execution_mode
        : 'standard',
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
  stories: [],
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
          stories: flattenStoriesMap(storiesByProjectId),
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
          stories: flattenStoriesMap(storiesByProjectId),
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
          stories: flattenStoriesMap(storiesByProjectId),
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
          stories: flattenStoriesMap(storiesByProjectId),
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
        const deletedStory = s.stories.find((story) => story.id === storyId) ?? null;
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
          stories: flattenStoriesMap(storiesByProjectId),
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
        workspace_id: raw.workspace_id ? String(raw.workspace_id) : null,
        session_id: raw.session_id ? String(raw.session_id) : null,
        executor_session_id: raw.executor_session_id ? String(raw.executor_session_id) : null,
        task_status: normalizeTaskStatus(String(raw.task_status ?? 'pending')),
        agent_binding: mapAgentBinding(raw.agent_binding),
        session_title: raw.session_title ? String(raw.session_title) : null,
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
        binding_id: String(raw.binding_id ?? bindingId),
        session_id: String(raw.session_id ?? ''),
        session_title: raw.session_title ? String(raw.session_title) : null,
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
              stories: flattenStoriesMap(storiesByProjectId),
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
              stories: flattenStoriesMap(storiesByProjectId),
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
          const deletedStory = s.stories.find((story) => story.id === entityId) ?? null;
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
            stories: flattenStoriesMap(storiesByProjectId),
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
