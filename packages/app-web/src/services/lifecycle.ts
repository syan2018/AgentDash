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
  EnqueuePendingMessageRequest,
  EnqueuePendingMessageResponse,
  LifecycleAgentMessageRequest,
  LifecycleAgentMessageResponse,
  LifecycleAgentSteeringRequest,
  LifecycleAgentSteeringResponse,
  PendingMessageView,
} from "../generated/workflow-contracts";

function sessionCommandPath(runtimeSessionId: string, route: string): string {
  return `/sessions/${encodeURIComponent(runtimeSessionId)}${route}`;
}

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
    sessionCommandPath(runtimeSessionId, "/messages"),
    request,
  );
}

export async function sendLifecycleAgentSteeringMessageByRuntimeSession(
  runtimeSessionId: string,
  request: LifecycleAgentSteeringRequest,
): Promise<LifecycleAgentSteeringResponse> {
  return api.post<LifecycleAgentSteeringResponse>(
    sessionCommandPath(runtimeSessionId, "/steering"),
    request,
  );
}

export async function listPendingMessages(
  runtimeSessionId: string,
): Promise<PendingMessageView[]> {
  return api.get<PendingMessageView[]>(
    sessionCommandPath(runtimeSessionId, "/pending-messages"),
  );
}

export async function enqueuePendingMessage(
  runtimeSessionId: string,
  body: EnqueuePendingMessageRequest,
): Promise<EnqueuePendingMessageResponse> {
  return api.post<EnqueuePendingMessageResponse>(
    sessionCommandPath(runtimeSessionId, "/pending-messages"),
    body,
  );
}

export async function deletePendingMessage(
  runtimeSessionId: string,
  messageId: string,
): Promise<void> {
  await api.delete<void>(
    sessionCommandPath(
      runtimeSessionId,
      `/pending-messages/${encodeURIComponent(messageId)}`,
    ),
  );
}

export async function promotePendingMessage(
  runtimeSessionId: string,
  messageId: string,
): Promise<void> {
  await api.post<void>(
    sessionCommandPath(
      runtimeSessionId,
      `/pending-messages/${encodeURIComponent(messageId)}/promote`,
    ),
    {},
  );
}
