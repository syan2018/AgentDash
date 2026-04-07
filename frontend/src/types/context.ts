import type { ThinkingLevel } from "./index";

// ─── 上下文容器 / 挂载策略 / 会话编排 ──────────────────

export type ContextContainerCapability = "read" | "write" | "list" | "search" | "exec";

export interface ContextContainerFile {
  path: string;
  content: string;
}

export type ContextContainerProvider =
  | { kind: "inline_files"; files: ContextContainerFile[] }
  | { kind: "external_service"; service_id: string; root_ref: string };

export interface ContextContainerExposure {
  include_in_project_sessions: boolean;
  include_in_task_sessions: boolean;
  include_in_story_sessions: boolean;
  allowed_agent_types: string[];
}

export interface ContextContainerDefinition {
  id: string;
  mount_id: string;
  display_name: string;
  provider: ContextContainerProvider;
  capabilities: ContextContainerCapability[];
  default_write: boolean;
  exposure: ContextContainerExposure;
}

export interface SessionRequiredContextBlock {
  title: string;
  content: string;
}

export interface SessionComposition {
  persona_label?: string | null;
  persona_prompt?: string | null;
  workflow_steps: string[];
  required_context_blocks: SessionRequiredContextBlock[];
}

// ─── 执行时 Mount / Address Space ───────────────────

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

export interface ExecutionAddressSpace {
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
  address_space_attached: boolean;
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
  | { owner_level: "project"; agent_key: string; agent_display_name: string; shared_context_mounts: ProjectAgentMount[] };

export interface SessionContextSnapshot {
  executor: TaskSessionExecutorSummary;
  project_defaults: SessionProjectDefaults;
  effective: SessionEffectiveContext;
  owner_context: SessionOwnerContext;
}

export interface StorySessionInfo {
  binding_id: string;
  session_id: string;
  session_title: string | null;
  last_activity: number | null;
  address_space: ExecutionAddressSpace | null;
  context_snapshot: SessionContextSnapshot | null;
}

export interface ProjectSessionInfo {
  binding_id: string;
  session_id: string;
  session_title: string | null;
  last_activity: number | null;
  address_space: ExecutionAddressSpace | null;
  context_snapshot: SessionContextSnapshot | null;
}