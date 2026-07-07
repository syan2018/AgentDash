import { settingsApi } from '../api/settings';
import type { JsonValue } from '../generated/common-contracts';

export const USER_WORKSPACE_STATE_SETTING_KEY = 'ui.workspace_state';

const USER_WORKSPACE_STATE_SCHEMA_VERSION = 1;
const MAX_RECENT_PROJECTS = 10;

export interface UserWorkspaceState {
  schema_version: 1;
  navigation: {
    current_project_id: string | null;
  };
  recent: {
    project_ids: string[];
  };
}

export interface WorkspaceProjectSelection {
  currentProjectId: string | null;
  workspaceState: UserWorkspaceState;
  shouldPersist: boolean;
}

interface ProjectRef {
  id: string;
}

interface JsonObject {
  [key: string]: JsonValue | undefined;
}

export function createEmptyUserWorkspaceState(): UserWorkspaceState {
  return {
    schema_version: USER_WORKSPACE_STATE_SCHEMA_VERSION,
    navigation: {
      current_project_id: null,
    },
    recent: {
      project_ids: [],
    },
  };
}

export async function loadUserWorkspaceState(): Promise<UserWorkspaceState> {
  const settings = await settingsApi.list({
    scope: 'user',
    category: USER_WORKSPACE_STATE_SETTING_KEY,
  });
  const setting = settings.find((entry) => entry.key === USER_WORKSPACE_STATE_SETTING_KEY);
  return parseUserWorkspaceState(setting?.value);
}

export async function saveUserWorkspaceState(state: UserWorkspaceState): Promise<void> {
  await settingsApi.update(
    { scope: 'user' },
    [{ key: USER_WORKSPACE_STATE_SETTING_KEY, value: userWorkspaceStateToJson(state) }],
  );
}

export function parseUserWorkspaceState(value: JsonValue | undefined): UserWorkspaceState {
  if (!isJsonObject(value)) {
    return createEmptyUserWorkspaceState();
  }

  const navigation = value.navigation;
  const currentProjectId = isJsonObject(navigation)
    ? readNonEmptyString(navigation.current_project_id)
    : null;

  const recent = value.recent;
  const recentProjectIds = isJsonObject(recent)
    ? readStringList(recent.project_ids)
    : [];

  return normalizeUserWorkspaceState(currentProjectId, recentProjectIds);
}

export function setCurrentProjectInUserWorkspaceState(
  state: UserWorkspaceState,
  projectId: string | null,
): UserWorkspaceState {
  const recentProjectIds = projectId
    ? [projectId, ...state.recent.project_ids.filter((id) => id !== projectId)]
    : state.recent.project_ids;
  return normalizeUserWorkspaceState(projectId, recentProjectIds);
}

export function resolveWorkspaceProjectSelection(
  projects: ProjectRef[],
  currentProjectId: string | null,
  state: UserWorkspaceState,
): WorkspaceProjectSelection {
  const availableProjectIds = new Set(projects.map((project) => project.id));
  const persistedProjectId = state.navigation.current_project_id;
  const selectedProjectId =
    persistedProjectId && availableProjectIds.has(persistedProjectId)
      ? persistedProjectId
      : currentProjectId && availableProjectIds.has(currentProjectId)
        ? currentProjectId
        : projects[0]?.id ?? null;

  const nextWorkspaceState = normalizeUserWorkspaceStateForProjects(
    setCurrentProjectInUserWorkspaceState(state, selectedProjectId),
    availableProjectIds,
  );

  return {
    currentProjectId: selectedProjectId,
    workspaceState: nextWorkspaceState,
    shouldPersist: !areUserWorkspaceStatesEqual(state, nextWorkspaceState),
  };
}

function normalizeUserWorkspaceState(
  currentProjectId: string | null,
  recentProjectIds: string[],
): UserWorkspaceState {
  const normalizedCurrentProjectId = readNonEmptyString(currentProjectId);
  const dedupedRecentProjectIds = uniqueStrings([
    ...(normalizedCurrentProjectId ? [normalizedCurrentProjectId] : []),
    ...recentProjectIds,
  ]).slice(0, MAX_RECENT_PROJECTS);

  return {
    schema_version: USER_WORKSPACE_STATE_SCHEMA_VERSION,
    navigation: {
      current_project_id: normalizedCurrentProjectId,
    },
    recent: {
      project_ids: dedupedRecentProjectIds,
    },
  };
}

function normalizeUserWorkspaceStateForProjects(
  state: UserWorkspaceState,
  availableProjectIds: Set<string>,
): UserWorkspaceState {
  const currentProjectId = state.navigation.current_project_id;
  const availableCurrentProjectId =
    currentProjectId && availableProjectIds.has(currentProjectId) ? currentProjectId : null;
  const availableRecentProjectIds = state.recent.project_ids.filter((id) => availableProjectIds.has(id));
  return normalizeUserWorkspaceState(availableCurrentProjectId, availableRecentProjectIds);
}

function userWorkspaceStateToJson(state: UserWorkspaceState): JsonValue {
  return {
    schema_version: state.schema_version,
    navigation: {
      current_project_id: state.navigation.current_project_id,
    },
    recent: {
      project_ids: state.recent.project_ids,
    },
  };
}

function isJsonObject(value: JsonValue | undefined): value is JsonObject {
  return value != null && typeof value === 'object' && !Array.isArray(value);
}

function readNonEmptyString(value: JsonValue | string | null | undefined): string | null {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function readStringList(value: JsonValue | undefined): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return uniqueStrings(value.filter((item): item is string => typeof item === 'string'));
}

function uniqueStrings(values: string[]): string[] {
  const result: string[] = [];
  for (const value of values) {
    const trimmed = value.trim();
    if (trimmed.length === 0 || result.includes(trimmed)) {
      continue;
    }
    result.push(trimmed);
  }
  return result;
}

function areUserWorkspaceStatesEqual(left: UserWorkspaceState, right: UserWorkspaceState): boolean {
  return left.navigation.current_project_id === right.navigation.current_project_id
    && left.recent.project_ids.length === right.recent.project_ids.length
    && left.recent.project_ids.every((id, index) => id === right.recent.project_ids[index]);
}
