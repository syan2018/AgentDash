import { api } from "../api/client";
import type {
  LifecycleDefinition,
  LifecycleExecutionEntry,
  LifecycleExecutionEventKind,
  LifecycleStepDefinition,
  WorkflowAgentRole,
  WorkflowAssignment,
  WorkflowCheckKind,
  WorkflowCheckSpec,
  WorkflowCompletionSpec,
  WorkflowConstraintKind,
  WorkflowConstraintSpec,
  WorkflowContextBinding,
  WorkflowContract,
  WorkflowDefinition,
  WorkflowDefinitionSource,
  WorkflowDefinitionStatus,
  WorkflowInjectionSpec,
  WorkflowRecordArtifact,
  WorkflowRecordArtifactType,
  WorkflowRun,
  WorkflowRunStatus,
  WorkflowStepExecutionStatus,
  WorkflowStepState,
  WorkflowTargetKind,
  WorkflowTemplate,
  WorkflowTemplateWorkflow,
  WorkflowValidationResult,
} from "../types";

// ─── 枚举 normalizer（将后端字符串收窄到前端联合类型）───

const WORKFLOW_TARGET_KINDS = new Set<string>(["project", "story", "task"]);
const WORKFLOW_AGENT_ROLES = new Set<string>(["project", "story", "task"]);
const WORKFLOW_CONSTRAINT_KINDS = new Set<string>(["block_stop_until_checks_pass", "custom"]);
const WORKFLOW_CHECK_KINDS = new Set<string>([
  "artifact_exists", "artifact_count_gte", "session_terminal_in",
  "checklist_evidence_present", "explicit_action_received", "custom",
]);
const WORKFLOW_RUN_STATUSES = new Set<string>([
  "draft", "ready", "running", "blocked", "completed", "failed", "cancelled",
]);
const WORKFLOW_STEP_STATUSES = new Set<string>([
  "pending", "ready", "running", "completed", "failed", "skipped",
]);
const WORKFLOW_ARTIFACT_TYPES = new Set<string>([
  "session_summary", "journal_update", "archive_suggestion", "phase_note",
  "checklist_evidence", "execution_trace", "decision_record", "context_snapshot",
]);
const WORKFLOW_DEF_SOURCES = new Set<string>(["builtin_seed", "user_authored", "cloned"]);
const WORKFLOW_DEF_STATUSES = new Set<string>(["draft", "active", "disabled"]);
const LIFECYCLE_EXECUTION_EVENT_KINDS = new Set<string>([
  "step_activated", "step_completed", "constraint_blocked",
  "completion_evaluated", "artifact_appended", "context_injected",
]);

function normalizeEnum<T extends string>(value: unknown, allowed: Set<string>, fallback: T): T {
  const s = typeof value === "string" ? value : String(value ?? "");
  return (allowed.has(s) ? s : fallback) as T;
}

// ─── 子结构 mapper ──────────────────────────────────────

function asRecord(raw: unknown): Record<string, unknown> | null {
  return raw && typeof raw === "object" ? (raw as Record<string, unknown>) : null;
}

function asRecordArray(raw: unknown): Record<string, unknown>[] {
  return Array.isArray(raw)
    ? raw.filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
    : [];
}

function asStringArray(raw: unknown): string[] {
  return Array.isArray(raw) ? raw.filter((item): item is string => typeof item === "string") : [];
}

function optString(raw: unknown): string | null {
  return raw != null ? String(raw) : null;
}

function mapWorkflowContextBinding(raw: Record<string, unknown>): WorkflowContextBinding {
  return {
    locator: String(raw.locator ?? ""),
    reason: String(raw.reason ?? ""),
    required: raw.required !== false,
    title: optString(raw.title),
  };
}

function mapWorkflowConstraintSpec(raw: Record<string, unknown>): WorkflowConstraintSpec {
  return {
    key: String(raw.key ?? ""),
    kind: normalizeEnum<WorkflowConstraintKind>(raw.kind, WORKFLOW_CONSTRAINT_KINDS, "custom"),
    description: String(raw.description ?? ""),
    payload: asRecord(raw.payload),
  };
}

function mapWorkflowCheckSpec(raw: Record<string, unknown>): WorkflowCheckSpec {
  return {
    key: String(raw.key ?? ""),
    kind: normalizeEnum<WorkflowCheckKind>(raw.kind, WORKFLOW_CHECK_KINDS, "custom"),
    description: String(raw.description ?? ""),
    payload: asRecord(raw.payload),
  };
}

function mapWorkflowInjectionSpec(raw: unknown): WorkflowInjectionSpec {
  const value = asRecord(raw);
  if (!value) return { goal: null, instructions: [], context_bindings: [] };
  return {
    goal: optString(value.goal),
    instructions: asStringArray(value.instructions),
    context_bindings: asRecordArray(value.context_bindings).map(mapWorkflowContextBinding),
  };
}

function mapWorkflowCompletionSpec(raw: unknown): WorkflowCompletionSpec {
  const value = asRecord(raw);
  if (!value) return { checks: [], default_artifact_type: null, default_artifact_title: null };
  return {
    checks: asRecordArray(value.checks).map(mapWorkflowCheckSpec),
    default_artifact_type: value.default_artifact_type != null
      ? normalizeEnum<WorkflowRecordArtifactType>(value.default_artifact_type, WORKFLOW_ARTIFACT_TYPES, "phase_note")
      : null,
    default_artifact_title: optString(value.default_artifact_title),
  };
}

function mapWorkflowContract(raw: unknown): WorkflowContract {
  const value = asRecord(raw);
  if (!value) {
    return { injection: mapWorkflowInjectionSpec(null), constraints: [], completion: mapWorkflowCompletionSpec(null) };
  }
  return {
    injection: mapWorkflowInjectionSpec(value.injection),
    constraints: asRecordArray(value.constraints).map(mapWorkflowConstraintSpec),
    completion: mapWorkflowCompletionSpec(value.completion),
  };
}

function mapWorkflowTemplateWorkflow(raw: Record<string, unknown>): WorkflowTemplateWorkflow {
  return {
    key: String(raw.key ?? ""),
    name: String(raw.name ?? "未命名 Workflow"),
    description: String(raw.description ?? ""),
    contract: mapWorkflowContract(raw.contract),
  };
}

function mapLifecycleStepDefinition(raw: unknown): LifecycleStepDefinition {
  const value = asRecord(raw);
  if (!value) return { key: "", description: "", workflow_key: null };
  const workflowKeyRaw = value.workflow_key ?? value.primary_workflow_key;
  return {
    key: typeof value.key === "string" ? value.key : "",
    description: typeof value.description === "string" ? value.description : "",
    workflow_key: typeof workflowKeyRaw === "string" && workflowKeyRaw ? workflowKeyRaw : null,
  };
}

function mapWorkflowStepState(raw: Record<string, unknown>): WorkflowStepState {
  return {
    step_key: String(raw.step_key ?? ""),
    status: normalizeEnum<WorkflowStepExecutionStatus>(raw.status, WORKFLOW_STEP_STATUSES, "pending"),
    started_at: optString(raw.started_at),
    completed_at: optString(raw.completed_at),
    summary: optString(raw.summary),
    context_snapshot: asRecord(raw.context_snapshot),
  };
}

function mapWorkflowRecordArtifact(raw: Record<string, unknown>): WorkflowRecordArtifact {
  return {
    id: String(raw.id ?? ""),
    step_key: String(raw.step_key ?? ""),
    artifact_type: normalizeEnum<WorkflowRecordArtifactType>(raw.artifact_type, WORKFLOW_ARTIFACT_TYPES, "phase_note"),
    title: String(raw.title ?? ""),
    content: String(raw.content ?? ""),
    created_at: String(raw.created_at ?? new Date().toISOString()),
  };
}

function mapLifecycleExecutionEntry(raw: Record<string, unknown>): LifecycleExecutionEntry {
  return {
    timestamp: String(raw.timestamp ?? new Date().toISOString()),
    step_key: String(raw.step_key ?? ""),
    event_kind: normalizeEnum<LifecycleExecutionEventKind>(raw.event_kind, LIFECYCLE_EXECUTION_EVENT_KINDS, "step_activated"),
    summary: String(raw.summary ?? ""),
    detail: asRecord(raw.detail),
  };
}

// ─── Entity mapper（后端 binding_kind → 前端 target_kind 翻译层）───

export function mapWorkflowDefinition(raw: Record<string, unknown>): WorkflowDefinition {
  return {
    id: String(raw.id ?? ""),
    key: String(raw.key ?? ""),
    name: String(raw.name ?? "未命名 Workflow"),
    description: String(raw.description ?? ""),
    target_kind: normalizeEnum<WorkflowTargetKind>(raw.binding_kind ?? raw.target_kind, WORKFLOW_TARGET_KINDS, "task"),
    recommended_roles: asStringArray(raw.recommended_binding_roles ?? raw.recommended_roles)
      .map((v) => normalizeEnum<WorkflowAgentRole>(v, WORKFLOW_AGENT_ROLES, "task")),
    source: normalizeEnum<WorkflowDefinitionSource>(raw.source, WORKFLOW_DEF_SOURCES, "user_authored"),
    status: normalizeEnum<WorkflowDefinitionStatus>(raw.status, WORKFLOW_DEF_STATUSES, "draft"),
    version: Number.isFinite(Number(raw.version)) ? Number(raw.version) : 1,
    contract: mapWorkflowContract(raw.contract),
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

export function mapLifecycleDefinition(raw: Record<string, unknown>): LifecycleDefinition {
  return {
    id: String(raw.id ?? ""),
    key: String(raw.key ?? ""),
    name: String(raw.name ?? "未命名 Lifecycle"),
    description: String(raw.description ?? ""),
    target_kind: normalizeEnum<WorkflowTargetKind>(raw.binding_kind ?? raw.target_kind, WORKFLOW_TARGET_KINDS, "task"),
    recommended_roles: asStringArray(raw.recommended_binding_roles ?? raw.recommended_roles)
      .map((v) => normalizeEnum<WorkflowAgentRole>(v, WORKFLOW_AGENT_ROLES, "task")),
    source: normalizeEnum<WorkflowDefinitionSource>(raw.source, WORKFLOW_DEF_SOURCES, "user_authored"),
    status: normalizeEnum<WorkflowDefinitionStatus>(raw.status, WORKFLOW_DEF_STATUSES, "draft"),
    version: Number.isFinite(Number(raw.version)) ? Number(raw.version) : 1,
    entry_step_key: String(raw.entry_step_key ?? ""),
    steps: Array.isArray(raw.steps) ? raw.steps.map(mapLifecycleStepDefinition) : [],
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

export function mapWorkflowTemplate(raw: Record<string, unknown>): WorkflowTemplate {
  const lifecycleRaw = asRecord(raw.lifecycle) ?? {};
  return {
    key: String(raw.key ?? ""),
    name: String(raw.name ?? "未命名 Workflow Template"),
    description: String(raw.description ?? ""),
    target_kind: normalizeEnum<WorkflowTargetKind>(raw.binding_kind ?? raw.target_kind, WORKFLOW_TARGET_KINDS, "task"),
    recommended_roles: asStringArray(raw.recommended_binding_roles ?? raw.recommended_roles)
      .map((v) => normalizeEnum<WorkflowAgentRole>(v, WORKFLOW_AGENT_ROLES, "task")),
    workflows: asRecordArray(raw.workflows).map(mapWorkflowTemplateWorkflow),
    lifecycle: {
      key: String(lifecycleRaw.key ?? ""),
      name: String(lifecycleRaw.name ?? "未命名 Lifecycle"),
      description: String(lifecycleRaw.description ?? ""),
      entry_step_key: String(lifecycleRaw.entry_step_key ?? ""),
      steps: Array.isArray(lifecycleRaw.steps)
        ? lifecycleRaw.steps.map(mapLifecycleStepDefinition)
        : [],
    },
  };
}

export function mapWorkflowAssignment(raw: Record<string, unknown>): WorkflowAssignment {
  return {
    id: String(raw.id ?? ""),
    project_id: String(raw.project_id ?? ""),
    lifecycle_id: String(raw.lifecycle_id ?? ""),
    role: normalizeEnum<WorkflowAgentRole>(raw.role, WORKFLOW_AGENT_ROLES, "task"),
    enabled: raw.enabled !== false,
    is_default: Boolean(raw.is_default),
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

export function mapWorkflowRun(raw: Record<string, unknown>): WorkflowRun {
  return {
    id: String(raw.id ?? ""),
    project_id: String(raw.project_id ?? ""),
    lifecycle_id: String(raw.lifecycle_id ?? ""),
    target_kind: normalizeEnum<WorkflowTargetKind>(raw.binding_kind ?? raw.target_kind, WORKFLOW_TARGET_KINDS, "task"),
    target_id: String(raw.binding_id ?? raw.target_id ?? ""),
    status: normalizeEnum<WorkflowRunStatus>(raw.status, WORKFLOW_RUN_STATUSES, "draft"),
    current_step_key: optString(raw.current_step_key),
    step_states: asRecordArray(raw.step_states).map(mapWorkflowStepState),
    record_artifacts: asRecordArray(raw.record_artifacts).map(mapWorkflowRecordArtifact),
    execution_log: asRecordArray(raw.execution_log).map(mapLifecycleExecutionEntry),
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
    last_activity_at: String(raw.last_activity_at ?? new Date().toISOString()),
  };
}

export async function fetchWorkflowDefinitions(targetKind?: WorkflowTargetKind): Promise<WorkflowDefinition[]> {
  const query = targetKind ? `?binding_kind=${targetKind}` : "";
  const raw = await api.get<Record<string, unknown>[]>(`/workflow-definitions${query}`);
  return raw.map(mapWorkflowDefinition);
}

export async function fetchLifecycleDefinitions(targetKind?: WorkflowTargetKind): Promise<LifecycleDefinition[]> {
  const query = targetKind ? `?binding_kind=${targetKind}` : "";
  const raw = await api.get<Record<string, unknown>[]>(`/lifecycle-definitions${query}`);
  return raw.map(mapLifecycleDefinition);
}

export async function createLifecycleDefinition(input: {
  key: string;
  name: string;
  description?: string;
  target_kind: WorkflowTargetKind;
  recommended_roles?: WorkflowAgentRole[];
  entry_step_key: string;
  steps: LifecycleStepDefinition[];
}): Promise<LifecycleDefinition> {
  const raw = await api.post<Record<string, unknown>>("/lifecycle-definitions", {
    key: input.key,
    name: input.name,
    description: input.description,
    binding_kind: input.target_kind,
    recommended_binding_roles: input.recommended_roles,
    entry_step_key: input.entry_step_key,
    steps: input.steps,
  });
  return mapLifecycleDefinition(raw);
}

export async function getLifecycleDefinition(id: string): Promise<LifecycleDefinition> {
  const raw = await api.get<Record<string, unknown>>(`/lifecycle-definitions/${id}`);
  return mapLifecycleDefinition(raw);
}

export async function updateLifecycleDefinition(
  id: string,
  input: {
    name?: string;
    description?: string;
    recommended_roles?: WorkflowAgentRole[];
    entry_step_key?: string;
    steps?: LifecycleStepDefinition[];
  },
): Promise<LifecycleDefinition> {
  const raw = await api.put<Record<string, unknown>>(`/lifecycle-definitions/${id}`, {
    name: input.name,
    description: input.description,
    recommended_binding_roles: input.recommended_roles,
    entry_step_key: input.entry_step_key,
    steps: input.steps,
  });
  return mapLifecycleDefinition(raw);
}

export async function validateLifecycleDefinition(input: {
  key: string;
  name: string;
  description?: string;
  target_kind: WorkflowTargetKind;
  recommended_roles?: WorkflowAgentRole[];
  entry_step_key: string;
  steps: LifecycleStepDefinition[];
}): Promise<WorkflowValidationResult> {
  const raw = await api.post<Record<string, unknown>>("/lifecycle-definitions/validate", {
    key: input.key,
    name: input.name,
    description: input.description,
    binding_kind: input.target_kind,
    recommended_binding_roles: input.recommended_roles,
    entry_step_key: input.entry_step_key,
    steps: input.steps,
  });
  return {
    valid: Boolean(raw.valid),
    issues: Array.isArray(raw.issues)
      ? raw.issues.map((item: Record<string, unknown>) => ({
          code: String(item.code ?? ""),
          message: String(item.message ?? ""),
          field_path: String(item.field_path ?? ""),
          severity: item.severity === "warning" ? "warning" as const : "error" as const,
        }))
      : [],
  };
}

export async function enableLifecycleDefinition(id: string): Promise<LifecycleDefinition> {
  const raw = await api.post<Record<string, unknown>>(`/lifecycle-definitions/${id}/enable`, {});
  return mapLifecycleDefinition(raw);
}

export async function disableLifecycleDefinition(id: string): Promise<LifecycleDefinition> {
  const raw = await api.post<Record<string, unknown>>(`/lifecycle-definitions/${id}/disable`, {});
  return mapLifecycleDefinition(raw);
}

export async function deleteLifecycleDefinition(id: string): Promise<void> {
  await api.delete(`/lifecycle-definitions/${id}`);
}

export async function fetchWorkflowTemplates(): Promise<WorkflowTemplate[]> {
  const raw = await api.get<Record<string, unknown>[]>("/workflow-templates");
  return raw.map(mapWorkflowTemplate);
}

export async function bootstrapWorkflowTemplate(builtinKey: string): Promise<LifecycleDefinition> {
  const raw = await api.post<Record<string, unknown>>(
    `/workflow-templates/${encodeURIComponent(builtinKey)}/bootstrap`,
    {},
  );
  return mapLifecycleDefinition(raw);
}

export async function fetchProjectWorkflowAssignments(projectId: string): Promise<WorkflowAssignment[]> {
  const raw = await api.get<Record<string, unknown>[]>(`/projects/${projectId}/workflow-assignments`);
  return raw.map(mapWorkflowAssignment);
}

export async function assignProjectLifecycle(input: {
  project_id: string;
  lifecycle_id: string;
  role: WorkflowAgentRole;
  enabled?: boolean;
  is_default?: boolean;
}): Promise<WorkflowAssignment> {
  const raw = await api.post<Record<string, unknown>>(
    `/projects/${input.project_id}/workflow-assignments`,
    {
      lifecycle_id: input.lifecycle_id,
      role: input.role,
      enabled: input.enabled ?? true,
      is_default: input.is_default ?? false,
    },
  );
  return mapWorkflowAssignment(raw);
}

export async function fetchWorkflowRunsByTarget(
  targetKind: WorkflowTargetKind,
  targetId: string,
): Promise<WorkflowRun[]> {
  const raw = await api.get<Record<string, unknown>[]>(
    `/lifecycle-runs/bindings/${targetKind}/${targetId}`,
  );
  return raw.map(mapWorkflowRun);
}

export async function startWorkflowRun(input: {
  lifecycle_id?: string;
  lifecycle_key?: string;
  target_kind: WorkflowTargetKind;
  target_id: string;
}): Promise<WorkflowRun> {
  const raw = await api.post<Record<string, unknown>>("/lifecycle-runs", {
    lifecycle_id: input.lifecycle_id,
    lifecycle_key: input.lifecycle_key,
    binding_kind: input.target_kind,
    binding_id: input.target_id,
  });
  return mapWorkflowRun(raw);
}

export async function activateWorkflowStep(input: {
  run_id: string;
  step_key: string;
}): Promise<WorkflowRun> {
  const raw = await api.post<Record<string, unknown>>(
    `/lifecycle-runs/${input.run_id}/steps/${encodeURIComponent(input.step_key)}/activate`,
    {},
  );
  return mapWorkflowRun(raw);
}

export async function completeWorkflowStep(input: {
  run_id: string;
  step_key: string;
  summary?: string;
  record_artifacts?: Array<{
    artifact_type: WorkflowRecordArtifactType;
    title: string;
    content: string;
  }>;
}): Promise<WorkflowRun> {
  const raw = await api.post<Record<string, unknown>>(
    `/lifecycle-runs/${input.run_id}/steps/${encodeURIComponent(input.step_key)}/complete`,
    {
      summary: input.summary,
      record_artifacts: input.record_artifacts ?? [],
    },
  );
  return mapWorkflowRun(raw);
}

export async function createWorkflowDefinition(input: {
  key: string;
  name: string;
  description?: string;
  target_kind: WorkflowTargetKind;
  recommended_roles?: WorkflowAgentRole[];
  contract: WorkflowContract;
}): Promise<WorkflowDefinition> {
  const raw = await api.post<Record<string, unknown>>("/workflow-definitions", {
    key: input.key,
    name: input.name,
    description: input.description,
    binding_kind: input.target_kind,
    recommended_binding_roles: input.recommended_roles,
    contract: input.contract,
  });
  return mapWorkflowDefinition(raw);
}

export async function getWorkflowDefinition(id: string): Promise<WorkflowDefinition> {
  const raw = await api.get<Record<string, unknown>>(`/workflow-definitions/${id}`);
  return mapWorkflowDefinition(raw);
}

export async function updateWorkflowDefinition(
  id: string,
  input: {
    name?: string;
    description?: string;
    recommended_roles?: WorkflowAgentRole[];
    contract?: WorkflowContract;
  },
): Promise<WorkflowDefinition> {
  const raw = await api.put<Record<string, unknown>>(`/workflow-definitions/${id}`, {
    name: input.name,
    description: input.description,
    recommended_binding_roles: input.recommended_roles,
    contract: input.contract,
  });
  return mapWorkflowDefinition(raw);
}

export async function validateWorkflowDefinition(input: {
  key: string;
  name: string;
  description?: string;
  target_kind: WorkflowTargetKind;
  recommended_roles?: WorkflowAgentRole[];
  contract: WorkflowContract;
}): Promise<WorkflowValidationResult> {
  const raw = await api.post<Record<string, unknown>>("/workflow-definitions/validate", {
    key: input.key,
    name: input.name,
    description: input.description,
    binding_kind: input.target_kind,
    recommended_binding_roles: input.recommended_roles,
    contract: input.contract,
  });
  return {
    valid: Boolean(raw.valid),
    issues: Array.isArray(raw.issues)
      ? raw.issues.map((item: Record<string, unknown>) => ({
          code: String(item.code ?? ""),
          message: String(item.message ?? ""),
          field_path: String(item.field_path ?? ""),
          severity: item.severity === "warning" ? "warning" as const : "error" as const,
        }))
      : [],
  };
}

export async function enableWorkflowDefinition(id: string): Promise<WorkflowDefinition> {
  const raw = await api.post<Record<string, unknown>>(`/workflow-definitions/${id}/enable`, {});
  return mapWorkflowDefinition(raw);
}

export async function disableWorkflowDefinition(id: string): Promise<WorkflowDefinition> {
  const raw = await api.post<Record<string, unknown>>(`/workflow-definitions/${id}/disable`, {});
  return mapWorkflowDefinition(raw);
}

export async function deleteWorkflowDefinition(id: string): Promise<void> {
  await api.delete(`/workflow-definitions/${id}`);
}
