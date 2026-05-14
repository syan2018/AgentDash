import { create } from 'zustand';
import {
  llmProvidersApi,
  type LlmProvider,
  type CreateLlmProviderRequest,
  type UpdateLlmProviderRequest,
} from '../api/llmProviders';

interface LlmProviderState {
  providers: LlmProvider[];
  loading: boolean;
  error: string | null;
  saving: boolean;

  fetchProviders: () => Promise<void>;
  createProvider: (req: CreateLlmProviderRequest) => Promise<LlmProvider | null>;
  updateProvider: (id: string, req: UpdateLlmProviderRequest) => Promise<LlmProvider | null>;
  deleteProvider: (id: string) => Promise<void>;
  reorderProviders: (ids: string[]) => Promise<void>;
}

export const useLlmProviderStore = create<LlmProviderState>((set, get) => ({
  providers: [],
  loading: false,
  error: null,
  saving: false,

  fetchProviders: async () => {
    set({ loading: true, error: null });
    try {
      const providers = await llmProvidersApi.list();
      set({ providers, loading: false });
    } catch (e) {
      set({ error: (e as Error).message, loading: false });
    }
  },

  createProvider: async (req) => {
    set({ saving: true, error: null });
    try {
      const provider = await llmProvidersApi.create(req);
      await get().fetchProviders();
      set({ saving: false });
      return provider;
    } catch (e) {
      set({ error: (e as Error).message, saving: false });
      return null;
    }
  },

  updateProvider: async (id, req) => {
    set({ saving: true, error: null });
    try {
      const provider = await llmProvidersApi.update(id, req);
      await get().fetchProviders();
      set({ saving: false });
      return provider;
    } catch (e) {
      set({ error: (e as Error).message, saving: false });
      return null;
    }
  },

  deleteProvider: async (id) => {
    try {
      await llmProvidersApi.delete(id);
      set((s) => ({ providers: s.providers.filter((p) => p.id !== id) }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  reorderProviders: async (ids) => {
    set({ saving: true, error: null });
    try {
      await llmProvidersApi.reorder(ids);
      await get().fetchProviders();
      set({ saving: false });
    } catch (e) {
      set({ error: (e as Error).message, saving: false });
    }
  },
}));
