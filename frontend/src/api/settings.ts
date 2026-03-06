import { api } from './client';

export interface SettingEntry {
  key: string;
  value: unknown;
  updated_at: string;
  masked: boolean;
}

export interface SettingUpdate {
  key: string;
  value: unknown;
}

export const settingsApi = {
  list: (category?: string) => {
    const params = category ? `?category=${encodeURIComponent(category)}` : '';
    return api.get<SettingEntry[]>(`/settings${params}`);
  },
  update: (settings: SettingUpdate[]) =>
    api.put<{ updated: string[] }>('/settings', { settings }),
  remove: (key: string) =>
    api.delete(`/settings/${encodeURIComponent(key)}`),
};
