import type { Artifact } from "./acp";
import type {
  ContextContainerCapability,
  ContextContainerDefinition,
  SessionComposition,
} from "./context";

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
export type WorkspaceStatus = "pending" | "preparing" | "ready" | "active" | "archived" | "error";
export type WorkspaceIdentityKind = "git_repo" | "p4_workspace" | "local_dir";
export type WorkspaceBindingStatus = "pending" | "ready" | "offline" | "error";
export type WorkspaceResolutionPolicy = "prefer_default_binding" | "prefer_online";
export type ThinkingLevel = "off" | "minimal" | "low" | "medium" | "high" | "xhigh";
export type AuthMode = "personal" | "enterprise";
export type ProjectVisibility = "private" | "template_visible";
export type ProjectRole = "owner" | "editor" | "viewer";
export type ProjectSubjectType = "user" | "group";

export const THINKING_LEVEL_OPTIONS: Array<{ value: ThinkingLevel; label: string }> = [
  { value: "off", label: "关闭" },
  { value: "minimal", label: "最少" },
  { value: "low", label: "低" },
  { value: "medium", label: "中" },
  { value: "high", label: "高" },
  { value: "xhigh", label: "超高" },
];

export function isThinkingLevel(value: unknown): value is ThinkingLevel {
  return (
    value === "off"
    || value === "minimal"
    || value === "low"
    || value === "medium"
    || value === "high"
    || value === "xhigh"
  );
}

export function isAuthMode(value: unknown): value is AuthMode {
  return value === "personal" || value === "enterprise";
}

export type ToolCluster = "read" | "write" | "execute" | "workflow" | "collaboration" | "canvas";

export type ToolClusterGroup = "basic" | "extended";

export interface ToolClusterOption {
  value: ToolCluster;
  label: string;
  description: string;
  group: ToolClusterGroup;
}

export const TOOL_CLUSTER_GROUPS: Array<{ key: ToolClusterGroup; label: string }> = [
  { key: "basic", label: "基础能力" },
  { key: "extended", label: "扩展能力" },
];

export const TOOL_CLUSTER_OPTIONS: ToolClusterOption[] = [
  { value: "read", label: "只读访问", description: "文件读取、目录列表、搜索", group: "basic" },
  { value: "write", label: "文件写入", description: "文件写入、补丁应用", group: "basic" },
  { value: "execute", label: "命令执行", description: "Shell 命令执行", group: "basic" },
  { value: "workflow", label: "工作流", description: "Workflow 产出汇报", group: "extended" },
  { value: "collaboration", label: "协作", description: "Companion 派发、回传、Hook 审核", group: "extended" },
  { value: "canvas", label: "Canvas", description: "Canvas 资产创建与展示", group: "extended" },
];

export type SystemPromptMode = "append" | "override";

export interface ProjectSchedulingConfig {
  stall_timeout_ms?: number | null;
}

// ─── 登录 / 认证 ──────────────────────────────────────

export interface LoginFieldDescriptor {
  name: string;
  label: string;
  field_type: string;
  placeholder?: string | null;
  required: boolean;
}

export interface LoginMetadata {
  provider_type: string;
  display_name: string;
  description?: string | null;
  fields: LoginFieldDescriptor[];
  requires_login: boolean;
}

export interface LoginCredentials {
  username: string;
  password: string;
  extra?: unknown;
}

export interface LoginResponse {
  access_token: string;
  identity: CurrentUser;
}

// ─── 当前用户 / 身份 ─────────────────────────────────

export interface AuthGroup {
  group_id: string;
  display_name?: string | null;
}

export interface CurrentUser {
  auth_mode: AuthMode;
  user_id: string;
  subject: string;
  display_name?: string | null;
  email?: string | null;
  groups: AuthGroup[];
  is_admin: boolean;
  provider?: string | null;
  extra: unknown;
}

export interface DirectoryUser {
  user_id: string;
  subject: string;
  auth_mode: string;
  display_name?: string | null;
  email?: string | null;
  is_admin: boolean;
  provider?: string | null;
  created_at: string;
  updated_at: string;
}

export interface DirectoryGroup {
  group_id: string;
  display_name?: string | null;
  created_at: string;
  updated_at: string;
}

// ─── Project ──────────────────────────────────────────

export interface McpHttpHeader {
  name: string;
  value: string;
}

export interface McpEnvVar {
  name: string;
  value: string;
}

export type McpTransportConfig =
  | { type: 'http'; url: string; headers?: McpHttpHeader[] }
  | { type: 'sse'; url: string; headers?: McpHttpHeader[] }
  | { type: 'stdio'; command: string; args?: string[]; env?: McpEnvVar[] }

export type McpRoutePolicy = 'auto' | 'relay' | 'direct';

export interface AgentPreset {
  name: string;
  agent_type: string;
  config: Record<string, unknown>;
}

// ─── Agent 独立实体（新模型）───

export interface AgentEntity {
  id: string;
  name: string;
  agent_type: string;
  base_config: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface ProjectAgentLink {
  id: string;
  project_id: string;
  agent_id: string;
  agent_name: string;
  agent_type: string;
  merged_config: Record<string, unknown>;
  config_override: Record<string, unknown> | null;
  default_lifecycle_key: string | null;
  is_default_for_story: boolean;
  is_default_for_task: boolean;
  knowledge_enabled: boolean;
  project_container_ids: string[];
  created_at: string;
  updated_at: string;
}

export interface ProjectConfig {
  default_agent_type?: string | null;
  default_workspace_id?: string | null;
  agent_presets?: AgentPreset[];
  context_containers: ContextContainerDefinition[];
  scheduling?: ProjectSchedulingConfig;
}

export interface Project {
  id: string;
  name: string;
  description: string;
  config: ProjectConfig;
  created_by_user_id: string;
  updated_by_user_id: string;
  visibility: ProjectVisibility;
  is_template: boolean;
  cloned_from_project_id?: string | null;
  access: ProjectAccessSummary;
  created_at: string;
  updated_at: string;
}

export interface ProjectAccessSummary {
  role?: ProjectRole | null;
  can_view: boolean;
  can_edit: boolean;
  can_manage_sharing: boolean;
  via_admin_bypass: boolean;
  via_template_visibility: boolean;
}

export interface ProjectSubjectGrant {
  project_id: string;
  subject_type: ProjectSubjectType;
  subject_id: string;
  role: ProjectRole;
  granted_by_user_id: string;
  created_at: string;
  updated_at: string;
}

export interface ProjectAgentExecutor {
  executor: string;
  provider_id?: string | null;
  model_id?: string | null;
  agent_id?: string | null;
  thinking_level?: ThinkingLevel | null;
  permission_policy?: string | null;
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
  session?: ProjectAgentSession | null;
}

export interface OpenProjectAgentSessionResult {
  created: boolean;
  session_id: string;
  binding_id: string;
  agent: ProjectAgentSummary;
}

// ─── Workspace ────────────────────────────────────────

export interface WorkspaceBinding {
  id: string;
  workspace_id: string;
  backend_id: string;
  root_ref: string;
  status: WorkspaceBindingStatus;
  detected_facts: Record<string, unknown>;
  last_verified_at?: string | null;
  priority: number;
  created_at: string;
  updated_at: string;
}

export interface Workspace {
  id: string;
  project_id: string;
  name: string;
  identity_kind: WorkspaceIdentityKind;
  identity_payload: Record<string, unknown>;
  resolution_policy: WorkspaceResolutionPolicy;
  default_binding_id?: string | null;
  status: WorkspaceStatus;
  bindings: WorkspaceBinding[];
  mount_capabilities: ContextContainerCapability[];
  created_at: string;
  updated_at: string;
}

export interface WorkspaceDetectionResult {
  identity_kind: WorkspaceIdentityKind;
  identity_payload: Record<string, unknown>;
  binding: WorkspaceBinding;
  confidence: string;
  warnings: string[];
  matched_workspace_ids: string[];
}

// ─── Story ────────────────────────────────────────────

export type StoryPriority = "p0" | "p1" | "p2" | "p3";

export type StoryType = "feature" | "bugfix" | "refactor" | "docs" | "test" | "other";

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
  source_refs: ContextSourceRef[];
  context_containers: ContextContainerDefinition[];
  disabled_container_ids: string[];
  session_composition?: SessionComposition | null;
}

export interface Story {
  id: string;
  project_id: string;
  default_workspace_id?: string | null;
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
  thinking_level?: ThinkingLevel | null;
  context_sources: ContextSourceRef[];
}

export interface Task {
  id: string;
  project_id: string;
  story_id: string;
  workspace_id?: string | null;
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

// ─── Routine ─────────────────────────────────────────────

export type RoutineTriggerType = "scheduled" | "webhook" | "plugin";
export type RoutineSessionMode = "fresh" | "reuse" | "per_entity";
export type RoutineExecutionStatus = "pending" | "running" | "completed" | "failed" | "skipped";

export interface RoutineTriggerConfig {
  type: RoutineTriggerType;
  // Scheduled
  cron_expression?: string;
  timezone?: string | null;
  // Webhook
  endpoint_id?: string;
  auth_token_hash?: string;
  // Plugin
  provider_key?: string;
  provider_config?: Record<string, unknown>;
}

export interface RoutineSessionStrategy {
  mode: RoutineSessionMode;
  entity_key_path?: string;
}

export interface Routine {
  id: string;
  project_id: string;
  name: string;
  prompt_template: string;
  agent_id: string;
  trigger_config: RoutineTriggerConfig;
  session_strategy: RoutineSessionStrategy;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  last_fired_at: string | null;
}

export interface RoutineCreationResponse extends Routine {
  webhook_token?: string | null;
}

export interface RoutineExecution {
  id: string;
  routine_id: string;
  trigger_source: string;
  trigger_payload: Record<string, unknown> | null;
  resolved_prompt: string | null;
  session_id: string | null;
  status: RoutineExecutionStatus;
  started_at: string;
  completed_at: string | null;
  error: string | null;
  entity_key: string | null;
}

export interface RegenerateTokenResponse {
  endpoint_id: string;
  webhook_token: string;
}

// ─── Re-exports from domain-split files ──────────────────

export * from "./context";
export * from "./workflow";
export * from "./canvas";
export * from "./session";
export * from "./mcp-preset";
export * from "./acp";
