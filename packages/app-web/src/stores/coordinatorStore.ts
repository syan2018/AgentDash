import { create } from 'zustand';
import type { BackendConfig, BackendRuntimeSummary, ViewConfig } from '../types';
import { api } from '../api/client';

interface CoordinatorState {
  backends: BackendConfig[];
  backendRuntimeSummaries: BackendRuntimeSummary[];
  views: ViewConfig[];
  isLoading: boolean;
  error: string | null;

  fetchBackends: () => Promise<void>;
  fetchBackendRuntimeSummaries: () => Promise<void>;
  addBackend: (config: Omit<BackendConfig, 'enabled'>) => Promise<void>;
  removeBackend: (id: string) => Promise<void>;
}

export const useCoordinatorStore = create<CoordinatorState>((set) => ({
  backends: [],
  backendRuntimeSummaries: [],
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

  fetchBackendRuntimeSummaries: async () => {
    try {
      const backendRuntimeSummaries = await api.get<BackendRuntimeSummary[]>('/backends/runtime-summary');
      set({ backendRuntimeSummaries });
    } catch (e) {
      set({ error: (e as Error).message });
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
