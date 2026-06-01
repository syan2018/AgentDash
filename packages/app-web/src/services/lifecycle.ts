/**
 * Lifecycle View Service
 *
 * 提供 LifecycleRunView / SubjectExecutionView / AgentFrameRuntimeView /
 * RuntimeSessionTraceView 等 target view 的 API 访问函数。
 *
 * 数据流向：api.client → mapper → typed view → lifecycleStore
 */

import { api } from "../api/client";
import {
  asRecord,
  asRecordArray,
  asStringArray,
  requireStringField,
  requireNumberField,
} from "../api/mappers";
import type {
  LifecycleRunView,
  WorkflowGraphInstanceView,
  ActivityStateView,
  ActivityAttemptView,
  LifecycleAgentView,
  AgentFrameRuntimeView,
  SubjectExecutionView,
  RuntimeSessionTraceView,
  LifecycleSubjectAssociationDto,
} from "../types";

// ─── Mapper：ActivityAttemptView ─────────────────────────

function mapActivityAttemptView(raw: Record<string, unknown>): ActivityAttemptView {
  return {
    graph_instance_id: requireStringField(raw, "graph_instance_id"),
    activity_key: requireStringField(raw, "activity_key"),
    attempt: requireNumberField(raw, "attempt"),
    status: requireStringField(raw, "status"),
    assignment_ref: raw.assignment_ref != null ? String(raw.assignment_ref) : undefined,
    executor_run: raw.executor_run != null ? String(raw.executor_run) : undefined,
  };
}

// ─── Mapper：ActivityStateView ──────────────────────────

function mapActivityStateView(raw: Record<string, unknown>): ActivityStateView {
  return {
    activity_key: requireStringField(raw, "activity_key"),
    status: requireStringField(raw, "status"),
    attempts: asRecordArray(raw.attempts).map(mapActivityAttemptView),
  };
}

// ─── Mapper：WorkflowGraphInstanceView ──────────────────

function mapWorkflowGraphInstanceView(raw: Record<string, unknown>): WorkflowGraphInstanceView {
  return {
    id: requireStringField(raw, "id"),
    run_id: requireStringField(raw, "run_id"),
    graph_id: requireStringField(raw, "graph_id"),
    role: requireStringField(raw, "role"),
    status: requireStringField(raw, "status"),
    activities: asRecordArray(raw.activities).map(mapActivityStateView),
  };
}

// ─── Mapper：LifecycleAgentView ─────────────────────────

function mapLifecycleAgentView(raw: Record<string, unknown>): LifecycleAgentView {
  return {
    id: requireStringField(raw, "id"),
    run_id: requireStringField(raw, "run_id"),
    agent_kind: requireStringField(raw, "agent_kind"),
    agent_role: requireStringField(raw, "agent_role"),
    status: requireStringField(raw, "status"),
    current_frame_id: raw.current_frame_id != null ? String(raw.current_frame_id) : undefined,
  };
}

// ─── Mapper：LifecycleSubjectAssociationDto ─────────────

function mapSubjectAssociation(raw: Record<string, unknown>): LifecycleSubjectAssociationDto {
  return {
    id: requireStringField(raw, "id"),
    anchor_run_id: requireStringField(raw, "anchor_run_id"),
    anchor_agent_id: raw.anchor_agent_id != null ? String(raw.anchor_agent_id) : undefined,
    subject_kind: requireStringField(raw, "subject_kind"),
    subject_id: requireStringField(raw, "subject_id"),
    role: requireStringField(raw, "role"),
  };
}

// ─── Mapper：LifecycleRunView ───────────────────────────

function mapLifecycleRunView(raw: Record<string, unknown>): LifecycleRunView {
  return {
    id: requireStringField(raw, "id"),
    status: requireStringField(raw, "status") as LifecycleRunView["status"],
    workflow_graph_instances: asRecordArray(raw.workflow_graph_instances).map(mapWorkflowGraphInstanceView),
    agents: asRecordArray(raw.agents).map(mapLifecycleAgentView),
    subject_associations: asRecordArray(raw.subject_associations).map(mapSubjectAssociation),
    runtime_trace_refs: asStringArray(raw.runtime_trace_refs),
  };
}

// ─── Mapper：AgentFrameRuntimeView ──────────────────────

function mapAgentFrameRuntimeView(raw: Record<string, unknown>): AgentFrameRuntimeView {
  const record = asRecord(raw);
  if (!record) throw new Error("AgentFrameRuntimeView 必须是对象");
  return {
    id: requireStringField(record, "id"),
    agent_id: requireStringField(record, "agent_id"),
    revision: requireNumberField(record, "revision"),
    procedure_ref: record.procedure_ref != null ? String(record.procedure_ref) : undefined,
    capability_surface: (asRecord(record.capability_surface) ?? {}) as Record<string, unknown>,
    context_slice: (asRecord(record.context_slice) ?? {}) as Record<string, unknown>,
    vfs_surface: (asRecord(record.vfs_surface) ?? {}) as Record<string, unknown>,
    mcp_surface: (asRecord(record.mcp_surface) ?? {}) as Record<string, unknown>,
    runtime_session_refs: asStringArray(record.runtime_session_refs),
  };
}

// ─── Mapper：SubjectExecutionView ───────────────────────

function mapSubjectExecutionView(raw: Record<string, unknown>): SubjectExecutionView {
  const agentRaw = asRecord(raw.current_agent);
  const attemptRaw = asRecord(raw.latest_attempt);
  return {
    subject_kind: requireStringField(raw, "subject_kind"),
    subject_id: requireStringField(raw, "subject_id"),
    current_agent: agentRaw ? mapLifecycleAgentView(agentRaw) : undefined,
    latest_attempt: attemptRaw ? mapActivityAttemptView(attemptRaw) : undefined,
    artifacts: (asRecord(raw.artifacts) ?? {}) as Record<string, unknown>,
  };
}

// ─── Mapper：RuntimeSessionTraceView ────────────────────

function mapRuntimeSessionTraceView(raw: Record<string, unknown>): RuntimeSessionTraceView {
  return {
    id: requireStringField(raw, "id"),
    events: Array.isArray(raw.events) ? raw.events : [],
    turns: Array.isArray(raw.turns) ? raw.turns : [],
  };
}

// ─── API Functions ──────────────────────────────────────

export async function fetchLifecycleRun(runId: string): Promise<LifecycleRunView> {
  const raw = await api.get<Record<string, unknown>>(`/lifecycle-runs/${encodeURIComponent(runId)}/view`);
  return mapLifecycleRunView(raw);
}

export async function fetchSubjectExecution(
  subjectKind: string,
  subjectId: string,
): Promise<SubjectExecutionView> {
  const raw = await api.get<Record<string, unknown>>(
    `/subjects/${encodeURIComponent(subjectKind)}/${encodeURIComponent(subjectId)}/execution`,
  );
  return mapSubjectExecutionView(raw);
}

export async function fetchAgentFrameRuntime(frameId: string): Promise<AgentFrameRuntimeView> {
  const raw = await api.get<Record<string, unknown>>(
    `/agent-frames/${encodeURIComponent(frameId)}/runtime`,
  );
  return mapAgentFrameRuntimeView(raw);
}

export async function fetchRuntimeTrace(runtimeSessionId: string): Promise<RuntimeSessionTraceView> {
  const raw = await api.get<Record<string, unknown>>(
    `/runtime-sessions/${encodeURIComponent(runtimeSessionId)}/trace`,
  );
  return mapRuntimeSessionTraceView(raw);
}
