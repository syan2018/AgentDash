/**
 * Lifecycle target view API。
 *
 * 这些返回值已经由 generated contracts 定义，service 层只负责 endpoint 调用。
 */

import { api } from "../api/client";
import type {
  AgentFrameRuntimeView,
  AgentRunWorkspaceListView,
  LifecycleRunView,
  ProjectActiveAgentsView,
  RuntimeSessionTraceView,
  SessionRuntimeControlView,
  SubjectExecutionView,
} from "../types";
import type { AgentRunWorkspaceView } from "../generated/workflow-contracts";
import {
  agentRunScopedPath,
  fetchAgentRunRuntimeControl as fetchScopedAgentRunRuntimeControl,
  type AgentRunRuntimeTarget,
} from "./agentRunRuntime";

function agentRunCommandPath(runId: string, agentId: string, route: string): string {
  return agentRunScopedPath({ runId, agentId }, route);
}

export async function fetchLifecycleRun(runId: string): Promise<LifecycleRunView> {
  return api.get<LifecycleRunView>(`/lifecycle-runs/${encodeURIComponent(runId)}`);
}

export async function fetchSubjectExecution(
  subjectKind: string,
  subjectId: string,
): Promise<SubjectExecutionView> {
  return api.get<SubjectExecutionView>(
    `/subjects/${encodeURIComponent(subjectKind)}/${encodeURIComponent(subjectId)}/execution`,
  );
}

export async function fetchProjectActiveAgents(projectId: string): Promise<ProjectActiveAgentsView> {
  return api.get<ProjectActiveAgentsView>(
    `/projects/${encodeURIComponent(projectId)}/active-agents`,
  );
}

export interface FetchProjectAgentRunsOptions {
  /** 单页大小（后端默认 30，上限 100）。 */
  limit?: number;
  /** keyset 游标，续拉下一页；省略则取首页。 */
  cursor?: string;
}

export async function fetchProjectAgentRuns(
  projectId: string,
  opts: FetchProjectAgentRunsOptions = {},
): Promise<AgentRunWorkspaceListView> {
  const params = new URLSearchParams();
  if (opts.limit != null) params.set("limit", String(opts.limit));
  if (opts.cursor) params.set("cursor", opts.cursor);
  const query = params.toString();
  return api.get<AgentRunWorkspaceListView>(
    `/projects/${encodeURIComponent(projectId)}/agent-runs${query ? `?${query}` : ""}`,
  );
}

export async function fetchSessionRuntimeControl(
  runtimeSessionId: string,
): Promise<SessionRuntimeControlView> {
  return api.get<SessionRuntimeControlView>(
    `/sessions/${encodeURIComponent(runtimeSessionId)}/runtime-control`,
  );
}

export async function fetchAgentRunRuntimeControl(
  target: AgentRunRuntimeTarget,
): Promise<SessionRuntimeControlView> {
  return fetchScopedAgentRunRuntimeControl(target);
}

export async function fetchAgentFrameRuntime(frameId: string): Promise<AgentFrameRuntimeView> {
  return api.get<AgentFrameRuntimeView>(`/agent-frames/${encodeURIComponent(frameId)}/runtime`);
}

export async function fetchRuntimeTrace(runtimeSessionId: string): Promise<RuntimeSessionTraceView> {
  return api.get<RuntimeSessionTraceView>(
    `/sessions/${encodeURIComponent(runtimeSessionId)}/trace`,
  );
}

export async function fetchAgentRunWorkspace(
  runId: string,
  agentId: string,
): Promise<AgentRunWorkspaceView> {
  return api.get<AgentRunWorkspaceView>(agentRunCommandPath(runId, agentId, "/workspace"));
}
