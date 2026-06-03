/**
 * Lifecycle target view API。
 *
 * 这些返回值已经由 generated contracts 定义，service 层只负责 endpoint 调用。
 */

import { api } from "../api/client";
import type {
  AgentFrameRuntimeView,
  LifecycleRunView,
  ProjectActiveAgentsView,
  ProjectSessionListView,
  RuntimeSessionTraceView,
  SessionRuntimeControlView,
  SubjectExecutionView,
} from "../types";
import type {
  LifecycleAgentMessageRequest,
  LifecycleAgentMessageResponse,
} from "../generated/workflow-contracts";

export async function fetchLifecycleRun(runId: string): Promise<LifecycleRunView> {
  return api.get<LifecycleRunView>(`/lifecycle-runs/${encodeURIComponent(runId)}/view`);
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

export async function fetchProjectSessionList(projectId: string): Promise<ProjectSessionListView> {
  return api.get<ProjectSessionListView>(
    `/projects/${encodeURIComponent(projectId)}/sessions`,
  );
}

export async function fetchSessionRuntimeControl(
  runtimeSessionId: string,
): Promise<SessionRuntimeControlView> {
  return api.get<SessionRuntimeControlView>(
    `/sessions/${encodeURIComponent(runtimeSessionId)}/runtime-control`,
  );
}

export async function fetchAgentFrameRuntime(frameId: string): Promise<AgentFrameRuntimeView> {
  return api.get<AgentFrameRuntimeView>(`/agent-frames/${encodeURIComponent(frameId)}/runtime`);
}

export async function fetchRuntimeTrace(runtimeSessionId: string): Promise<RuntimeSessionTraceView> {
  return api.get<RuntimeSessionTraceView>(
    `/sessions/${encodeURIComponent(runtimeSessionId)}/trace`,
  );
}

export async function sendLifecycleAgentMessageByRuntimeSession(
  runtimeSessionId: string,
  request: LifecycleAgentMessageRequest,
): Promise<LifecycleAgentMessageResponse> {
  return api.post<LifecycleAgentMessageResponse>(
    `/lifecycle-agents/by-runtime-session/${encodeURIComponent(runtimeSessionId)}/messages`,
    request,
  );
}
