import { create } from 'zustand';
import { settingsApi, type SettingEntry, type SettingUpdate, type SettingsScopeRequest } from '../api/settings';

interface SettingsState {
  settings: SettingEntry[];
  loading: boolean;
  error: string | null;
  saving: boolean;

  fetchSettings: (scope: SettingsScopeRequest) => Promise<void>;
  updateSettings: (scope: SettingsScopeRequest, updates: SettingUpdate[]) => Promise<string[]>;
  deleteSetting: (scope: SettingsScopeRequest, key: string) => Promise<void>;
  getSetting: (key: string) => SettingEntry | undefined;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: [],
  loading: false,
  error: null,
  saving: false,

  fetchSettings: async (scope) => {
    set({ loading: true, error: null });
    try {
      const settings = await settingsApi.list(scope);
      set({ settings, loading: false });
    } catch (e) {
      set({ error: (e as Error).message, loading: false });
    }
  },

  updateSettings: async (scope, updates) => {
    set({ saving: true, error: null });
    try {
      const result = await settingsApi.update(scope, updates);
      await get().fetchSettings(scope);
      set({ saving: false });
      return result.updated;
    } catch (e) {
      set({ error: (e as Error).message, saving: false });
      return [];
    }
  },

  deleteSetting: async (scope, key) => {
    try {
      await settingsApi.remove(scope, key);
      set((s) => ({ settings: s.settings.filter((e) => e.key !== key) }));
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  getSetting: (key) => get().settings.find((s) => s.key === key),
}));
