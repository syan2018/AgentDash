import type {
  AgentPreset,
  CapabilityDirective,
  CapabilityKey,
  ProjectVfsMountExposureGrant,
  SystemPromptMode,
  ThinkingLevel,
} from "../../../types";
import {
  CAPABILITY_OPTIONS,
  directiveKind,
  directivePath,
  isThinkingLevel,
  parseCapabilityPath,
} from "../../../types";
import {
  addDirective,
  normalizeDirectives,
  removeDirective,
} from "../../workflow/capability-directive-ops";

const MCP_CAPABILITY_PREFIX = "mcp:";
const TOOL_PATH_SEPARATOR = "::";
const WELL_KNOWN_CAPABILITY_KEYS = new Set<string>(
  CAPABILITY_OPTIONS.map((option) => option.value),
);
const PROJECT_VFS_MOUNT_CAPABILITIES = new Set<string>(["read", "write", "list", "search"]);

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
  project_vfs_mount_exposure_grants: ProjectVfsMountExposureGrant[];
  skill_asset_keys: string[];
  capability_directives: CapabilityDirective[];
  default_companion_enabled: boolean;
  extra_companions: string[];
  visible_workspace_module_refs: string[];
}

export function mcpCapabilityKey(presetKey: string): string {
  const key = presetKey.trim();
  if (!key) throw new Error("MCP Preset key 不能为空");
  if (key.includes(":") || key.includes(TOOL_PATH_SEPARATOR)) {
    throw new Error(`MCP Preset key 非法：${presetKey}`);
  }
  return `${MCP_CAPABILITY_PREFIX}${key}`;
}

export function mcpToolCapabilityPath(presetKey: string, toolName: string): string {
  const tool = toolName.trim();
  if (!tool) throw new Error("MCP tool name 不能为空");
  if (tool.includes(TOOL_PATH_SEPARATOR)) {
    throw new Error(`MCP tool name 非法：${toolName}`);
  }
  return `${mcpCapabilityKey(presetKey)}${TOOL_PATH_SEPARATOR}${tool}`;
}

export function isMcpCapabilityKey(value: string): boolean {
  return extractMcpPresetKey(value) !== null;
}

export function extractMcpPresetKey(value: string): string | null {
  if (!value.startsWith(MCP_CAPABILITY_PREFIX)) return null;
  if (value.includes(TOOL_PATH_SEPARATOR)) return null;
  const key = value.slice(MCP_CAPABILITY_PREFIX.length);
  return key.trim() ? key : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  return true;
}

function stringArrayField(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function isCapabilityDirective(value: unknown): value is CapabilityDirective {
  if (!isRecord(value)) return false;
  const hasAdd = typeof value.add === "string";
  const hasRemove = typeof value.remove === "string";
  return hasAdd !== hasRemove;
}

function isCapabilityKey(value: string): value is CapabilityKey {
  return WELL_KNOWN_CAPABILITY_KEYS.has(value);
}

function isProjectVfsMountExposureGrant(value: unknown): value is ProjectVfsMountExposureGrant {
  if (!isRecord(value)) return false;
  if (typeof value.mount_id !== "string" || !Array.isArray(value.capabilities)) return false;
  return value.capabilities.every(
    (capability) => typeof capability === "string" && PROJECT_VFS_MOUNT_CAPABILITIES.has(capability),
  );
}

function isOwnedWellKnownCapabilityDirective(directive: CapabilityDirective): boolean {
  try {
    const path = parseCapabilityPath(directivePath(directive));
    return path.tool === null && isCapabilityKey(path.capability);
  } catch {
    return false;
  }
}

export function selectedMcpPresetKeysFromDirectives(
  directives: CapabilityDirective[],
): string[] {
  const states = new Map<string, boolean>();
  for (const directive of directives) {
    const path = directivePath(directive);
    const presetKey = extractMcpPresetKey(path);
    if (!presetKey) continue;
    states.set(presetKey, directiveKind(directive) === "add");
  }
  return Array.from(states.entries())
    .filter(([, selected]) => selected)
    .map(([presetKey]) => presetKey);
}

export function addMcpPresetDirective(
  directives: CapabilityDirective[],
  presetKey: string,
): CapabilityDirective[] {
  const capability = mcpCapabilityKey(presetKey);
  const next = directives.filter((directive) => directivePath(directive) !== capability);
  return normalizeDirectives(addDirective(next, { add: capability }));
}

export function removeMcpPresetDirective(
  directives: CapabilityDirective[],
  presetKey: string,
): CapabilityDirective[] {
  const capability = mcpCapabilityKey(presetKey);
  return directives.filter((directive) => {
    const path = directivePath(directive);
    return path !== capability && !path.startsWith(`${capability}${TOOL_PATH_SEPARATOR}`);
  });
}

export function setMcpToolBlockedDirective(
  directives: CapabilityDirective[],
  presetKey: string,
  toolName: string,
  blocked: boolean,
): CapabilityDirective[] {
  const target: CapabilityDirective = { remove: mcpToolCapabilityPath(presetKey, toolName) };
  if (blocked) return normalizeDirectives(addDirective(directives, target));
  return removeDirective(directives, target);
}

export function replaceWellKnownCapabilitySelection(
  directives: CapabilityDirective[],
  selected: CapabilityKey[],
): CapabilityDirective[] {
  const preserved = directives.filter((directive) => !isOwnedWellKnownCapabilityDirective(directive));
  if (selected.length >= CAPABILITY_OPTIONS.length) return preserved;
  return normalizeDirectives([
    ...preserved,
    ...selected.map((key): CapabilityDirective => ({ add: key })),
  ]);
}

export function presetToForm(preset?: AgentPreset): PresetFormState {
  const cfg = isRecord(preset?.config) ? preset.config : {};
  const rawSkillAssetKeys = stringArrayField(cfg.skill_asset_keys);
  const rawProjectVfsMountExposureGrants = Array.isArray(cfg.project_vfs_mount_exposure_grants)
    ? cfg.project_vfs_mount_exposure_grants.filter(isProjectVfsMountExposureGrant)
    : [];
  const rawDirectives = Array.isArray(cfg.capability_directives)
    ? cfg.capability_directives.filter(isCapabilityDirective)
    : [];
  const capabilityDirectives = normalizeDirectives(rawDirectives);
  const rawCompanions = stringArrayField(cfg.extra_companions);
  const rawVisibleModuleRefs = stringArrayField(cfg.visible_workspace_module_refs);
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
    project_vfs_mount_exposure_grants: rawProjectVfsMountExposureGrants,
    skill_asset_keys: rawSkillAssetKeys,
    capability_directives: capabilityDirectives,
    default_companion_enabled: cfg.default_companion_enabled === true,
    extra_companions: rawCompanions,
    visible_workspace_module_refs: rawVisibleModuleRefs,
  };
}

export function formToPreset(form: PresetFormState): AgentPreset {
  const config: AgentPreset["config"] = {};
  if (form.display_name.trim()) config.display_name = form.display_name.trim();
  if (form.description.trim()) config.description = form.description.trim();
  if (form.provider_id.trim()) config.provider_id = form.provider_id.trim();
  if (form.model_id.trim()) config.model_id = form.model_id.trim();
  if (form.agent_id.trim()) config.agent_id = form.agent_id.trim();
  if (form.thinking_level) config.thinking_level = form.thinking_level;
  if (form.permission_policy.trim()) config.permission_policy = form.permission_policy.trim();
  if (form.system_prompt.trim()) config.system_prompt = form.system_prompt.trim();
  if (form.system_prompt.trim() && form.system_prompt_mode) config.system_prompt_mode = form.system_prompt_mode;
  if (form.project_vfs_mount_exposure_grants.length > 0) {
    config.project_vfs_mount_exposure_grants = form.project_vfs_mount_exposure_grants;
  }
  if (form.skill_asset_keys.length > 0) config.skill_asset_keys = form.skill_asset_keys;
  if (form.capability_directives.length > 0) {
    config.capability_directives = normalizeDirectives(form.capability_directives);
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
  const cfg = isRecord(preset.config) ? preset.config : {};
  const displayName = String(cfg.display_name ?? "").trim();
  const parts: string[] = [preset.agent_type];
  if (displayName && displayName !== preset.name) parts.unshift(displayName);
  const desc = String(cfg.description ?? "").trim();
  if (desc) parts.push(desc);
  const directives = Array.isArray(cfg.capability_directives)
    ? cfg.capability_directives.filter(isCapabilityDirective)
    : [];
  const presetKeys = selectedMcpPresetKeysFromDirectives(directives);
  if (presetKeys.length > 0) parts.push(`${presetKeys.length} MCP Preset`);
  const skillKeys = stringArrayField(cfg.skill_asset_keys);
  if (skillKeys.length > 0) parts.push(`${skillKeys.length} Skill`);
  const projectVfsMountExposureGrants = Array.isArray(cfg.project_vfs_mount_exposure_grants)
    ? cfg.project_vfs_mount_exposure_grants.filter(isProjectVfsMountExposureGrant)
    : [];
  if (projectVfsMountExposureGrants.length > 0) parts.push(`${projectVfsMountExposureGrants.length} Project VFS`);
  return parts.join(" · ");
}
