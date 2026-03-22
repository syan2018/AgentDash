import { api } from "../api/client";
import type {
  WorkflowAgentRole,
  WorkflowAssignment,
  WorkflowContextBindingKind,
  WorkflowDefinition,
  WorkflowPhaseCompletionMode,
  WorkflowPhaseDefinition,
  WorkflowPhaseExecutionStatus,
  WorkflowPhaseState,
  WorkflowRecordArtifact,
  WorkflowRecordArtifactType,
  WorkflowRecordPolicy,
  WorkflowTemplate,
  WorkflowRun,
  WorkflowRunStatus,
  WorkflowTargetKind,
} from "../types";

function normalizeWorkflowTargetKind(value: string): WorkflowTargetKind {
  return value === "project" || value === "story" || value === "task" ? value : "task";
}

function normalizeWorkflowAgentRole(value: string): WorkflowAgentRole {
  switch (value) {
    case "project_context_maintainer":
    case "story_lifecycle_companion":
    case "task_execution_worker":
    case "review_agent":
    case "record_agent":
      return value;
    default:
      return "task_execution_worker";
  }
}

function normalizeWorkflowContextBindingKind(value: string): WorkflowContextBindingKind {
  switch (value) {
    case "document_path":
    case "runtime_context":
    case "checklist":
    case "journal_target":
    case "action_ref":
      return value;
    default:
      return "document_path";
  }
}

function normalizePhaseCompletionMode(value: string): WorkflowPhaseCompletionMode {
  switch (value) {
    case "manual":
    case "session_ended":
    case "checklist_passed":
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

function normalizeWorkflowPhaseExecutionStatus(value: string): WorkflowPhaseExecutionStatus {
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

function mapWorkflowPhaseDefinition(raw: Record<string, unknown>): WorkflowPhaseDefinition {
  return {
    key: String(raw.key ?? ""),
    title: String(raw.title ?? ""),
    description: String(raw.description ?? ""),
    agent_instructions: Array.isArray(raw.agent_instructions)
      ? raw.agent_instructions.filter((item): item is string => typeof item === "string")
      : [],
    context_bindings: Array.isArray(raw.context_bindings)
      ? raw.context_bindings
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map((item) => ({
            kind: normalizeWorkflowContextBindingKind(String(item.kind ?? "document_path")),
            locator: String(item.locator ?? ""),
            reason: String(item.reason ?? ""),
            required: item.required !== false,
            title: item.title != null ? String(item.title) : null,
          }))
      : [],
    requires_session: Boolean(raw.requires_session),
    completion_mode: normalizePhaseCompletionMode(String(raw.completion_mode ?? "manual")),
    default_artifact_type:
      raw.default_artifact_type != null
        ? normalizeWorkflowRecordArtifactType(String(raw.default_artifact_type))
        : null,
    default_artifact_title:
      raw.default_artifact_title != null ? String(raw.default_artifact_title) : null,
  };
}

function mapWorkflowRecordPolicy(raw: unknown): WorkflowRecordPolicy {
  if (!raw || typeof raw !== "object") {
    return {
      emit_summary: true,
      emit_journal_update: true,
      emit_archive_suggestion: true,
    };
  }
  const value = raw as Record<string, unknown>;
  return {
    emit_summary: value.emit_summary !== false,
    emit_journal_update: value.emit_journal_update !== false,
    emit_archive_suggestion: value.emit_archive_suggestion !== false,
  };
}

function mapWorkflowPhaseState(raw: Record<string, unknown>): WorkflowPhaseState {
  return {
    phase_key: String(raw.phase_key ?? ""),
    status: normalizeWorkflowPhaseExecutionStatus(String(raw.status ?? "pending")),
    session_binding_id: raw.session_binding_id != null ? String(raw.session_binding_id) : null,
    started_at: raw.started_at != null ? String(raw.started_at) : null,
    completed_at: raw.completed_at != null ? String(raw.completed_at) : null,
    summary: raw.summary != null ? String(raw.summary) : null,
  };
}

function mapWorkflowRecordArtifact(raw: Record<string, unknown>): WorkflowRecordArtifact {
  return {
    id: String(raw.id ?? ""),
    phase_key: String(raw.phase_key ?? ""),
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
    version: Number.isFinite(Number(raw.version)) ? Number(raw.version) : 1,
    enabled: raw.enabled !== false,
    phases: Array.isArray(raw.phases)
      ? raw.phases
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapWorkflowPhaseDefinition)
      : [],
    record_policy: mapWorkflowRecordPolicy(raw.record_policy),
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

export function mapWorkflowTemplate(raw: Record<string, unknown>): WorkflowTemplate {
  return {
    key: String(raw.key ?? ""),
    name: String(raw.name ?? "未命名 Workflow Template"),
    description: String(raw.description ?? ""),
    target_kind: normalizeWorkflowTargetKind(String(raw.target_kind ?? "task")),
    recommended_role: normalizeWorkflowAgentRole(String(raw.recommended_role ?? "task_execution_worker")),
    phases: Array.isArray(raw.phases)
      ? raw.phases
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapWorkflowPhaseDefinition)
      : [],
    record_policy: mapWorkflowRecordPolicy(raw.record_policy),
  };
}

export function mapWorkflowAssignment(raw: Record<string, unknown>): WorkflowAssignment {
  return {
    id: String(raw.id ?? ""),
    project_id: String(raw.project_id ?? ""),
    workflow_id: String(raw.workflow_id ?? ""),
    role: normalizeWorkflowAgentRole(String(raw.role ?? "task_execution_worker")),
    enabled: raw.enabled !== false,
    is_default: Boolean(raw.is_default),
    created_at: String(raw.created_at ?? new Date().toISOString()),
    updated_at: String(raw.updated_at ?? new Date().toISOString()),
  };
}

export function mapWorkflowRun(raw: Record<string, unknown>): WorkflowRun {
  return {
    id: String(raw.id ?? ""),
    workflow_id: String(raw.workflow_id ?? ""),
    target_kind: normalizeWorkflowTargetKind(String(raw.target_kind ?? "task")),
    target_id: String(raw.target_id ?? ""),
    status: normalizeWorkflowRunStatus(String(raw.status ?? "draft")),
    current_phase_key: raw.current_phase_key != null ? String(raw.current_phase_key) : null,
    phase_states: Array.isArray(raw.phase_states)
      ? raw.phase_states
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapWorkflowPhaseState)
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
  const raw = await api.get<Record<string, unknown>[]>(`/workflows${query}`);
  return raw.map(mapWorkflowDefinition);
}

export async function fetchWorkflowTemplates(): Promise<WorkflowTemplate[]> {
  const raw = await api.get<Record<string, unknown>[]>("/workflow-templates");
  return raw.map(mapWorkflowTemplate);
}

export async function bootstrapWorkflowTemplate(builtinKey: string): Promise<WorkflowDefinition> {
  const raw = await api.post<Record<string, unknown>>(
    `/workflow-templates/${encodeURIComponent(builtinKey)}/bootstrap`,
    {},
  );
  return mapWorkflowDefinition(raw);
}

export async function fetchProjectWorkflowAssignments(projectId: string): Promise<WorkflowAssignment[]> {
  const raw = await api.get<Record<string, unknown>[]>(`/projects/${projectId}/workflow-assignments`);
  return raw.map(mapWorkflowAssignment);
}

export async function assignProjectWorkflow(input: {
  project_id: string;
  workflow_id: string;
  role: WorkflowAgentRole;
  enabled?: boolean;
  is_default?: boolean;
}): Promise<WorkflowAssignment> {
  const raw = await api.post<Record<string, unknown>>(
    `/projects/${input.project_id}/workflow-assignments`,
    {
      workflow_id: input.workflow_id,
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
    `/workflow-runs/targets/${targetKind}/${targetId}`,
  );
  return raw.map(mapWorkflowRun);
}

export async function startWorkflowRun(input: {
  workflow_id?: string;
  workflow_key?: string;
  target_kind: WorkflowTargetKind;
  target_id: string;
}): Promise<WorkflowRun> {
  const raw = await api.post<Record<string, unknown>>("/workflows/runs", input);
  return mapWorkflowRun(raw);
}

export async function activateWorkflowPhase(input: {
  run_id: string;
  phase_key: string;
  session_binding_id?: string;
}): Promise<WorkflowRun> {
  const raw = await api.post<Record<string, unknown>>(
    `/workflow-runs/${input.run_id}/phases/${encodeURIComponent(input.phase_key)}/activate`,
    {
      session_binding_id: input.session_binding_id,
    },
  );
  return mapWorkflowRun(raw);
}

export async function completeWorkflowPhase(input: {
  run_id: string;
  phase_key: string;
  summary?: string;
  record_artifacts?: Array<{
    artifact_type: WorkflowRecordArtifactType;
    title: string;
    content: string;
  }>;
}): Promise<WorkflowRun> {
  const raw = await api.post<Record<string, unknown>>(
    `/workflow-runs/${input.run_id}/phases/${encodeURIComponent(input.phase_key)}/complete`,
    {
      summary: input.summary,
      record_artifacts: input.record_artifacts ?? [],
    },
  );
  return mapWorkflowRun(raw);
}

