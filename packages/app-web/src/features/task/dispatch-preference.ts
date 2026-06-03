import type { TaskDispatchPreference, ProjectConfig, Workspace } from "../../types";

function normalizeText(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed ? trimmed : null;
}

export function createDefaultDispatchPreference(_projectConfig?: ProjectConfig): TaskDispatchPreference {
  return {
    agent_type: null,
    agent_pid: null,
    preset_name: null,
    prompt_template: null,
    initial_context: null,
    thinking_level: null,
    context_sources: [],
  };
}

export function resolveDefaultWorkspaceId(
  projectConfig: ProjectConfig | undefined,
  workspaces: Workspace[],
): string {
  const projectDefault = projectConfig?.default_workspace_id?.trim() ?? "";
  if (projectDefault && workspaces.some((item) => item.id === projectDefault)) {
    return projectDefault;
  }
  return "";
}

export function normalizeDispatchPreference(pref: TaskDispatchPreference): TaskDispatchPreference {
  return {
    agent_type: normalizeText(pref.agent_type),
    agent_pid: normalizeText(pref.agent_pid),
    preset_name: normalizeText(pref.preset_name),
    prompt_template: normalizeText(pref.prompt_template),
    initial_context: normalizeText(pref.initial_context),
    thinking_level: pref.thinking_level ?? null,
    context_sources: Array.isArray(pref.context_sources) ? pref.context_sources : [],
  };
}

export function hasDispatchPreferenceSelection(
  pref: TaskDispatchPreference,
  _projectConfig?: ProjectConfig,
): boolean {
  const normalized = normalizeDispatchPreference(pref);
  return Boolean(normalized.agent_type || normalized.preset_name);
}
