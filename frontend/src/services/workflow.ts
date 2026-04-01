import { api } from "../api/client";
import type {
  HookRulePreset,
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
  WorkflowHookRuleSpec,
  WorkflowHookTrigger,
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
const WORKFLOW_HOOK_TRIGGERS = new Set<string>([
  "before_tool", "after_tool", "after_turn", "before_stop", "session_terminal",
  "before_subagent_dispatch", "after_subagent_dispatch", "subagent_result",
]);
const LIFECYCLE_EXECUTION_EVENT_KINDS = new Set<string>([
  "step_activated", "step_completed", "constraint_blocked",
  "completion_evaluated", "artifact_appended", "context_injected",
]);

function normalizeEnum<T extends string>(value: unknown, allowed: Set<string>, field: string): T {
  const s = typeof value === "string" ? value : String(value ?? "");
  if (!allowed.has(s)) {
    throw new Error(`未知的 ${field}: ${s}`);
  }
  return s as T;
}

function requireStringField(raw: Record<string, unknown>, field: string): string {
  const value = raw[field];
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`缺少或非法的字段 ${field}`);
  }
  return value;
}

function mapValidationIssue(raw: Record<string, unknown>) {
  return {
    code: requireStringField(raw, "code"),
    message: requireStringField(raw, "message"),
    field_path: requireStringField(raw, "field_path"),
    severity: raw.severity === "warning" ? "warning" as const : "error" as const,
  };
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
    locator: requireStringField(raw, "locator"),
    reason: requireStringField(raw, "reason"),
    required: raw.required !== false,
    title: optString(raw.title),
  };
}

function mapWorkflowConstraintSpec(raw: Record<string, unknown>): WorkflowConstraintSpec {
  return {
    key: requireStringField(raw, "key"),
    kind: normalizeEnum<WorkflowConstraintKind>(raw.kind, WORKFLOW_CONSTRAINT_KINDS, "workflow constraint kind"),
    description: requireStringField(raw, "description"),
    payload: asRecord(raw.payload),
  };
}

function mapWorkflowCheckSpec(raw: Record<string, unknown>): WorkflowCheckSpec {
  return {
    key: requireStringField(raw, "key"),
    kind: normalizeEnum<WorkflowCheckKind>(raw.kind, WORKFLOW_CHECK_KINDS, "workflow check kind"),
    description: requireStringField(raw, "description"),
    payload: asRecord(raw.payload),
  };
}

function mapWorkflowInjectionSpec(raw: unknown): WorkflowInjectionSpec {
  const value = asRecord(raw);
  if (!value) {
    throw new Error("workflow contract 缺少 injection");
  }
  return {
    goal: optString(value.goal),
    instructions: asStringArray(value.instructions),
    context_bindings: asRecordArray(value.context_bindings).map(mapWorkflowContextBinding),
  };
}

function mapWorkflowCompletionSpec(raw: unknown): WorkflowCompletionSpec {
  const value = asRecord(raw);
  if (!value) {
    throw new Error("workflow contract 缺少 completion");
  }
  return {
    checks: asRecordArray(value.checks).map(mapWorkflowCheckSpec),
    default_artifact_type: value.default_artifact_type != null
      ? normalizeEnum<WorkflowRecordArtifactType>(value.default_artifact_type, WORKFLOW_ARTIFACT_TYPES, "workflow default artifact type")
      : null,
    default_artifact_title: optString(value.default_artifact_title),
  };
}

function mapWorkflowHookRuleSpec(raw: Record<string, unknown>): WorkflowHookRuleSpec {
  return {
    key: requireStringField(raw, "key"),
    trigger: normalizeEnum<WorkflowHookTrigger>(raw.trigger, WORKFLOW_HOOK_TRIGGERS, "workflow hook trigger"),
    description: requireStringField(raw, "description"),
    preset: optString(raw.preset),
    params: asRecord(raw.params),
    script: optString(raw.script),
    enabled: raw.enabled !== false,
  };
}

function mapWorkflowContract(raw: unknown): WorkflowContract {
  const value = asRecord(raw);
  if (!value) {
    throw new Error("workflow contract 缺失或不是对象");
  }
  return {
    injection: mapWorkflowInjectionSpec(value.injection),
    hook_rules: asRecordArray(value.hook_rules).map(mapWorkflowHookRuleSpec),
    constraints: asRecordArray(value.constraints).map(mapWorkflowConstraintSpec),
    completion: mapWorkflowCompletionSpec(value.completion),
  };
}

function mapWorkflowTemplateWorkflow(raw: Record<string, unknown>): WorkflowTemplateWorkflow {
  return {
    key: requireStringField(raw, "key"),
    name: requireStringField(raw, "name"),
    description: requireStringField(raw, "description"),
    contract: mapWorkflowContract(raw.contract),
  };
}

function mapLifecycleStepDefinition(raw: unknown): LifecycleStepDefinition {
  const value = asRecord(raw);
  if (!value) {
    throw new Error("lifecycle step 缺失或不是对象");
  }
  const workflowKeyRaw = value.workflow_key ?? value.primary_workflow_key;
  return {
    key: requireStringField(value, "key"),
    description: requireStringField(value, "description"),
    workflow_key: typeof workflowKeyRaw === "string" && workflowKeyRaw ? workflowKeyRaw : null,
  };
}

function mapWorkflowStepState(raw: Record<string, unknown>): WorkflowStepState {
  return {
    step_key: requireStringField(raw, "step_key"),
    status: normalizeEnum<WorkflowStepExecutionStatus>(raw.status, WORKFLOW_STEP_STATUSES, "workflow step status"),
    started_at: optString(raw.started_at),
    completed_at: optString(raw.completed_at),
    summary: optString(raw.summary),
    context_snapshot: asRecord(raw.context_snapshot),
  };
}

function mapWorkflowRecordArtifact(raw: Record<string, unknown>): WorkflowRecordArtifact {
  return {
    id: requireStringField(raw, "id"),
    step_key: requireStringField(raw, "step_key"),
    artifact_type: normalizeEnum<WorkflowRecordArtifactType>(raw.artifact_type, WORKFLOW_ARTIFACT_TYPES, "workflow artifact type"),
    title: requireStringField(raw, "title"),
    content: requireStringField(raw, "content"),
    created_at: requireStringField(raw, "created_at"),
  };
}

function mapLifecycleExecutionEntry(raw: Record<string, unknown>): LifecycleExecutionEntry {
  return {
    timestamp: requireStringField(raw, "timestamp"),
    step_key: requireStringField(raw, "step_key"),
    event_kind: normalizeEnum<LifecycleExecutionEventKind>(raw.event_kind, LIFECYCLE_EXECUTION_EVENT_KINDS, "lifecycle execution event kind"),
    summary: requireStringField(raw, "summary"),
    detail: asRecord(raw.detail),
  };
}

// ─── Entity mapper（后端 binding_kind → 前端 target_kind 翻译层）───

export function mapWorkflowDefinition(raw: Record<string, unknown>): WorkflowDefinition {
  return {
    id: requireStringField(raw, "id"),
    key: requireStringField(raw, "key"),
    name: requireStringField(raw, "name"),
    description: requireStringField(raw, "description"),
    target_kind: normalizeEnum<WorkflowTargetKind>(raw.binding_kind ?? raw.target_kind, WORKFLOW_TARGET_KINDS, "workflow target kind"),
    recommended_roles: asStringArray(raw.recommended_binding_roles ?? raw.recommended_roles)
      .map((v) => normalizeEnum<WorkflowAgentRole>(v, WORKFLOW_AGENT_ROLES, "workflow agent role")),
    source: normalizeEnum<WorkflowDefinitionSource>(raw.source, WORKFLOW_DEF_SOURCES, "workflow definition source"),
    status: normalizeEnum<WorkflowDefinitionStatus>(raw.status, WORKFLOW_DEF_STATUSES, "workflow definition status"),
    version: Number.isFinite(Number(raw.version)) ? Number(raw.version) : 1,
    contract: mapWorkflowContract(raw.contract),
    created_at: requireStringField(raw, "created_at"),
    updated_at: requireStringField(raw, "updated_at"),
  };
}

export function mapLifecycleDefinition(raw: Record<string, unknown>): LifecycleDefinition {
  return {
    id: requireStringField(raw, "id"),
    key: requireStringField(raw, "key"),
    name: requireStringField(raw, "name"),
    description: requireStringField(raw, "description"),
    target_kind: normalizeEnum<WorkflowTargetKind>(raw.binding_kind ?? raw.target_kind, WORKFLOW_TARGET_KINDS, "lifecycle target kind"),
    recommended_roles: asStringArray(raw.recommended_binding_roles ?? raw.recommended_roles)
      .map((v) => normalizeEnum<WorkflowAgentRole>(v, WORKFLOW_AGENT_ROLES, "lifecycle agent role")),
    source: normalizeEnum<WorkflowDefinitionSource>(raw.source, WORKFLOW_DEF_SOURCES, "lifecycle definition source"),
    status: normalizeEnum<WorkflowDefinitionStatus>(raw.status, WORKFLOW_DEF_STATUSES, "lifecycle definition status"),
    version: Number.isFinite(Number(raw.version)) ? Number(raw.version) : 1,
    entry_step_key: requireStringField(raw, "entry_step_key"),
    steps: Array.isArray(raw.steps) ? raw.steps.map(mapLifecycleStepDefinition) : [],
    created_at: requireStringField(raw, "created_at"),
    updated_at: requireStringField(raw, "updated_at"),
  };
}

export function mapWorkflowTemplate(raw: Record<string, unknown>): WorkflowTemplate {
  const lifecycleRaw = asRecord(raw.lifecycle);
  if (!lifecycleRaw) {
    throw new Error("workflow template 缺少 lifecycle");
  }
  return {
    key: requireStringField(raw, "key"),
    name: requireStringField(raw, "name"),
    description: requireStringField(raw, "description"),
    target_kind: normalizeEnum<WorkflowTargetKind>(raw.binding_kind ?? raw.target_kind, WORKFLOW_TARGET_KINDS, "workflow template target kind"),
    recommended_roles: asStringArray(raw.recommended_binding_roles ?? raw.recommended_roles)
      .map((v) => normalizeEnum<WorkflowAgentRole>(v, WORKFLOW_AGENT_ROLES, "workflow template agent role")),
    workflows: asRecordArray(raw.workflows).map(mapWorkflowTemplateWorkflow),
    lifecycle: {
      key: requireStringField(lifecycleRaw, "key"),
      name: requireStringField(lifecycleRaw, "name"),
      description: requireStringField(lifecycleRaw, "description"),
      entry_step_key: requireStringField(lifecycleRaw, "entry_step_key"),
      steps: Array.isArray(lifecycleRaw.steps)
        ? lifecycleRaw.steps.map(mapLifecycleStepDefinition)
        : [],
    },
  };
}

export function mapWorkflowAssignment(raw: Record<string, unknown>): WorkflowAssignment {
  return {
    id: requireStringField(raw, "id"),
    project_id: requireStringField(raw, "project_id"),
    lifecycle_id: requireStringField(raw, "lifecycle_id"),
    role: normalizeEnum<WorkflowAgentRole>(raw.role, WORKFLOW_AGENT_ROLES, "workflow assignment role"),
    enabled: raw.enabled !== false,
    is_default: Boolean(raw.is_default),
    created_at: requireStringField(raw, "created_at"),
    updated_at: requireStringField(raw, "updated_at"),
  };
}

export function mapWorkflowRun(raw: Record<string, unknown>): WorkflowRun {
  return {
    id: requireStringField(raw, "id"),
    project_id: requireStringField(raw, "project_id"),
    lifecycle_id: requireStringField(raw, "lifecycle_id"),
    target_kind: normalizeEnum<WorkflowTargetKind>(raw.binding_kind ?? raw.target_kind, WORKFLOW_TARGET_KINDS, "workflow run target kind"),
    target_id: typeof raw.binding_id === "string" && raw.binding_id.trim()
      ? raw.binding_id
      : requireStringField(raw, "target_id"),
    status: normalizeEnum<WorkflowRunStatus>(raw.status, WORKFLOW_RUN_STATUSES, "workflow run status"),
    current_step_key: optString(raw.current_step_key),
    step_states: asRecordArray(raw.step_states).map(mapWorkflowStepState),
    record_artifacts: asRecordArray(raw.record_artifacts).map(mapWorkflowRecordArtifact),
    execution_log: asRecordArray(raw.execution_log).map(mapLifecycleExecutionEntry),
    created_at: requireStringField(raw, "created_at"),
    updated_at: requireStringField(raw, "updated_at"),
    last_activity_at: requireStringField(raw, "last_activity_at"),
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
      ? raw.issues.map((item, index) => {
          if (!item || typeof item !== "object") {
            throw new Error(`workflow validation issue[${index}] 必须是对象`);
          }
          return mapValidationIssue(item as Record<string, unknown>);
        })
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
      ? raw.issues.map((item, index) => {
          if (!item || typeof item !== "object") {
            throw new Error(`lifecycle validation issue[${index}] 必须是对象`);
          }
          return mapValidationIssue(item as Record<string, unknown>);
        })
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

export async function fetchHookPresets(): Promise<HookRulePreset[]> {
  const raw = await api.get<Record<string, unknown>>("/hook-presets");
  const grouped = asRecord(raw.presets);
  if (!grouped) {
    throw new Error("hook presets 响应缺少 presets");
  }
  return Object.entries(grouped).flatMap(([groupKey, items]) => {
    if (!Array.isArray(items)) {
      throw new Error(`hook presets.${groupKey} 必须是数组`);
    }
    return items.map((item, index) => {
      if (!item || typeof item !== "object") {
        throw new Error(`hook presets.${groupKey}[${index}] 必须是对象`);
      }
      const record = item as Record<string, unknown>;
      return {
        key: requireStringField(record, "key"),
        trigger: normalizeEnum<WorkflowHookTrigger>(record.trigger, WORKFLOW_HOOK_TRIGGERS, "hook preset trigger"),
        label: requireStringField(record, "label"),
        description: requireStringField(record, "description"),
        param_schema: asRecord(record.param_schema),
        script: typeof record.script === "string" ? record.script : undefined,
        source: record.source === "builtin" || record.source === "user_defined" ? record.source : undefined,
      };
    });
  });
}
