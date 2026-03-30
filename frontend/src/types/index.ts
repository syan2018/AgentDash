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
  mount_policy: MountDerivationPolicy;
}

export interface SessionStoryOverrides {
  context_containers: ContextContainerDefinition[];
  disabled_container_ids: string[];
  mount_policy_override?: MountDerivationPolicy | null;
  session_composition?: SessionComposition | null;
}

export interface SessionEffectiveContext {
  mount_policy: MountDerivationPolicy;
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

// ─── Workflow ─────────────────────────────────────────

export type WorkflowTargetKind = "project" | "story" | "task";

export type WorkflowAgentRole = "project" | "story" | "task";

export type WorkflowRunStatus =
  | "draft"
  | "ready"
  | "running"
  | "blocked"
  | "completed"
  | "failed"
  | "cancelled";

export type WorkflowStepExecutionStatus =
  | "pending"
  | "ready"
  | "running"
  | "completed"
  | "failed"
  | "skipped";

export type WorkflowConstraintKind =
  | "block_stop_until_checks_pass"
  | "custom";

export type WorkflowCheckKind =
  | "artifact_exists"
  | "artifact_count_gte"
  | "session_terminal_in"
  | "checklist_evidence_present"
  | "explicit_action_received"
  | "custom";

export type WorkflowRecordArtifactType =
  | "session_summary"
  | "journal_update"
  | "archive_suggestion"
  | "phase_note"
  | "checklist_evidence"
  | "execution_trace"
  | "decision_record"
  | "context_snapshot";

export interface WorkflowContextBinding {
  locator: string;
  reason: string;
  required: boolean;
  title?: string | null;
}

export interface WorkflowConstraintSpec {
  key: string;
  kind: WorkflowConstraintKind;
  description: string;
  payload?: Record<string, unknown> | null;
}

export interface WorkflowCheckSpec {
  key: string;
  kind: WorkflowCheckKind;
  description: string;
  payload?: Record<string, unknown> | null;
}

export interface WorkflowInjectionSpec {
  goal?: string | null;
  instructions: string[];
  context_bindings: WorkflowContextBinding[];
}

export interface WorkflowCompletionSpec {
  checks: WorkflowCheckSpec[];
  default_artifact_type?: WorkflowRecordArtifactType | null;
  default_artifact_title?: string | null;
}

export type WorkflowHookTrigger =
  | "before_tool"
  | "after_tool"
  | "after_turn"
  | "before_stop"
  | "session_terminal"
  | "before_subagent_dispatch"
  | "after_subagent_dispatch"
  | "subagent_result";

export interface WorkflowHookRuleSpec {
  key: string;
  trigger: WorkflowHookTrigger;
  description: string;
  preset?: string | null;
  params?: Record<string, unknown> | null;
  script?: string | null;
  enabled: boolean;
}

export interface HookRulePreset {
  key: string;
  trigger: WorkflowHookTrigger;
  label: string;
  description: string;
  param_schema?: Record<string, unknown> | null;
  script?: string;
  source?: "builtin" | "user_defined";
}

export interface WorkflowContract {
  injection: WorkflowInjectionSpec;
  hook_rules: WorkflowHookRuleSpec[];
  constraints: WorkflowConstraintSpec[];
  completion: WorkflowCompletionSpec;
}

export type WorkflowDefinitionSource =
  | "builtin_seed"
  | "user_authored"
  | "cloned";

export type WorkflowDefinitionStatus =
  | "draft"
  | "active"
  | "disabled";

export type ValidationSeverity = "error" | "warning";

export interface ValidationIssue {
  code: string;
  message: string;
  field_path: string;
  severity: ValidationSeverity;
}

export interface WorkflowValidationResult {
  valid: boolean;
  issues: ValidationIssue[];
}

export interface WorkflowTemplateWorkflow {
  key: string;
  name: string;
  description: string;
  contract: WorkflowContract;
}

export interface LifecycleStepDefinition {
  key: string;
  description: string;
  workflow_key?: string | null;
}

export interface WorkflowTemplate {
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_roles: WorkflowAgentRole[];
  workflows: WorkflowTemplateWorkflow[];
  lifecycle: {
    key: string;
    name: string;
    description: string;
    entry_step_key: string;
    steps: LifecycleStepDefinition[];
  };
}

export interface WorkflowDefinition {
  id: string;
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_roles: WorkflowAgentRole[];
  source: WorkflowDefinitionSource;
  status: WorkflowDefinitionStatus;
  version: number;
  contract: WorkflowContract;
  created_at: string;
  updated_at: string;
}

export interface LifecycleDefinition {
  id: string;
  key: string;
  name: string;
  description: string;
  target_kind: WorkflowTargetKind;
  recommended_roles: WorkflowAgentRole[];
  source: WorkflowDefinitionSource;
  status: WorkflowDefinitionStatus;
  version: number;
  entry_step_key: string;
  steps: LifecycleStepDefinition[];
  created_at: string;
  updated_at: string;
}

export interface WorkflowAssignment {
  id: string;
  project_id: string;
  lifecycle_id: string;
  role: WorkflowAgentRole;
  enabled: boolean;
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

export interface WorkflowStepState {
  step_key: string;
  status: WorkflowStepExecutionStatus;
  started_at?: string | null;
  completed_at?: string | null;
  summary?: string | null;
  context_snapshot?: Record<string, unknown> | null;
}

export interface WorkflowRecordArtifact {
  id: string;
  step_key: string;
  artifact_type: WorkflowRecordArtifactType;
  title: string;
  content: string;
  created_at: string;
}

export type LifecycleExecutionEventKind =
  | "step_activated"
  | "step_completed"
  | "constraint_blocked"
  | "completion_evaluated"
  | "artifact_appended"
  | "context_injected";

export interface LifecycleExecutionEntry {
  timestamp: string;
  step_key: string;
  event_kind: LifecycleExecutionEventKind;
  summary: string;
  detail?: Record<string, unknown> | null;
}

export interface WorkflowRun {
  id: string;
  project_id: string;
  lifecycle_id: string;
  target_kind: WorkflowTargetKind;
  target_id: string;
  status: WorkflowRunStatus;
  current_step_key?: string | null;
  step_states: WorkflowStepState[];
  record_artifacts: WorkflowRecordArtifact[];
  execution_log: LifecycleExecutionEntry[];
  created_at: string;
  updated_at: string;
  last_activity_at: string;
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

export type McpServerDecl =
  | { type: 'http'; name: string; url: string; headers?: McpHttpHeader[] }
  | { type: 'sse'; name: string; url: string; headers?: McpHttpHeader[] }
  | { type: 'stdio'; name: string; command: string; args?: string[]; env?: McpEnvVar[] }

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
  created_at: string;
  updated_at: string;
}

export interface ProjectConfig {
  default_agent_type?: string | null;
  default_workspace_id?: string | null;
  agent_presets?: AgentPreset[];
  context_containers: ContextContainerDefinition[];
  mount_policy: MountDerivationPolicy;
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

export type ProjectAgentWritebackMode = "read_only" | "confirm_before_write";

export interface ProjectAgentExecutor {
  executor: string;
  variant?: string | null;
  provider_id?: string | null;
  model_id?: string | null;
  agent_id?: string | null;
  thinking_level?: ThinkingLevel | null;
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
  project_id: string;
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
  project_id: string;
  session_id: string;
  owner_type: SessionOwnerType;
  owner_id: string;
  label: string;
  created_at: string;
  owner_title?: string | null;
  story_id?: string | null;
  task_id?: string | null;
}

export interface HookOwnerSummary {
  owner_type: string;
  owner_id: string;
  label?: string | null;
  project_id?: string | null;
  story_id?: string | null;
  task_id?: string | null;
}

export interface HookInjection {
  slot: string;
  content: string;
  source: string;
}

export interface HookDiagnosticEntry {
  code: string;
  message: string;
}

export interface HookCompletionStatus {
  mode: string;
  satisfied: boolean;
  advanced: boolean;
  reason: string;
}

export interface HookTraceEntry {
  sequence: number;
  timestamp_ms: number;
  revision: number;
  trigger: string;
  decision: string;
  tool_name?: string | null;
  tool_call_id?: string | null;
  subagent_type?: string | null;
  matched_rule_keys: string[];
  refresh_snapshot: boolean;
  block_reason?: string | null;
  completion?: HookCompletionStatus | null;
  diagnostics: HookDiagnosticEntry[];
}

export interface HookPendingAction {
  id: string;
  created_at_ms: number;
  title: string;
  summary: string;
  action_type: string;
  turn_id?: string | null;
  source_trigger: string;
  status: "pending" | "resolved";
  last_injected_at_ms?: number | null;
  resolved_at_ms?: number | null;
  resolution_kind?: "adopted" | "dismissed" | null;
  resolution_note?: string | null;
  resolution_turn_id?: string | null;
  injections: HookInjection[];
}

export type SessionExecutionStatus = "idle" | "running" | "completed" | "failed" | "interrupted";

export interface SessionExecutionState {
  session_id: string;
  status: SessionExecutionStatus;
  turn_id?: string | null;
  message?: string | null;
}

export interface ActiveWorkflowHookMetadata {
  lifecycle_id: string;
  lifecycle_key: string;
  lifecycle_name: string;
  run_id: string;
  run_status: string;
  step_key: string;
  step_title: string;
  primary_workflow_id: string;
  /** Bound workflow key when step is workflow-driven; omit or null for manual steps. */
  workflow_key?: string | null;
  /** @deprecated Prefer workflow_key; may still appear from older API payloads. */
  primary_workflow_key?: string | null;
  primary_workflow_name: string;
}

export interface HookRuntimeMetadata {
  active_workflow?: ActiveWorkflowHookMetadata | null;
}

export interface SessionHookSnapshot {
  session_id: string;
  owners: HookOwnerSummary[];
  sources: string[];
  tags: string[];
  injections: HookInjection[];
  diagnostics: HookDiagnosticEntry[];
  metadata?: HookRuntimeMetadata | null;
}

export interface HookSessionRuntimeInfo {
  session_id: string;
  revision: number;
  snapshot: SessionHookSnapshot;
  diagnostics: HookDiagnosticEntry[];
  trace: HookTraceEntry[];
  pending_actions: HookPendingAction[];
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
  project_id: string;
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

// ─── 项目活跃会话条目 ──────────────────────────────────
// 对应后端 GET /api/projects/{id}/sessions 响应体中的单条记录

export interface ProjectSessionEntry {
  session_id: string;
  session_title: string | null;
  last_activity: number | null;
  execution_status: "idle" | "running" | "completed" | "failed" | "interrupted";
  owner_type: "project" | "story" | "task";
  owner_id: string;
  owner_title: string | null;
  story_id: string | null;
  story_title: string | null;
  agent_key: string | null;
  agent_display_name: string | null;
  parent_session_id: string | null;
}
