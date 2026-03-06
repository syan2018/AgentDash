import { create } from 'zustand';
import { settingsApi, type SettingEntry, type SettingUpdate } from '../api/settings';

interface SettingsState {
  settings: SettingEntry[];
  loading: boolean;
  error: string | null;
  saving: boolean;

  fetchSettings: (category?: string) => Promise<void>;
  updateSettings: (updates: SettingUpdate[]) => Promise<string[]>;
  deleteSetting: (key: string) => Promise<void>;
  getSetting: (key: string) => SettingEntry | undefined;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: [],
  loading: false,
  error: null,
  saving: false,

  fetchSettings: async (category) => {
    set({ loading: true, error: null });
    try {
      const settings = await settingsApi.list(category);
      set({ settings, loading: false });
    } catch (e) {
      set({ error: (e as Error).message, loading: false });
    }
  },

  updateSettings: async (updates) => {
    set({ saving: true, error: null });
    try {
      const result = await settingsApi.update(updates);
      await get().fetchSettings();
      set({ saving: false });
      return result.updated;
    } catch (e) {
      set({ error: (e as Error).message, saving: false });
      return [];
    }
  },

  deleteSetting: async (key) => {
    try {
      await settingsApi.remove(key);
      set((s) => ({ settings: s.settings.filter((e) => e.key !== key) }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  getSetting: (key) => get().settings.find((s) => s.key === key),
}));
