/**
 * Lifecycle target view types.
 *
 * 对应 target-state-blueprint B6 阶段定义的前端读取视图体系。
 * 这些类型是前端归一化 store 与 API 函数的基础合约，
 * 后续由 contracts generator 输出时将替换此手写定义。
 */

import type { LifecycleRunStatus } from "../generated/workflow-contracts";
import type { JsonValue } from "../generated/common-contracts";

// ─── Run 视图 ─────────────────────────────────────────────

export type LifecycleRunView = {
  id: string;
  status: LifecycleRunStatus;
  workflow_graph_instances: WorkflowGraphInstanceView[];
  agents: LifecycleAgentView[];
  subject_associations: LifecycleSubjectAssociationDto[];
  runtime_trace_refs: string[];
};

// ─── Graph Instance 视图 ─────────────────────────────────

export type WorkflowGraphInstanceView = {
  id: string;
  run_id: string;
  graph_id: string;
  role: string;
  status: string;
  activities: ActivityStateView[];
};

export type ActivityStateView = {
  activity_key: string;
  status: string;
  attempts: ActivityAttemptView[];
};

export type ActivityAttemptView = {
  graph_instance_id: string;
  activity_key: string;
  attempt: number;
  status: string;
  assignment_ref?: string;
  executor_run?: string;
};

// ─── Agent 视图 ──────────────────────────────────────────

export type LifecycleAgentView = {
  id: string;
  run_id: string;
  agent_kind: string;
  agent_role: string;
  status: string;
  current_frame_id?: string;
};

// ─── Agent Frame Runtime 视图 ────────────────────────────

export type AgentFrameRuntimeView = {
  id: string;
  agent_id: string;
  revision: number;
  procedure_ref?: string;
  capability_surface: Record<string, unknown>;
  context_slice: Record<string, unknown>;
  vfs_surface: Record<string, unknown>;
  mcp_surface: Record<string, unknown>;
  runtime_session_refs: string[];
};

// ─── Subject Execution 视图 ──────────────────────────────

export type SubjectExecutionView = {
  subject_kind: string;
  subject_id: string;
  current_agent?: LifecycleAgentView;
  latest_attempt?: ActivityAttemptView;
  artifacts: Record<string, unknown>;
};

// ─── Runtime Trace 视图（降级后的 session 详情） ─────────

export type RuntimeSessionTraceView = {
  id: string;
  events: JsonValue[];
  turns: JsonValue[];
};

// ─── Subject Association DTO ─────────────────────────────

export type LifecycleSubjectAssociationDto = {
  id: string;
  anchor_run_id: string;
  anchor_agent_id?: string;
  subject_kind: string;
  subject_id: string;
  role: string;
};

// ─── Subject Execution 索引 key ─────────────────────────

export function subjectExecutionKey(kind: string, id: string): string {
  return `${kind}:${id}`;
}
