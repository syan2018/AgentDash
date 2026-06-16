import { create } from "zustand";
import type {
  ContextContainerDefinition,
  ContextSourceRef,
  SessionComposition,
  StateChange,
  Story,
  StoryTaskProjectionItem,
} from "../types";
import * as storyService from "../services/story";
import {
  canMapStoryFromPayload,
  mapStoryFromPayload,
} from "../services/story";

interface StoryState {
  storiesByProjectId: Record<string, Story[]>;
  storyTaskProjectionByStoryId: Record<string, StoryTaskProjectionItem[]>;
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
  fetchStoryTaskProjection: (storyId: string) => Promise<void>;
  selectStory: (id: string | null) => void;
  selectTask: (id: string | null) => void;
  handleStateChange: (change: StateChange) => void;
}

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

export const useStoryStore = create<StoryState>((set) => ({
  storiesByProjectId: {},
  storyTaskProjectionByStoryId: {},
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
          const nextProjection = { ...s.storyTaskProjectionByStoryId };
          delete nextProjection[id];
          return {
            storiesByProjectId,
            storyTaskProjectionByStoryId: nextProjection,
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
        const nextProjection = { ...s.storyTaskProjectionByStoryId };
        delete nextProjection[storyId];
        return {
          storiesByProjectId,
          storyTaskProjectionByStoryId: nextProjection,
          selectedStoryId: s.selectedStoryId === storyId ? null : s.selectedStoryId,
        };
      });
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  fetchStoryTaskProjection: async (storyId) => {
    try {
      const projection = await storyService.fetchStoryTaskProjection(storyId);
      set((s) => ({
        storyTaskProjectionByStoryId: {
          ...s.storyTaskProjectionByStoryId,
          [storyId]: projection.tasks,
        },
      }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  selectStory: (id) => set({ selectedStoryId: id }),
  selectTask: (id) => set({ selectedTaskId: id }),

  handleStateChange: (change) => {
    const entityId = change.entity_id;
    const payload = (change.payload && typeof change.payload === "object"
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

    switch (change.kind) {
      case "story_created":
      case "story_updated":
      case "story_status_changed": {
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
      case "story_deleted": {
        set((s) => {
          const payloadProjectId =
            typeof payload.project_id === "string" ? payload.project_id : null;
          const deletedStory = findStoryById(s.storiesByProjectId, entityId);
          const storiesByProjectId = removeStoryFromProjectMap(
            s.storiesByProjectId,
            entityId,
            payloadProjectId ?? deletedStory?.project_id ?? null,
          );
          const nextProjection = { ...s.storyTaskProjectionByStoryId };
          delete nextProjection[entityId];
          return {
            storiesByProjectId,
            storyTaskProjectionByStoryId: nextProjection,
            selectedStoryId:
              s.selectedStoryId === entityId ? null : s.selectedStoryId,
          };
        });
        break;
      }
    }
  },
}));
