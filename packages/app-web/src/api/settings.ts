import { api } from './client';

export interface SettingEntry {
  scope_kind: "system" | "user" | "project";
  scope_id?: string | null;
  key: string;
  value: unknown;
  updated_at: string;
  masked: boolean;
}

export interface SettingUpdate {
  key: string;
  value: unknown;
}

export interface SettingsScopeRequest {
  scope: "system" | "user" | "project";
  project_id?: string;
  category?: string;
}

function buildScopeParams(scope: SettingsScopeRequest): string {
  const params = new URLSearchParams();
  params.set("scope", scope.scope);
  if (scope.project_id) {
    params.set("project_id", scope.project_id);
  }
  if (scope.category) {
    params.set("category", scope.category);
  }
  const query = params.toString();
  return query ? `?${query}` : "";
}

export const settingsApi = {
  list: (scope: SettingsScopeRequest) => {
    return api.get<SettingEntry[]>(`/settings${buildScopeParams(scope)}`);
  },
  update: (scope: SettingsScopeRequest, settings: SettingUpdate[]) =>
    api.put<{ updated: string[] }>(`/settings${buildScopeParams(scope)}`, { settings }),
  remove: (scope: SettingsScopeRequest, key: string) =>
    api.delete(`/settings/${encodeURIComponent(key)}${buildScopeParams(scope)}`),
};
