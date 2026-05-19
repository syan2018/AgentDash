import type {
  AgentPreset,
  CapabilityDirective,
  CapabilityKey,
  SystemPromptMode,
  ThinkingLevel,
} from "../../../types";
import { isThinkingLevel } from "../../../types";

export interface PresetFormState {
  name: string;
  display_name: string;
  description: string;
  agent_type: string;
  provider_id: string;
  model_id: string;
  agent_id: string;
  thinking_level: ThinkingLevel | "";
  permission_policy: string;
  system_prompt: string;
  system_prompt_mode: SystemPromptMode | "";
  mcp_preset_keys: string[];
  skill_asset_keys: string[];
  capability_directives: CapabilityKey[];
  allowed_companions: string[];
}

export function presetToForm(preset?: AgentPreset): PresetFormState {
  const cfg = preset?.config ?? {};
  const rawMcpPresetKeys = Array.isArray(cfg.mcp_preset_keys)
    ? (cfg.mcp_preset_keys as string[])
    : [];
  const rawSkillAssetKeys = Array.isArray(cfg.skill_asset_keys)
    ? (cfg.skill_asset_keys as string[])
    : [];
  const rawDirectives = Array.isArray(cfg.capability_directives) ? cfg.capability_directives as CapabilityDirective[] : [];
  const capKeys: CapabilityKey[] = rawDirectives
    .filter((d): d is { add: CapabilityKey } => "add" in d)
    .map((d) => d.add);
  const rawCompanions = Array.isArray(cfg.allowed_companions) ? (cfg.allowed_companions as string[]) : [];
  return {
    name: preset?.name ?? "",
    display_name: String(cfg.display_name ?? ""),
    description: String(cfg.description ?? ""),
    agent_type: preset?.agent_type ?? "",
    provider_id: String(cfg.provider_id ?? ""),
    model_id: String(cfg.model_id ?? ""),
    agent_id: String(cfg.agent_id ?? ""),
    thinking_level: isThinkingLevel(cfg.thinking_level) ? cfg.thinking_level : "",
    permission_policy: String(cfg.permission_policy ?? ""),
    system_prompt: String(cfg.system_prompt ?? ""),
    system_prompt_mode: (cfg.system_prompt_mode === "override" || cfg.system_prompt_mode === "append") ? cfg.system_prompt_mode : "",
    mcp_preset_keys: rawMcpPresetKeys,
    skill_asset_keys: rawSkillAssetKeys,
    capability_directives: capKeys,
    allowed_companions: rawCompanions,
  };
}

export function formToPreset(form: PresetFormState): AgentPreset {
  const config: Record<string, unknown> = {};
  if (form.display_name.trim()) config.display_name = form.display_name.trim();
  if (form.description.trim()) config.description = form.description.trim();
  if (form.provider_id.trim()) config.provider_id = form.provider_id.trim();
  if (form.model_id.trim()) config.model_id = form.model_id.trim();
  if (form.agent_id.trim()) config.agent_id = form.agent_id.trim();
  if (form.thinking_level) config.thinking_level = form.thinking_level;
  if (form.permission_policy.trim()) config.permission_policy = form.permission_policy.trim();
  if (form.system_prompt.trim()) config.system_prompt = form.system_prompt.trim();
  if (form.system_prompt.trim() && form.system_prompt_mode) config.system_prompt_mode = form.system_prompt_mode;
  if (form.mcp_preset_keys.length > 0) config.mcp_preset_keys = form.mcp_preset_keys;
  if (form.skill_asset_keys.length > 0) config.skill_asset_keys = form.skill_asset_keys;
  if (form.capability_directives.length > 0) {
    config.capability_directives = form.capability_directives.map((key) => ({ add: key }));
  }
  if (form.allowed_companions.length > 0) config.allowed_companions = form.allowed_companions;
  return {
    name: form.name.trim(),
    agent_type: form.agent_type.trim(),
    config,
  };
}

export function validateForm(form: PresetFormState, existingNames: string[], editingName?: string): string | null {
  if (!form.name.trim()) return "预设名称不能为空";
  if (!form.agent_type.trim()) return "Agent 类型不能为空";
  const filtered = editingName
    ? existingNames.filter((n) => n !== editingName)
    : existingNames;
  if (filtered.includes(form.name.trim())) {
    return `预设名称 "${form.name.trim()}" 已存在`;
  }
  return null;
}

export function formatPresetSummary(preset: AgentPreset): string {
  const cfg = preset.config ?? {};
  const displayName = String(cfg.display_name ?? "").trim();
  const parts: string[] = [preset.agent_type];
  if (displayName && displayName !== preset.name) parts.unshift(displayName);
  const desc = String(cfg.description ?? "").trim();
  if (desc) parts.push(desc);
  const presetKeys = Array.isArray(cfg.mcp_preset_keys) ? (cfg.mcp_preset_keys as string[]) : [];
  if (presetKeys.length > 0) parts.push(`${presetKeys.length} MCP Preset`);
  const skillKeys = Array.isArray(cfg.skill_asset_keys) ? (cfg.skill_asset_keys as string[]) : [];
  if (skillKeys.length > 0) parts.push(`${skillKeys.length} Skill`);
  return parts.join(" · ");
}
