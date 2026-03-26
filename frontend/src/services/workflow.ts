import { api } from "../api/client";
import type {
  BindingKindMetadata,
  LifecycleDefinition,
  LifecycleFailureAction,
  LifecycleStepDefinition,
  LifecycleTransitionPolicyKind,
  WorkflowAgentRole,
  WorkflowAssignment,
  WorkflowCheckKind,
  WorkflowCheckSpec,
  WorkflowCompletionSpec,
  WorkflowConstraintKind,
  WorkflowConstraintSpec,
  WorkflowContextBinding,
  WorkflowContextBindingKind,
  WorkflowContract,
  WorkflowDefinition,
  WorkflowDefinitionSource,
  WorkflowDefinitionStatus,
  WorkflowInjectionSpec,
  WorkflowRecordArtifact,
  WorkflowRecordArtifactType,
  WorkflowRun,
  WorkflowRunStatus,
  WorkflowSessionBinding,
  WorkflowSessionTerminalState,
  WorkflowStepExecutionStatus,
  WorkflowStepState,
  WorkflowTargetKind,
  WorkflowTemplate,
  WorkflowTemplateWorkflow,
  WorkflowValidationResult,
} from "../types";

function normalizeWorkflowTargetKind(value: string): WorkflowTargetKind {
  return value === "project" || value === "story" || value === "task" ? value : "task";
}

function normalizeWorkflowAgentRole(value: string): WorkflowAgentRole {
  switch (value) {
    case "project":
    case "story":
    case "task":
      return value;
    default:
      return "task";
  }
}

function normalizeWorkflowContextBindingKind(value: string): WorkflowContextBindingKind {
  switch (value) {
    case "document_path":
    case "runtime_context":
    case "checklist":
    case "journal_target":
    case "action_ref":
    case "artifact_ref":
      return value;
    default:
      return "document_path";
  }
}

function normalizeWorkflowSessionBinding(value: string): WorkflowSessionBinding {
  switch (value) {
    case "not_required":
    case "optional":
    case "required":
      return value;
    default:
      return "not_required";
  }
}

function normalizeWorkflowConstraintKind(value: string): WorkflowConstraintKind {
  switch (value) {
    case "deny_task_status_transition":
    case "block_stop_until_checks_pass":
    case "custom":
      return value;
    default:
      return "custom";
  }
}

function normalizeWorkflowCheckKind(value: string): WorkflowCheckKind {
  switch (value) {
    case "task_status_in":
    case "artifact_exists":
    case "artifact_count_gte":
    case "session_terminal_in":
    case "checklist_evidence_present":
    case "explicit_action_received":
    case "custom":
      return value;
    default:
      return "custom";
  }
}

function normalizeLifecycleTransitionPolicyKind(value: string): LifecycleTransitionPolicyKind {
  switch (value) {
    case "manual":
    case "all_checks_pass":
    case "any_checks_pass":
    case "session_terminal_matches":
    case "explicit_action":
      return value;
    default:
      return "manual";
  }
}

function normalizeWorkflowRunStatus(value: string): WorkflowRunStatus {
  switch (value) {
    case "draft":
    case "ready":
    case "running":
    case "blocked":
    case "completed":
    case "failed":
    case "cancelled":
      return value;
    default:
      return "draft";
  }
}

function normalizeWorkflowStepExecutionStatus(value: string): WorkflowStepExecutionStatus {
  switch (value) {
    case "pending":
    case "ready":
    case "running":
    case "completed":
    case "failed":
    case "skipped":
      return value;
    default:
      return "pending";
  }
}

function normalizeWorkflowRecordArtifactType(value: string): WorkflowRecordArtifactType {
  switch (value) {
    case "session_summary":
    case "journal_update":
    case "archive_suggestion":
    case "phase_note":
    case "checklist_evidence":
      return value;
    default:
      return "phase_note";
  }
}

function normalizeWorkflowSessionTerminalState(value: string): WorkflowSessionTerminalState {
  switch (value) {
    case "completed":
    case "failed":
    case "interrupted":
      return value;
    default:
      return "completed";
  }
}

function normalizeWorkflowDefinitionSource(value: string): WorkflowDefinitionSource {
  switch (value) {
    case "builtin_seed":
    case "user_authored":
    case "cloned":
      return value;
    default:
      return "user_authored";
  }
}

function normalizeWorkflowDefinitionStatus(value: string): WorkflowDefinitionStatus {
  switch (value) {
    case "draft":
    case "active":
    case "disabled":
      return value;
    default:
      return "draft";
  }
}

function mapWorkflowContextBinding(raw: Record<string, unknown>): WorkflowContextBinding {
  return {
    kind: normalizeWorkflowContextBindingKind(String(raw.kind ?? "document_path")),
    locator: String(raw.locator ?? ""),
    reason: String(raw.reason ?? ""),
    required: raw.required !== false,
    title: raw.title != null ? String(raw.title) : null,
  };
}

function mapWorkflowConstraintSpec(raw: Record<string, unknown>): WorkflowConstraintSpec {
  return {
    key: String(raw.key ?? ""),
    kind: normalizeWorkflowConstraintKind(String(raw.kind ?? "custom")),
    description: String(raw.description ?? ""),
    payload:
      raw.payload && typeof raw.payload === "object"
        ? (raw.payload as Record<string, unknown>)
        : null,
  };
}

function mapWorkflowCheckSpec(raw: Record<string, unknown>): WorkflowCheckSpec {
  return {
    key: String(raw.key ?? ""),
    kind: normalizeWorkflowCheckKind(String(raw.kind ?? "custom")),
    description: String(raw.description ?? ""),
    payload:
      raw.payload && typeof raw.payload === "object"
        ? (raw.payload as Record<string, unknown>)
        : null,
  };
}

function mapWorkflowInjectionSpec(raw: unknown): WorkflowInjectionSpec {
  if (!raw || typeof raw !== "object") {
    return {
      goal: null,
      instructions: [],
      context_bindings: [],
      session_binding: "not_required",
    };
  }
  const value = raw as Record<string, unknown>;
  return {
    goal: value.goal != null ? String(value.goal) : null,
    instructions: Array.isArray(value.instructions)
      ? value.instructions.filter((item): item is string => typeof item === "string")
      : [],
    context_bindings: Array.isArray(value.context_bindings)
      ? value.context_bindings
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapWorkflowContextBinding)
      : [],
    session_binding: normalizeWorkflowSessionBinding(String(value.session_binding ?? "not_required")),
  };
}

function mapWorkflowCompletionSpec(raw: unknown): WorkflowCompletionSpec {
  if (!raw || typeof raw !== "object") {
    return {
      checks: [],
      default_artifact_type: null,
      default_artifact_title: null,
    };
  }
  const value = raw as Record<string, unknown>;
  return {
    checks: Array.isArray(value.checks)
      ? value.checks
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapWorkflowCheckSpec)
      : [],
    default_artifact_type:
      value.default_artifact_type != null
        ? normalizeWorkflowRecordArtifactType(String(value.default_artifact_type))
        : null,
    default_artifact_title:
      value.default_artifact_title != null ? String(value.default_artifact_title) : null,
  };
}

function mapWorkflowContract(raw: unknown): WorkflowContract {
  if (!raw || typeof raw !== "object") {
    return {
      injection: mapWorkflowInjectionSpec(null),
      constraints: [],
      completion: mapWorkflowCompletionSpec(null),
    };
  }
  const value = raw as Record<string, unknown>;
  return {
    injection: mapWorkflowInjectionSpec(value.injection),
    constraints: Array.isArray(value.constraints)
      ? value.constraints
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapWorkflowConstraintSpec)
      : [],
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

function mapLifecycleStepDefinition(raw: Record<string, unknown>): LifecycleStepDefinition {
  const transitionRaw = (raw.transition ?? {}) as Record<string, unknown>;
  const policyRaw = (transitionRaw.policy ?? {}) as Record<string, unknown>;

  return {
    key: String(raw.key ?? ""),
    title: String(raw.title ?? ""),
    description: String(raw.description ?? ""),
    primary_workflow_key: String(raw.primary_workflow_key ?? ""),
    session_binding: normalizeWorkflowSessionBinding(String(raw.session_binding ?? "not_required")),
    transition: {
      policy: {
        kind: normalizeLifecycleTransitionPolicyKind(String(policyRaw.kind ?? "manual")),
        next_step_key: policyRaw.next_step_key != null ? String(policyRaw.next_step_key) : null,
        session_terminal_states: Array.isArray(policyRaw.session_terminal_states)
          ? policyRaw.session_terminal_states.map((item: unknown) =>
              normalizeWorkflowSessionTerminalState(String(item)),
            )
          : [],
        action_key: policyRaw.action_key != null ? String(policyRaw.action_key) : null,
      },
      on_failure: transitionRaw.on_failure != null ? String(transitionRaw.on_failure) as LifecycleFailureAction : null,
    },
  };
}

function mapWorkflowStepState(raw: Record<string, unknown>): WorkflowStepState {
  return {
    step_key: String(raw.step_key ?? ""),
    status: normalizeWorkflowStepExecutionStatus(String(raw.status ?? "pending")),
    session_binding_id: raw.session_binding_id != null ? String(raw.session_binding_id) : null,
    started_at: raw.started_at != null ? String(raw.started_at) : null,
    completed_at: raw.completed_at != null ? String(raw.completed_at) : null,
    summary: raw.summary != null ? String(raw.summary) : null,
    completed_by: raw.completed_by != null ? String(raw.completed_by) as WorkflowStepState["completed_by"] : null,
  };
}

function mapWorkflowRecordArtifact(raw: Record<string, unknown>): WorkflowRecordArtifact {
  return {
    id: String(raw.id ?? ""),
    step_key: String(raw.step_key ?? ""),
    artifact_type: normalizeWorkflowRecordArtifactType(String(raw.artifact_type ?? "phase_note")),
    title: String(raw.title ?? ""),
    content: String(raw.content ?? ""),
    created_at: String(raw.created_at ?? new Date().toISOString()),
  };
}

export function mapWorkflowDefinition(raw: Record<string, unknown>): WorkflowDefinition {
  return {
    id: String(raw.id ?? ""),
    key: String(raw.key ?? ""),
    name: String(raw.name ?? "未命名 Workflow"),
    description: String(raw.description ?? ""),
    target_kind: normalizeWorkflowTargetKind(String(raw.target_kind ?? "task")),
    recommended_roles: Array.isArray(raw.recommended_roles)
      ? raw.recommended_roles
          .filter((item): item is string => typeof item === "string")
          .map(normalizeWorkflowAgentRole)
      : [],
    source: normalizeWorkflowDefinitionSource(String(raw.source ?? "user_authored")),
    status: normalizeWorkflowDefinitionStatus(String(raw.status ?? "draft")),
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
    target_kind: normalizeWorkflowTargetKind(String(raw.target_kind ?? "task")),
    recommended_roles: Array.isArray(raw.recommended_roles)
      ? raw.recommended_roles
          .filter((item): item is string => typeof item === "string")
          .map(normalizeWorkflowAgentRole)
      : [],
    source: normalizeWorkflowDefinitionSource(String(raw.source ?? "user_authored")),
    status: normalizeWorkflowDefinitionStatus(String(raw.status ?? "draft")),
    version: Number.isFinite(Number(raw.version)) ? Number(raw.version) : 1,
    entry_step_key: String(raw.entry_step_key ?? ""),
    steps: Array.isArray(raw.steps)
      ? raw.steps
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapLifecycleStepDefinition)
      : [],
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

export function mapWorkflowTemplate(raw: Record<string, unknown>): WorkflowTemplate {
  const lifecycleRaw =
    raw.lifecycle && typeof raw.lifecycle === "object"
      ? (raw.lifecycle as Record<string, unknown>)
      : {};
  return {
    key: String(raw.key ?? ""),
    name: String(raw.name ?? "未命名 Workflow Template"),
    description: String(raw.description ?? ""),
    target_kind: normalizeWorkflowTargetKind(String(raw.target_kind ?? "task")),
    recommended_roles: Array.isArray(raw.recommended_roles)
      ? raw.recommended_roles
          .filter((item): item is string => typeof item === "string")
          .map(normalizeWorkflowAgentRole)
      : [],
    workflows: Array.isArray(raw.workflows)
      ? raw.workflows
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapWorkflowTemplateWorkflow)
      : [],
    lifecycle: {
      key: String(lifecycleRaw.key ?? ""),
      name: String(lifecycleRaw.name ?? "未命名 Lifecycle"),
      description: String(lifecycleRaw.description ?? ""),
      entry_step_key: String(lifecycleRaw.entry_step_key ?? ""),
      steps: Array.isArray(lifecycleRaw.steps)
        ? lifecycleRaw.steps
            .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
            .map(mapLifecycleStepDefinition)
        : [],
    },
  };
}

export function mapWorkflowAssignment(raw: Record<string, unknown>): WorkflowAssignment {
  return {
    id: String(raw.id ?? ""),
    project_id: String(raw.project_id ?? ""),
    lifecycle_id: String(raw.lifecycle_id ?? ""),
    role: normalizeWorkflowAgentRole(String(raw.role ?? "task")),
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
    target_kind: normalizeWorkflowTargetKind(String(raw.target_kind ?? "task")),
    target_id: String(raw.target_id ?? ""),
    status: normalizeWorkflowRunStatus(String(raw.status ?? "draft")),
    current_step_key: raw.current_step_key != null ? String(raw.current_step_key) : null,
    step_states: Array.isArray(raw.step_states)
      ? raw.step_states
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapWorkflowStepState)
      : [],
    record_artifacts: Array.isArray(raw.record_artifacts)
      ? raw.record_artifacts
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapWorkflowRecordArtifact)
      : [],
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
    last_activity_at: String(raw.last_activity_at ?? new Date().toISOString()),
  };
}

export async function fetchWorkflowDefinitions(targetKind?: WorkflowTargetKind): Promise<WorkflowDefinition[]> {
  const query = targetKind ? `?target_kind=${targetKind}` : "";
  const raw = await api.get<Record<string, unknown>[]>(`/workflow-definitions${query}`);
  return raw.map(mapWorkflowDefinition);
}

export async function fetchLifecycleDefinitions(targetKind?: WorkflowTargetKind): Promise<LifecycleDefinition[]> {
  const query = targetKind ? `?target_kind=${targetKind}` : "";
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
  const raw = await api.post<Record<string, unknown>>("/lifecycle-definitions", input);
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
  const raw = await api.put<Record<string, unknown>>(`/lifecycle-definitions/${id}`, input);
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
  const raw = await api.post<Record<string, unknown>>("/lifecycle-definitions/validate", input);
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
    `/lifecycle-runs/targets/${targetKind}/${targetId}`,
  );
  return raw.map(mapWorkflowRun);
}

export async function startWorkflowRun(input: {
  lifecycle_id?: string;
  lifecycle_key?: string;
  target_kind: WorkflowTargetKind;
  target_id: string;
}): Promise<WorkflowRun> {
  const raw = await api.post<Record<string, unknown>>("/lifecycle-runs", input);
  return mapWorkflowRun(raw);
}

export async function activateWorkflowStep(input: {
  run_id: string;
  step_key: string;
  session_binding_id?: string;
}): Promise<WorkflowRun> {
  const raw = await api.post<Record<string, unknown>>(
    `/lifecycle-runs/${input.run_id}/steps/${encodeURIComponent(input.step_key)}/activate`,
    {
      session_binding_id: input.session_binding_id,
    },
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
  const raw = await api.post<Record<string, unknown>>("/workflow-definitions", input);
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
  const raw = await api.put<Record<string, unknown>>(`/workflow-definitions/${id}`, input);
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
  const raw = await api.post<Record<string, unknown>>("/workflow-definitions/validate", input);
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

export async function fetchBindingMetadata(): Promise<BindingKindMetadata[]> {
  const raw = await api.get<Record<string, unknown>[]>("/workflow-definitions/binding-metadata");
  return raw.map((item) => ({
    kind: normalizeWorkflowContextBindingKind(String(item.kind ?? "document_path")),
    label: String(item.label ?? ""),
    description: String(item.description ?? ""),
    locator_options: Array.isArray(item.locator_options)
      ? item.locator_options.map((opt: Record<string, unknown>) => ({
          locator: String(opt.locator ?? ""),
          label: String(opt.label ?? ""),
          description: String(opt.description ?? ""),
          applicable_target_kinds: Array.isArray(opt.applicable_target_kinds)
            ? opt.applicable_target_kinds.map((tk: string) => normalizeWorkflowTargetKind(tk))
            : [],
        }))
      : [],
  }));
}
