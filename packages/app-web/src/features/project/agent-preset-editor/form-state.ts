import type {
  AgentPreset,
  AgentVfsAccessGrant,
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
  vfs_access_grants: AgentVfsAccessGrant[];
  skill_asset_keys: string[];
  capability_directives: CapabilityKey[];
  default_companion_enabled: boolean;
  extra_companions: string[];
  visible_workspace_module_refs: string[];
}

export function presetToForm(preset?: AgentPreset): PresetFormState {
  const cfg = (preset?.config && typeof preset.config === "object" && !Array.isArray(preset.config)
    ? preset.config
    : {}) as Record<string, unknown>;
  const rawMcpPresetKeys = Array.isArray(cfg.mcp_preset_keys)
    ? (cfg.mcp_preset_keys as string[])
    : [];
  const rawSkillAssetKeys = Array.isArray(cfg.skill_asset_keys)
    ? (cfg.skill_asset_keys as string[])
    : [];
  const rawVfsAccessGrants = Array.isArray(cfg.vfs_access_grants)
    ? (cfg.vfs_access_grants as AgentVfsAccessGrant[])
    : [];
  const rawDirectives = Array.isArray(cfg.capability_directives) ? cfg.capability_directives as CapabilityDirective[] : [];
  const capKeys: CapabilityKey[] = rawDirectives
    .filter((d): d is { add: CapabilityKey } => "add" in d)
    .map((d) => d.add);
  const rawCompanions = Array.isArray(cfg.extra_companions) ? (cfg.extra_companions as string[]) : [];
  const rawVisibleModuleRefs = Array.isArray(cfg.visible_workspace_module_refs)
    ? (cfg.visible_workspace_module_refs as string[])
    : [];
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
    vfs_access_grants: rawVfsAccessGrants,
    skill_asset_keys: rawSkillAssetKeys,
    capability_directives: capKeys,
    default_companion_enabled: cfg.default_companion_enabled === true,
    extra_companions: rawCompanions,
    visible_workspace_module_refs: rawVisibleModuleRefs,
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
  if (form.vfs_access_grants.length > 0) config.vfs_access_grants = form.vfs_access_grants;
  if (form.skill_asset_keys.length > 0) config.skill_asset_keys = form.skill_asset_keys;
  if (form.capability_directives.length > 0) {
    config.capability_directives = form.capability_directives.map((key) => ({ add: key }));
  }
  if (form.default_companion_enabled) config.default_companion_enabled = true;
  if (form.extra_companions.length > 0) config.extra_companions = form.extra_companions;
  // 空 = 全部可见（不写 config，下游默认全集）；非空 = 仅勾选 module。
  if (form.visible_workspace_module_refs.length > 0) {
    config.visible_workspace_module_refs = form.visible_workspace_module_refs;
  }
  return {
    name: form.name.trim(),
    agent_type: form.agent_type.trim(),
    config: config as AgentPreset["config"],
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
  const cfg = (preset.config && typeof preset.config === "object" && !Array.isArray(preset.config)
    ? preset.config
    : {}) as Record<string, unknown>;
  const displayName = String(cfg.display_name ?? "").trim();
  const parts: string[] = [preset.agent_type];
  if (displayName && displayName !== preset.name) parts.unshift(displayName);
  const desc = String(cfg.description ?? "").trim();
  if (desc) parts.push(desc);
  const presetKeys = Array.isArray(cfg.mcp_preset_keys) ? (cfg.mcp_preset_keys as string[]) : [];
  if (presetKeys.length > 0) parts.push(`${presetKeys.length} MCP Preset`);
  const skillKeys = Array.isArray(cfg.skill_asset_keys) ? (cfg.skill_asset_keys as string[]) : [];
  if (skillKeys.length > 0) parts.push(`${skillKeys.length} Skill`);
  const vfsGrants = Array.isArray(cfg.vfs_access_grants) ? (cfg.vfs_access_grants as AgentVfsAccessGrant[]) : [];
  if (vfsGrants.length > 0) parts.push(`${vfsGrants.length} VFS`);
  return parts.join(" · ");
}
