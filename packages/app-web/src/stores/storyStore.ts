import { create } from 'zustand';
import type {
  Story,
  Task,
  AgentBinding,
  StateChange,
  SessionBinding,
  StoryRunOverviewDto,
  ContextSourceRef,
  ContextContainerDefinition,
  SessionComposition,
} from '../types';
import * as storyService from '../services/story';
import {
  canMapStoryFromPayload,
  canMapTaskFromPayload,
  mapStoryFromPayload,
  mapTaskFromPayload,
  type CreateStorySessionInput,
  type TaskSessionPayload,
} from '../services/story';
import type { StorySessionInfo } from '../types';

export type { CreateStorySessionInput } from '../services/story';

export interface CreateTaskInput {
  title: string;
  description?: string;
  workspace_id?: string | null;
  lifecycle_step_key?: string | null;
  agent_binding?: AgentBinding;
}

/** @deprecated 使用 services/story 的 TaskSessionPayload */
export type TaskSessionInfo = TaskSessionPayload;

interface StoryState {
  storiesByProjectId: Record<string, Story[]>;
  tasksByStoryId: Record<string, Task[]>;
  runsByStoryId: Record<string, StoryRunOverviewDto[]>;
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
      status?: Story["status"];
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
  batchUpdateStories: (
    storyIds: string[],
    patch: {
      status?: Story["status"];
      priority?: Story["priority"];
      story_type?: Story["story_type"];
    },
  ) => Promise<{ updated: number; failed: number }>;
  batchDeleteStories: (storyIds: string[]) => Promise<{ deleted: number; failed: number }>;
  deleteStory: (storyId: string) => Promise<void>;
  createTask: (storyId: string, payload: CreateTaskInput) => Promise<Task | null>;
  updateTask: (
    taskId: string,
    payload: {
      title?: string;
      description?: string;
      workspace_id?: string | null;
      lifecycle_step_key?: string | null;
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
  fetchTaskSession: (taskId: string) => Promise<TaskSessionPayload | null>;
  fetchStorySessionInfo: (storyId: string, bindingId: string) => Promise<StorySessionInfo | null>;
  deleteTask: (taskId: string, storyId: string) => Promise<void>;
  selectStory: (id: string | null) => void;
  selectTask: (id: string | null) => void;
  fetchTasks: (storyId: string) => Promise<void>;
  fetchStoryRuns: (storyId: string) => Promise<void>;
  fetchStorySessions: (storyId: string) => Promise<void>;
  createStorySession: (storyId: string, input: CreateStorySessionInput) => Promise<SessionBinding | null>;
  unbindStorySession: (storyId: string, bindingId: string) => Promise<void>;
  handleStateChange: (change: StateChange) => void;
}

// ─── 客户端缓存 collection helpers ──────────────────────

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

const storyRefreshInFlight = new Set<string>();
const taskRefreshInFlight = new Set<string>();

export const useStoryStore = create<StoryState>((set) => ({
  storiesByProjectId: {},
  tasksByStoryId: {},
  runsByStoryId: {},
  sessionsByStoryId: {},
  selectedStoryId: null,
  selectedTaskId: null,
  isLoading: false,
  error: null,

  fetchStoriesByProject: async (projectId) => {
    set({ isLoading: true, error: null });
    try {
      const stories = await storyService.fetchStoriesByProject(projectId);
      set((s) => ({
        storiesByProjectId: { ...s.storiesByProjectId, [projectId]: stories },
        isLoading: false,
      }));
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  fetchStoryById: async (storyId) => {
    try {
      const story = await storyService.fetchStoryById(storyId);
      set((s) => ({ storiesByProjectId: upsertStoryInProjectMap(s.storiesByProjectId, story) }));
      return story;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  createStory: async (projectId, title, description, options) => {
    try {
      const story = await storyService.createStory(projectId, title, description, options);
      set((s) => ({ storiesByProjectId: upsertStoryInProjectMap(s.storiesByProjectId, story) }));
      return story;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  updateStory: async (storyId, payload) => {
    try {
      const story = await storyService.updateStory(storyId, payload);
      set((s) => {
        const withoutOld = removeStoryFromProjectMap(s.storiesByProjectId, storyId);
        return { storiesByProjectId: upsertStoryInProjectMap(withoutOld, story) };
      });
      return story;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  batchUpdateStories: async (storyIds, patch) => {
    let updated = 0;
    let failed = 0;
    const request = storyService.buildBatchStoryRequest(patch);
    for (const id of storyIds) {
      try {
        const story = await storyService.patchStory(id, request);
        set((s) => {
          const withoutOld = removeStoryFromProjectMap(s.storiesByProjectId, id);
          return { storiesByProjectId: upsertStoryInProjectMap(withoutOld, story) };
        });
        updated += 1;
      } catch (e) {
        failed += 1;
        set({ error: (e as Error).message });
      }
    }
    return { updated, failed };
  },

  batchDeleteStories: async (storyIds) => {
    let deleted = 0;
    let failed = 0;
    for (const id of storyIds) {
      try {
        await storyService.deleteStory(id);
        set((s) => {
          const story = findStoryById(s.storiesByProjectId, id);
          const storiesByProjectId = removeStoryFromProjectMap(
            s.storiesByProjectId,
            id,
            story?.project_id ?? null,
          );
          const nextTasks = { ...s.tasksByStoryId };
          delete nextTasks[id];
          const nextSessions = { ...s.sessionsByStoryId };
          delete nextSessions[id];
          return {
            storiesByProjectId,
            tasksByStoryId: nextTasks,
            sessionsByStoryId: nextSessions,
            selectedStoryId: s.selectedStoryId === id ? null : s.selectedStoryId,
          };
        });
        deleted += 1;
      } catch (e) {
        failed += 1;
        set({ error: (e as Error).message });
      }
    }
    return { deleted, failed };
  },

  deleteStory: async (storyId) => {
    try {
      await storyService.deleteStory(storyId);
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
      const task = await storyService.createTask(storyId, payload);
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
      const task = await storyService.updateTask(taskId, payload);
      set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  startTaskExecution: async (taskId, payload) => {
    try {
      const task = await storyService.startTaskExecution(taskId, payload);
      set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  continueTaskExecution: async (taskId, payload) => {
    try {
      const task = await storyService.continueTaskExecution(taskId, payload);
      set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  cancelTaskExecution: async (taskId) => {
    try {
      const task = await storyService.cancelTaskExecution(taskId);
      set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  refreshTask: async (taskId) => {
    try {
      const task = await storyService.fetchTask(taskId);
      set((s) => ({ tasksByStoryId: upsertTaskInMap(s.tasksByStoryId, task) }));
      return task;
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  fetchTaskSession: async (taskId) => {
    try {
      return await storyService.fetchTaskSession(taskId);
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  fetchStorySessionInfo: async (storyId, bindingId) => {
    try {
      return await storyService.fetchStorySessionInfo(storyId, bindingId);
    } catch (e) {
      set({ error: (e as Error).message });
      return null;
    }
  },

  deleteTask: async (taskId, storyId) => {
    try {
      await storyService.deleteTask(taskId);
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
      const tasks = await storyService.fetchTasks(storyId);
      set((s) => ({
        tasksByStoryId: { ...s.tasksByStoryId, [storyId]: tasks },
      }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  fetchStoryRuns: async (storyId) => {
    try {
      const runs = await storyService.fetchStoryRuns(storyId);
      set((s) => ({
        runsByStoryId: { ...s.runsByStoryId, [storyId]: runs },
      }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  fetchStorySessions: async (storyId) => {
    try {
      const bindings = await storyService.fetchStorySessions(storyId);
      set((s) => ({
        sessionsByStoryId: { ...s.sessionsByStoryId, [storyId]: bindings },
      }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  createStorySession: async (storyId, input) => {
    try {
      const binding = await storyService.createStorySession(storyId, input);
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
      await storyService.unbindStorySession(storyId, bindingId);
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
      storyService
        .fetchStoryById(storyId)
        .then((story) => {
          set((s) => ({ storiesByProjectId: upsertStoryInProjectMap(s.storiesByProjectId, story) }));
        })
        .catch(() => {})
        .finally(() => {
          storyRefreshInFlight.delete(storyId);
        });
    };

    const refreshTaskById = (taskId: string) => {
      if (taskRefreshInFlight.has(taskId)) return;
      taskRefreshInFlight.add(taskId);
      storyService
        .fetchTask(taskId)
        .then((task) => {
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
          const story = mapStoryFromPayload(payload);
          set((s) => {
            const withoutOld = removeStoryFromProjectMap(s.storiesByProjectId, story.id);
            return { storiesByProjectId: upsertStoryInProjectMap(withoutOld, story) };
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
          const task = mapTaskFromPayload(payload);
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
