import { create } from 'zustand';
import type { BackendConfig, ViewConfig } from '../types';
import { api } from '../api/client';

interface CoordinatorState {
  backends: BackendConfig[];
  views: ViewConfig[];
  isLoading: boolean;
  error: string | null;

  fetchBackends: () => Promise<void>;
  addBackend: (config: Omit<BackendConfig, 'enabled'>) => Promise<void>;
  removeBackend: (id: string) => Promise<void>;
}

export const useCoordinatorStore = create<CoordinatorState>((set) => ({
  backends: [],
  views: [],
  isLoading: false,
  error: null,

  fetchBackends: async () => {
    set({ isLoading: true, error: null });
    try {
      const backends = await api.get<BackendConfig[]>('/backends');
      set({ backends, isLoading: false });
    } catch (e) {
      set({ error: (e as Error).message, isLoading: false });
    }
  },

  addBackend: async (config) => {
    try {
      const backend = await api.post<BackendConfig>('/backends', config);
      set((s) => ({ backends: [...s.backends, backend] }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  removeBackend: async (id) => {
    try {
      await api.delete(`/backends/${id}`);
      set((s) => ({
        backends: s.backends.filter((b) => b.id !== id),
      }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },
}));
