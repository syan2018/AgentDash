import type { AgentBinding, ProjectConfig, Workspace } from "../../types";

function normalizeText(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed ? trimmed : null;
}

export function createDefaultAgentBinding(_projectConfig?: ProjectConfig): AgentBinding {
  return {
    agent_type: null,
    agent_pid: null,
    preset_name: null,
    prompt_template: null,
    initial_context: null,
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

export function normalizeAgentBinding(binding: AgentBinding): AgentBinding {
  return {
    agent_type: normalizeText(binding.agent_type),
    agent_pid: normalizeText(binding.agent_pid),
    preset_name: normalizeText(binding.preset_name),
    prompt_template: normalizeText(binding.prompt_template),
    initial_context: normalizeText(binding.initial_context),
    context_sources: Array.isArray(binding.context_sources) ? binding.context_sources : [],
  };
}

export function hasAgentBindingSelection(
  binding: AgentBinding,
  _projectConfig?: ProjectConfig,
): boolean {
  const normalized = normalizeAgentBinding(binding);
  return Boolean(normalized.agent_type || normalized.preset_name);
}
