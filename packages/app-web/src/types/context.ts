import type { ThinkingLevel } from "./project-agent";
import type {
  ContextContainerDefinition,
  ContextContainerFile,
  ContextContainerProvider,
  SessionComposition,
  SessionRequiredContextBlock,
  VfsCapabilityDto,
} from "../generated/context-contracts";
import type {
  ResolvedMountEditCapabilities,
  ResolvedMountPurpose,
  ResolvedMountSummary,
  ResolvedVfsSurface,
  ResolvedVfsSurfaceSource,
} from "../generated/vfs-contracts";

export type {
  ContextContainerDefinition,
  ContextContainerFile,
  ContextContainerProvider,
  ResolvedMountEditCapabilities,
  ResolvedMountPurpose,
  ResolvedMountSummary,
  ResolvedVfsSurface,
  ResolvedVfsSurfaceSource,
  SessionComposition,
  SessionRequiredContextBlock,
};

// ─── VFS Mount 配置 / 会话编排 ──────────────────

export type ContextContainerCapability = VfsCapabilityDto;

// ─── 执行时 Mount / VFS ─────────────────────────────

export type ExecutionMountCapability = "read" | "write" | "list" | "search" | "exec";

export interface ExecutionMount {
  id: string;
  provider: string;
  backend_id: string;
  root_ref: string;
  capabilities: ExecutionMountCapability[];
  default_write: boolean;
  display_name: string;
  metadata?: Record<string, unknown>;
}

export interface ExecutionVfs {
  mounts: ExecutionMount[];
  default_mount_id?: string | null;
}

export interface RuntimeMcpServerSummary {
  name: string;
  transport: string;
  target: string;
}

export interface TaskSessionToolVisibilitySummary {
  markdown: string;
  resolved: boolean;
  toolset_label: string;
  tool_names: string[];
  mcp_servers: RuntimeMcpServerSummary[];
}

export interface TaskSessionRuntimePolicySummary {
  markdown: string;
  workspace_attached: boolean;
  vfs_attached: boolean;
  mcp_enabled: boolean;
  visible_mounts: string[];
  visible_tools: string[];
  writable_mounts: string[];
  exec_mounts: string[];
  path_policy: string;
}

export interface TaskSessionExecutorSummary {
  executor?: string | null;
  provider_id?: string | null;
  model_id?: string | null;
  agent_id?: string | null;
  /** 推理级别（替代旧的 reasoning_id） */
  thinking_level?: ThinkingLevel | null;
  permission_policy?: string | null;
  preset_name?: string | null;
  source: string;
  resolution_error?: string | null;
}

export interface SessionProjectDefaults {
  default_agent_type?: string | null;
  context_containers: ContextContainerDefinition[];
}

export interface SessionStoryOverrides {
  context_containers: ContextContainerDefinition[];
  disabled_container_ids: string[];
  session_composition?: SessionComposition | null;
}

export interface SessionEffectiveContext {
  session_composition: SessionComposition;
  tool_visibility: TaskSessionToolVisibilitySummary;
  runtime_policy: TaskSessionRuntimePolicySummary;
}

export type SessionOwnerContext =
  | { owner_level: "task"; story_overrides: SessionStoryOverrides }
  | { owner_level: "story"; story_overrides: SessionStoryOverrides }
  | { owner_level: "project"; agent_key: string; agent_display_name: string };

export interface SessionContextSnapshot {
  executor: TaskSessionExecutorSummary;
  project_defaults: SessionProjectDefaults;
  effective: SessionEffectiveContext;
  owner_context: SessionOwnerContext;
}

// ─── Session Baseline Capabilities ──────────────────

export interface CompanionAgentEntry {
  name: string;
  executor: string;
  display_name: string;
}

export interface SkillEntry {
  name: string;
  capability_key?: string;
  provider_key?: string;
  local_name?: string;
  display_name?: string | null;
  description: string;
  file_path: string;
  base_dir?: string | null;
  exposure?: SkillContextExposure;
  disable_model_invocation?: boolean;
}

export type SkillContextExposure = "default_exposed" | "explicit_only";

export interface SkillIdentityFields {
  name?: string;
  capability_key?: string;
  provider_key?: string;
  local_name?: string;
  display_name?: string | null;
  exposure?: SkillContextExposure;
  disable_model_invocation?: boolean;
}

export interface SkillCapabilityEntry {
  capability_key: string;
  provider_key: string;
  local_name: string;
  display_name?: string | null;
  description: string;
  file_path: string;
  base_dir?: string | null;
  exposure?: SkillContextExposure;
  disable_model_invocation?: boolean;
}

export interface SkillProviderCluster {
  provider_key: string;
  display_name: string;
  model_summary?: string | null;
  ui_summary?: string | null;
  inventory_hint?: string | null;
  inventory_count?: number | null;
  default_exposed_skills: SkillCapabilityEntry[];
}

export interface SkillDiscoveryDiagnostic {
  provider_key: string;
  code: string;
  message: string;
  local_name?: string | null;
  file_path?: string | null;
}

export interface SessionBaselineCapabilities {
  skills: SkillEntry[];
  skill_clusters?: SkillProviderCluster[];
  skill_diagnostics?: SkillDiscoveryDiagnostic[];
}

export function skillDisplayLabel(skill: SkillIdentityFields): string {
  return (
    skill.display_name
    ?? skill.local_name
    ?? skill.name
    ?? skill.capability_key
    ?? "skill"
  );
}

export function skillIdentityKey(skill: SkillIdentityFields): string {
  if (skill.capability_key) return skill.capability_key;
  if (skill.provider_key && skill.local_name) return `${skill.provider_key}/${skill.local_name}`;
  return skill.name ?? skill.local_name ?? "skill";
}

export function isDefaultExposedSkill(skill: SkillIdentityFields): boolean {
  return (skill.exposure ?? "default_exposed") === "default_exposed";
}

export function isModelInvocationVisibleSkill(skill: SkillIdentityFields): boolean {
  return isDefaultExposedSkill(skill) && skill.disable_model_invocation !== true;
}
