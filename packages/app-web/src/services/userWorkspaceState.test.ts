import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { SettingEntry } from '../api/settings';
import type { JsonValue } from '../generated/common-contracts';
import {
  USER_WORKSPACE_STATE_SETTING_KEY,
  createEmptyUserWorkspaceState,
  loadUserWorkspaceState,
  resolveWorkspaceProjectSelection,
  saveUserWorkspaceState,
  setCurrentProjectInUserWorkspaceState,
} from './userWorkspaceState';

const mocks = vi.hoisted(() => ({
  settingsList: vi.fn(),
  settingsUpdate: vi.fn(),
}));

vi.mock('../api/settings', () => ({
  settingsApi: {
    list: mocks.settingsList,
    update: mocks.settingsUpdate,
  },
}));

describe('userWorkspaceState service', () => {
  beforeEach(() => {
    mocks.settingsList.mockReset();
    mocks.settingsUpdate.mockReset();
  });

  it('loads structured workspace state from user settings', async () => {
    mocks.settingsList.mockResolvedValue([
      setting({
        schema_version: 1,
        navigation: { current_project_id: 'project-2' },
        recent: { project_ids: ['project-2', 'project-1'] },
      }),
    ]);

    await expect(loadUserWorkspaceState()).resolves.toEqual({
      schema_version: 1,
      navigation: { current_project_id: 'project-2' },
      recent: { project_ids: ['project-2', 'project-1'] },
    });
    expect(mocks.settingsList).toHaveBeenCalledWith({
      scope: 'user',
      category: USER_WORKSPACE_STATE_SETTING_KEY,
    });
  });

  it('normalizes malformed settings to an empty workspace state', async () => {
    mocks.settingsList.mockResolvedValue([setting('not-an-object')]);

    await expect(loadUserWorkspaceState()).resolves.toEqual(createEmptyUserWorkspaceState());
  });

  it('selects persisted project before list order', () => {
    const state = setCurrentProjectInUserWorkspaceState(createEmptyUserWorkspaceState(), 'project-1');
    const selection = resolveWorkspaceProjectSelection(
      [{ id: 'project-2' }, { id: 'project-1' }],
      null,
      state,
    );

    expect(selection.currentProjectId).toBe('project-1');
    expect(selection.shouldPersist).toBe(false);
  });

  it('falls back to the first available project and marks state for persistence', () => {
    const state = setCurrentProjectInUserWorkspaceState(createEmptyUserWorkspaceState(), 'missing-project');
    const selection = resolveWorkspaceProjectSelection(
      [{ id: 'project-2' }, { id: 'project-1' }],
      null,
      state,
    );

    expect(selection.currentProjectId).toBe('project-2');
    expect(selection.workspaceState.navigation.current_project_id).toBe('project-2');
    expect(selection.workspaceState.recent.project_ids).toEqual(['project-2']);
    expect(selection.shouldPersist).toBe(true);
  });

  it('saves current workspace state as structured user setting', async () => {
    mocks.settingsUpdate.mockResolvedValue({ updated: [USER_WORKSPACE_STATE_SETTING_KEY] });
    const state = setCurrentProjectInUserWorkspaceState(createEmptyUserWorkspaceState(), 'project-1');

    await saveUserWorkspaceState(state);

    expect(mocks.settingsUpdate).toHaveBeenCalledWith(
      { scope: 'user' },
      [{
        key: USER_WORKSPACE_STATE_SETTING_KEY,
        value: {
          schema_version: 1,
          navigation: { current_project_id: 'project-1' },
          recent: { project_ids: ['project-1'] },
        },
      }],
    );
  });
});

function setting(value: JsonValue): SettingEntry {
  return {
    scope_kind: 'user',
    key: USER_WORKSPACE_STATE_SETTING_KEY,
    value,
    updated_at: new Date(0).toISOString(),
    masked: false,
  };
}
