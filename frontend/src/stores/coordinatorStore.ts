import { create } from 'zustand';
import type { BackendConfig, ViewConfig } from '../types';
import { api } from '../api/client';

interface CoordinatorState {
  backends: BackendConfig[];
  views: ViewConfig[];
  currentBackendId: string | null;
  isLoading: boolean;
  error: string | null;

  fetchBackends: () => Promise<void>;
  addBackend: (config: Omit<BackendConfig, 'enabled'>) => Promise<void>;
  removeBackend: (id: string) => Promise<void>;
  selectBackend: (id: string | null) => void;
}

export const useCoordinatorStore = create<CoordinatorState>((set, get) => ({
  backends: [],
  views: [],
  currentBackendId: null,
  isLoading: false,
  error: null,

  fetchBackends: async () => {
    set({ isLoading: true, error: null });
    try {
      const backends = await api.get<BackendConfig[]>('/backends');
      set({ backends, isLoading: false });
      if (!get().currentBackendId && backends.length > 0) {
        set({ currentBackendId: backends[0].id });
      }
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
        currentBackendId: s.currentBackendId === id ? null : s.currentBackendId,
      }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  selectBackend: (id) => set({ currentBackendId: id }),
}));
