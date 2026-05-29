import type { ThinkingLevel } from "./index";
import type {
  ContextContainerDefinition,
  ContextContainerFile,
  ContextContainerProvider,
  SessionComposition,
  SessionRequiredContextBlock,
  VfsCapabilityDto,
} from "../generated/core-contracts";
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

export interface TaskSessionMcpServerSummary {
  name: string;
  transport: string;
  target: string;
}

export interface TaskSessionToolVisibilitySummary {
  markdown: string;
  resolved: boolean;
  toolset_label: string;
  tool_names: string[];
  mcp_servers: TaskSessionMcpServerSummary[];
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
  description: string;
  file_path: string;
  disable_model_invocation?: boolean;
}

export interface SessionBaselineCapabilities {
  skills: SkillEntry[];
}

export interface StorySessionInfo {
  binding_id: string;
  session_id: string;
  session_title: string | null;
  last_activity: number | null;
  vfs: ExecutionVfs | null;
  runtime_surface: ResolvedVfsSurface | null;
  context_snapshot: SessionContextSnapshot | null;
}

export interface ProjectSessionInfo {
  binding_id: string;
  session_id: string;
  session_title: string | null;
  last_activity: number | null;
  vfs: ExecutionVfs | null;
  runtime_surface: ResolvedVfsSurface | null;
  context_snapshot: SessionContextSnapshot | null;
}
