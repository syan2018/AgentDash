import { api } from './client';
import type {
  SettingResponse,
  SettingUpdate,
  SettingsScopeKind,
  SettingsScopeQuery,
  UpdateSettingsResponse,
} from '../generated/settings-contracts';

export type SettingEntry = SettingResponse;
export type { SettingUpdate };

export type SettingsScopeRequest = SettingsScopeQuery & { scope: SettingsScopeKind };

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
    api.put<UpdateSettingsResponse>(`/settings${buildScopeParams(scope)}`, { settings }),
  remove: (scope: SettingsScopeRequest, key: string) =>
    api.delete(`/settings/${encodeURIComponent(key)}${buildScopeParams(scope)}`),
};
