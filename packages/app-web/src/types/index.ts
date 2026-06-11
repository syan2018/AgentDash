import type { CapabilityDirective } from "./workflow";
import type {
  TaskDispatchPreference as CoreTaskDispatchPreference,
  AgentPreset as CoreAgentPreset,
  Artifact as CoreArtifact,
  BackendResponse,
  BackendWithStatusResponse,
  ContextSourceRef as CoreContextSourceRef,
  ProjectAccessSummaryResponse,
  ProjectConfig as CoreProjectConfig,
  ProjectResponse,
  ProjectSubjectGrantResponse,
  StoryResponse,
  TaskResponse,
  WorkspaceBindingResponse,
  WorkspaceIdentityKind,
  WorkspaceResponse,
} from "../generated/core-contracts";
import type {
  ProjectVfsMountContentDto,
  ProjectVfsMountResponse,
} from "../generated/vfs-contracts";
import type {
  CreateProjectAgentSessionRequest as GeneratedCreateProjectAgentSessionRequest,
  ProjectAgent as GeneratedProjectAgent,
  ProjectAgentExecutor as GeneratedProjectAgentExecutor,
  ProjectAgentLaunchResult as GeneratedProjectAgentLaunchResult,
  ProjectAgentSessionStartResult as GeneratedProjectAgentSessionStartResult,
  ProjectAgentSummary as GeneratedProjectAgentSummary,
} from "../generated/project-agent-contracts";

// ─── Generated Core Contracts ─────────────────────────

export type TaskDispatchPreference = CoreTaskDispatchPreference & {
  thinking_level?: ThinkingLevel | null;
};
export type AgentPreset = CoreAgentPreset;
export type Artifact = CoreArtifact;
export type ContextSourceRef = CoreContextSourceRef;
export type Project = ProjectResponse;
export type ProjectAccessSummary = ProjectAccessSummaryResponse;
export type ProjectConfig = CoreProjectConfig;
export type ProjectSubjectGrant = ProjectSubjectGrantResponse;
export type Story = StoryResponse;
export type StoryContext = StoryResponse["context"];
export type Task = Omit<TaskResponse, "dispatch_preference"> & { dispatch_preference: TaskDispatchPreference };
export type Workspace = WorkspaceResponse;
export type WorkspaceBinding = WorkspaceBindingResponse;
export type {
  ContextSourceKind,
  BackendType,
  ProjectRole,
  ProjectSubjectType,
  ProjectVisibility,
  StoryPriority,
  StoryStatus,
  StoryType,
  TaskStatus,
  WorkspaceBindingStatus,
  WorkspaceIdentityKind,
  WorkspaceResolutionPolicy,
  WorkspaceStatus,
} from "../generated/core-contracts";

// ─── 基础枚举 ─────────────────────────────────────────

export type BackendConfig = BackendWithStatusResponse;
export type BackendSafeConfig = BackendResponse;
export type ProjectBackendAccessStatus = "active" | "paused" | "revoked";
export type ProjectBackendAccessMode = "use_inventory";
export type BackendWorkspaceInventoryStatus = "available" | "stale" | "offline" | "error";
export type BackendWorkspaceInventorySource =
  | "runtime_register"
  | "manual_refresh"
  | "scheduled_refresh"
  | "capability_expansion_ack";
export type ThinkingLevel = "off" | "minimal" | "low" | "medium" | "high" | "xhigh";
export type AuthMode = "personal" | "enterprise";

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

export type CapabilityKey =
  | "file_read"
  | "file_write"
  | "shell_execute"
  | "workflow"
  | "collaboration"
  | "workspace_module";

export type CapabilityGroup = "basic" | "extended";

export interface CapabilityOption {
  value: CapabilityKey;
  label: string;
  description: string;
  group: CapabilityGroup;
}

export const CAPABILITY_GROUPS: Array<{ key: CapabilityGroup; label: string }> = [
  { key: "basic", label: "基础能力" },
  { key: "extended", label: "扩展能力" },
];

export const CAPABILITY_OPTIONS: CapabilityOption[] = [
  { value: "file_read", label: "只读访问", description: "文件读取、目录列表、搜索", group: "basic" },
  { value: "file_write", label: "文件写入", description: "文件写入、补丁应用", group: "basic" },
  { value: "shell_execute", label: "命令执行", description: "Shell 命令执行", group: "basic" },
  { value: "workflow", label: "工作流", description: "Workflow 产出汇报", group: "extended" },
  { value: "collaboration", label: "协作", description: "Companion 派发、回传、Hook 审核", group: "extended" },
  { value: "workspace_module", label: "Workspace Module", description: "模块创建、调用与展示，包含 Canvas", group: "extended" },
];

export type SystemPromptMode = "append" | "override";

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
  login_mode?: "form" | "redirect";
  start_url?: string | null;
  requires_login: boolean;
}

export interface AuthStartRequest {
  return_to?: string | null;
}

export interface AuthStartResponse {
  auth_url: string;
  state: string;
  expires_at_epoch_seconds: number;
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
  avatar_url?: string | null;
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
  avatar_url?: string | null;
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

export interface AgentPresetConfig extends Record<string, unknown> {
  executor?: string;
  provider_id?: string;
  model_id?: string;
  agent_id?: string;
  thinking_level?: ThinkingLevel;
  permission_policy?: string;
  system_prompt?: string;
  system_prompt_mode?: SystemPromptMode;
  display_name?: string;
  description?: string;
  capability_directives?: CapabilityDirective[];
  mcp_preset_keys?: string[];
  vfs_access_grants?: AgentVfsAccessGrant[];
  skill_asset_keys?: string[];
  allowed_companions?: string[];
}

export interface AgentVfsAccessGrant {
  mount_id: string;
  capabilities: Array<"read" | "write" | "list" | "search">;
}

export type ProjectVfsMountContent = ProjectVfsMountContentDto;
export type ProjectVfsMount = ProjectVfsMountResponse;

// ─── Project Agent 项目实例 ───

export type ProjectAgent = Pick<
  GeneratedProjectAgent,
  "id" | "project_id" | "name" | "agent_type" | "knowledge_enabled" | "created_at" | "updated_at"
> & {
  config: AgentPresetConfig;
  default_lifecycle_key: string | null;
};

export type ProjectAgentExecutor = Omit<
  GeneratedProjectAgentExecutor,
  "provider_id" | "model_id" | "agent_id" | "thinking_level" | "permission_policy"
> & {
  provider_id?: string | null;
  model_id?: string | null;
  agent_id?: string | null;
  thinking_level?: ThinkingLevel | null;
  permission_policy?: string | null;
};

export type ProjectAgentSummary = Omit<
  GeneratedProjectAgentSummary,
  "executor" | "preset_name"
> & {
  executor: ProjectAgentExecutor;
  preset_name?: string | null;
};

export type ProjectAgentLaunchResult = Omit<
  GeneratedProjectAgentLaunchResult,
  "agent"
> & {
  agent: ProjectAgentSummary;
};

export type CreateProjectAgentSessionRequest = GeneratedCreateProjectAgentSessionRequest;

export type ProjectAgentSessionStartResult = Omit<
  GeneratedProjectAgentSessionStartResult,
  "agent"
> & {
  agent: ProjectAgentSummary;
};

// ─── Workspace ────────────────────────────────────────

export type WorkspaceDetectionResult = {
  identity_kind: WorkspaceIdentityKind;
  identity_payload: Record<string, unknown>;
  binding: WorkspaceBinding;
  confidence: string;
  warnings: string[];
  matched_workspace_ids: string[];
};

export type ProjectBackendAccess = {
  id: string;
  project_id: string;
  backend_id: string;
  status: ProjectBackendAccessStatus;
  access_mode: ProjectBackendAccessMode;
  priority: number;
  root_policy: Record<string, unknown>;
  capability_policy: Record<string, unknown>;
  note?: string | null;
  created_by?: string | null;
  created_at: string;
  updated_at: string;
};

export interface BackendWorkspaceInventory {
  id: string;
  backend_id: string;
  root_ref: string;
  identity_kind: WorkspaceIdentityKind;
  identity_payload: Record<string, unknown>;
  detected_facts: Record<string, unknown>;
  status: BackendWorkspaceInventoryStatus;
  source: BackendWorkspaceInventorySource;
  last_seen_at: string;
  last_error?: string | null;
  created_at: string;
  updated_at: string;
}

export type WorkspaceInventoryCandidate = {
  backend_id: string;
  root_ref: string;
  identity_kind: WorkspaceIdentityKind;
  identity_payload: Record<string, unknown>;
  detected_facts: Record<string, unknown>;
  status: BackendWorkspaceInventoryStatus;
  matched_workspace_ids: string[];
  reason: string;
};

export type WorkspaceBindingSyncResult = {
  updated_workspace_ids: string[];
  created_bindings: number;
  updated_bindings: number;
  candidates: WorkspaceInventoryCandidate[];
  conflicts: WorkspaceInventoryCandidate[];
};

export interface InventoryRefreshResult {
  access_id: string;
  backend_id: string;
  refreshed: number;
  failed: number;
  items: BackendWorkspaceInventory[];
  warnings: string[];
}

// ─── Story / Task ─────────────────────────────────────

// ─── Routine ─────────────────────────────────────────────

export type RoutineTriggerType = "scheduled" | "webhook" | "plugin";
export type RoutineDispatchMode = "fresh" | "reuse" | "per_entity";
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

export interface RoutineDispatchStrategy {
  mode: RoutineDispatchMode;
  entity_key_path?: string;
}

export interface Routine {
  id: string;
  project_id: string;
  name: string;
  prompt_template: string;
  project_agent_id: string;
  trigger_config: RoutineTriggerConfig;
  dispatch_strategy: RoutineDispatchStrategy;
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
  runtime_refs?: AgentRuntimeRefs | null;
  status: RoutineExecutionStatus;
  started_at: string;
  completed_at: string | null;
  error: string | null;
  entity_key: string | null;
}

export interface ActivityBindingRefs {
  graph_instance_ref: string;
  assignment_ref?: string | null;
}

export interface AgentRuntimeRefs {
  run_ref: string;
  agent_ref: string;
  frame_ref: string;
  activity_binding?: ActivityBindingRefs | null;
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
export * from "./skill-asset";
export * from "./extension-runtime";
export * from "./shared-library";
export * from "./acp";
export * from "./lifecycle-views";
