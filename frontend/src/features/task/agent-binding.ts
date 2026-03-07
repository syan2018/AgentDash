import type { AgentBinding, ProjectConfig, Workspace } from "../../types";

function normalizeText(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed ? trimmed : null;
}

export function createDefaultAgentBinding(projectConfig?: ProjectConfig): AgentBinding {
  const defaultAgentType = normalizeText(projectConfig?.default_agent_type);
  if (defaultAgentType) {
    return {
      agent_type: defaultAgentType,
      agent_pid: null,
      preset_name: null,
      prompt_template: null,
      initial_context: null,
      context_sources: [],
    };
  }

  const fallbackPreset = projectConfig?.agent_presets?.[0];
  if (fallbackPreset) {
    return {
      agent_type: normalizeText(fallbackPreset.agent_type),
      agent_pid: null,
      preset_name: normalizeText(fallbackPreset.name),
      prompt_template: null,
      initial_context: null,
      context_sources: [],
    };
  }

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
  return workspaces[0]?.id ?? "";
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
  projectConfig?: ProjectConfig,
): boolean {
  const normalized = normalizeAgentBinding(binding);
  return Boolean(normalized.agent_type || normalized.preset_name || normalizeText(projectConfig?.default_agent_type));
}
