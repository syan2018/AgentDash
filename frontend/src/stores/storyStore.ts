import { create } from 'zustand';
import type { Story, Task } from '../types';
import { api } from '../api/client';

interface StoryState {
  stories: Story[];
  currentStoryId: string | null;
  tasks: Record<string, Task[]>;
  isLoading: boolean;
  error: string | null;

  fetchStories: (backendId: string) => Promise<void>;
  createStory: (backendId: string, title: string, description?: string) => Promise<void>;
  selectStory: (id: string | null) => void;
  fetchTasks: (storyId: string) => Promise<void>;
}

export const useStoryStore = create<StoryState>((set) => ({
  stories: [],
  currentStoryId: null,
  tasks: {},
  isLoading: false,
  error: null,

  fetchStories: async (backendId) => {
    set({ isLoading: true, error: null });
    try {
      const stories = await api.get<Story[]>(`/stories?backend_id=${backendId}`);
      set({ stories, isLoading: false });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  createStory: async (backendId, title, description) => {
    try {
      const story = await api.post<Story>('/stories', {
        backend_id: backendId,
        title,
        description,
      });
      set((s) => ({ stories: [story, ...s.stories] }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  selectStory: (id) => set({ currentStoryId: id }),

  fetchTasks: async (storyId) => {
    try {
      const tasks = await api.get<Task[]>(`/stories/${storyId}/tasks`);
      set((s) => ({ tasks: { ...s.tasks, [storyId]: tasks } }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },
}));
