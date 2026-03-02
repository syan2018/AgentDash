import type { AgentBinding, ProjectConfig } from "../../types";

function normalizeText(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed ? trimmed : null;
}

export function createDefaultAgentBinding(projectConfig?: ProjectConfig): AgentBinding {
  return {
    agent_type: normalizeText(projectConfig?.default_agent_type),
    agent_pid: null,
    preset_name: null,
    prompt_template: null,
    initial_context: null,
  };
}

export function normalizeAgentBinding(binding: AgentBinding): AgentBinding {
  return {
    agent_type: normalizeText(binding.agent_type),
    agent_pid: normalizeText(binding.agent_pid),
    preset_name: normalizeText(binding.preset_name),
    prompt_template: normalizeText(binding.prompt_template),
    initial_context: normalizeText(binding.initial_context),
  };
}

export function hasAgentBindingSelection(
  binding: AgentBinding,
  projectConfig?: ProjectConfig,
): boolean {
  const normalized = normalizeAgentBinding(binding);
  return Boolean(normalized.agent_type || normalized.preset_name || normalizeText(projectConfig?.default_agent_type));
}
