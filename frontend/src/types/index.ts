// ─── 基础枚举 ─────────────────────────────────────────

export type StoryStatus =
  | "draft"
  | "ready"
  | "running"
  | "review"
  | "completed"
  | "failed"
  | "cancelled";

export type TaskStatus =
  | "pending"
  | "assigned"
  | "running"
  | "awaiting_verification"
  | "completed"
  | "failed";

export type TaskExecutionMode = "standard" | "auto_retry" | "one_shot";

export type BackendType = "local" | "remote";
export type WorkspaceType = "git_worktree" | "static" | "ephemeral";
export type WorkspaceStatus = "pending" | "preparing" | "ready" | "active" | "archived" | "error";

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

export interface MountDerivationPolicy {
  include_local_workspace: boolean;
  local_workspace_capabilities: ContextContainerCapability[];
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
  variant?: string | null;
  model_id?: string | null;
  agent_id?: string | null;
  reasoning_id?: string | null;
  permission_policy?: string | null;
  preset_name?: string | null;
  source: string;
  resolution_error?: string | null;
}

export interface SessionProjectDefaults {
  default_agent_type?: string | null;
  context_containers: ContextContainerDefinition[];
  mount_policy: MountDerivationPolicy;
  session_composition: SessionComposition;
}

export interface SessionStoryOverrides {
  context_containers: ContextContainerDefinition[];
  disabled_container_ids: string[];
  mount_policy_override?: MountDerivationPolicy | null;
  session_composition_override?: SessionComposition | null;
}

export interface SessionEffectiveContext {
  mount_policy: MountDerivationPolicy;
  session_composition: SessionComposition;
  tool_visibility: TaskSessionToolVisibilitySummary;
  runtime_policy: TaskSessionRuntimePolicySummary;
}

export interface SessionContextSnapshot {
  project_defaults: SessionProjectDefaults;
  story_overrides: SessionStoryOverrides;
  effective: SessionEffectiveContext;
}

export interface TaskSessionContextSnapshot {
  executor: TaskSessionExecutorSummary;
  project_defaults: SessionProjectDefaults;
  story_overrides: SessionStoryOverrides;
  effective: SessionEffectiveContext;
}

export interface StorySessionContextSnapshot extends SessionContextSnapshot {
  executor: TaskSessionExecutorSummary;
}

export interface StorySessionInfo {
  binding_id: string;
  session_id: string;
  session_title: string | null;
  last_activity: number | null;
  address_space: ExecutionAddressSpace | null;
  context_snapshot: StorySessionContextSnapshot | null;
}

export interface ProjectSessionContextSnapshot {
  agent_key: string;
  agent_display_name: string;
  executor: TaskSessionExecutorSummary;
  project_defaults: SessionProjectDefaults;
  effective: SessionEffectiveContext;
  shared_context_mounts: ProjectAgentMount[];
}

export interface ProjectSessionInfo {
  binding_id: string;
  session_id: string;
  session_title: string | null;
  last_activity: number | null;
  address_space: ExecutionAddressSpace | null;
  context_snapshot: ProjectSessionContextSnapshot | null;
}

// ─── Project ──────────────────────────────────────────

export interface AgentPreset {
  name: string;
  agent_type: string;
  config: Record<string, unknown>;
}

export interface ProjectConfig {
  default_agent_type?: string | null;
  default_workspace_id?: string | null;
  agent_presets: AgentPreset[];
  context_containers: ContextContainerDefinition[];
  mount_policy: MountDerivationPolicy;
  session_composition: SessionComposition;
}

export interface Project {
  id: string;
  name: string;
  description: string;
  backend_id: string;
  config: ProjectConfig;
  created_at: string;
  updated_at: string;
}

export type ProjectAgentWritebackMode = "read_only" | "confirm_before_write";

export interface ProjectAgentExecutor {
  executor: string;
  variant?: string | null;
  model_id?: string | null;
  agent_id?: string | null;
  reasoning_id?: string | null;
  permission_policy?: string | null;
}

export interface ProjectAgentMount {
  container_id: string;
  mount_id: string;
  display_name: string;
  writable: boolean;
}

export interface ProjectAgentSession {
  binding_id: string;
  session_id: string;
  session_title: string | null;
  last_activity: number | null;
}

export interface ProjectAgentSummary {
  key: string;
  display_name: string;
  description: string;
  executor: ProjectAgentExecutor;
  preset_name?: string | null;
  source: string;
  writeback_mode: ProjectAgentWritebackMode;
  shared_context_mounts: ProjectAgentMount[];
  session?: ProjectAgentSession | null;
}

export interface OpenProjectAgentSessionResult {
  created: boolean;
  session_id: string;
  binding_id: string;
  agent: ProjectAgentSummary;
}

// ─── Workspace ────────────────────────────────────────

export interface GitConfig {
  source_repo: string;
  branch: string;
  commit_hash?: string | null;
}

export interface Workspace {
  id: string;
  project_id: string;
  backend_id: string;
  name: string;
  container_ref: string;
  workspace_type: WorkspaceType;
  status: WorkspaceStatus;
  git_config?: GitConfig | null;
  created_at: string;
  updated_at: string;
}

// ─── Story ────────────────────────────────────────────

export type StoryPriority = "p0" | "p1" | "p2" | "p3";

export type StoryType = "feature" | "bugfix" | "refactor" | "docs" | "test" | "other";

export interface ResourceRef {
  name: string;
  uri: string;
  resource_type: string;
}

export type ContextSourceKind =
  | "manual_text"
  | "file"
  | "project_snapshot"
  | "http_fetch"
  | "mcp_resource"
  | "entity_ref";
export type ContextSlot = "requirements" | "constraints" | "codebase" | "references" | "instruction_append";
export type ContextDelivery = "inline" | "resource" | "lazy";

export interface ContextSourceRef {
  kind: ContextSourceKind;
  locator: string;
  label?: string | null;
  slot: ContextSlot;
  priority: number;
  required: boolean;
  max_chars?: number | null;
  delivery: ContextDelivery;
}

export interface StoryContext {
  prd_doc?: string | null;
  spec_refs: string[];
  resource_list: ResourceRef[];
  source_refs: ContextSourceRef[];
  context_containers: ContextContainerDefinition[];
  disabled_container_ids: string[];
  mount_policy_override?: MountDerivationPolicy | null;
  session_composition_override?: SessionComposition | null;
}

export interface Story {
  id: string;
  project_id: string;
  backend_id: string;
  title: string;
  description?: string;
  status: StoryStatus;
  priority: StoryPriority;
  story_type: StoryType;
  tags: string[];
  task_count: number;
  context: StoryContext;
  created_at: string;
  updated_at: string;
}

// ─── Task ─────────────────────────────────────────────

export interface AgentBinding {
  agent_type?: string | null;
  agent_pid?: string | null;
  preset_name?: string | null;
  prompt_template?: string | null;
  initial_context?: string | null;
  context_sources: ContextSourceRef[];
}

export interface Task {
  id: string;
  story_id: string;
  workspace_id?: string | null;
  session_id?: string | null;
  executor_session_id?: string | null;
  title: string;
  description?: string;
  status: TaskStatus;
  execution_mode: TaskExecutionMode;
  agent_binding: AgentBinding;
  artifacts: Artifact[];
  created_at: string;
  updated_at: string;
}

// ─── SessionBinding ─────────────────────────────────

export type SessionOwnerType = "project" | "story" | "task";

export interface SessionBinding {
  id: string;
  session_id: string;
  owner_type: SessionOwnerType;
  owner_id: string;
  label: string;
  created_at: string;
  session_title?: string;
  session_updated_at?: number;
}

export interface SessionTaskContext {
  task_id: string;
  agent_binding?: AgentBinding;
}

export type SessionReturnTarget =
  | {
      owner_type: "project";
      project_id: string;
    }
  | {
      owner_type: "story";
      story_id: string;
    }
  | {
      owner_type: "task";
      story_id: string;
      task_id: string;
    };

export interface SessionBindingOwner {
  id: string;
  session_id: string;
  owner_type: SessionOwnerType;
  owner_id: string;
  label: string;
  created_at: string;
  owner_title?: string | null;
  project_id?: string | null;
  story_id?: string | null;
  task_id?: string | null;
}

export interface ProjectSessionAgentContext {
  agent_key: string;
  display_name: string;
  executor_hint?: string | null;
}

export interface SessionNavigationState {
  task_context?: SessionTaskContext;
  project_agent?: ProjectSessionAgentContext;
  return_to?: SessionReturnTarget;
}

export interface StoryNavigationState {
  open_task_id?: string;
}

// ─── Artifact / ACP 展示类型 ──────────────────────────

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "image"; data: string; mimeType: string }
  | { type: "resource_link"; uri: string; name: string; description?: string; mimeType?: string; size?: number }
  | { type: "resource"; resource: { uri: string; mimeType?: string; text?: string } };

export type ArtifactType =
  | "code_change"
  | "test_result"
  | "log_output"
  | "file"
  | "tool_execution";

export interface Artifact {
  id: string;
  artifact_type: ArtifactType;
  content: unknown;
  created_at: string;
}

export interface ToolCall {
  title: string;
  kind:
    | "read"
    | "edit"
    | "delete"
    | "move"
    | "search"
    | "execute"
    | "think"
    | "fetch"
    | "switch_mode"
    | "other";
  status?: "pending" | "in_progress" | "completed" | "failed";
  rawInput?: unknown;
  rawOutput?: unknown;
}

export interface PlanEntry {
  content: string;
  priority: "high" | "medium" | "low";
  status: "pending" | "in_progress" | "completed";
}

export interface ConfirmationRequest {
  stagedTaskId: string;
  title: string;
  description?: string;
  requestKind: string;
  createdAt: string;
  projectId?: string;
}

export type SessionUpdate =
  | { type: "content"; blocks: ContentBlock[] }
  | { type: "tool_call"; toolCall: ToolCall }
  | { type: "plan"; entries: PlanEntry[] }
  | { type: "confirmation_request"; request: ConfirmationRequest };

// ─── Backend ──────────────────────────────────────────

export interface BackendConfig {
  id: string;
  name: string;
  endpoint: string;
  auth_token: string | null;
  enabled: boolean;
  backend_type: BackendType;
  /** WebSocket 中继在线状态（由 API 附加） */
  online?: boolean;
  /** 在线后端的可访问根路径 */
  accessible_roots?: string[];
  /** 在线后端的执行器能力 */
  capabilities?: {
    executors: Array<{
      id: string;
      name: string;
      variants: string[];
      available: boolean;
    }>;
    supports_cancel: boolean;
    supports_workspace_files: boolean;
    supports_discover_options: boolean;
  };
}

export interface ViewConfig {
  id: string;
  name: string;
  backend_ids: string[];
  filters: Record<string, unknown>;
  sort_by: string | null;
}

// ─── SSE 事件 ──────────────────────────────────────────

export interface StateChange {
  id: number;
  entity_id: string;
  kind: string;
  payload: Record<string, unknown>;
  backend_id: string;
  created_at: string;
}

export type StreamEvent =
  | { type: "Connected"; data: { last_event_id: number } }
  | { type: "StateChanged"; data: StateChange }
  | { type: "Heartbeat"; data: { timestamp: number } };
