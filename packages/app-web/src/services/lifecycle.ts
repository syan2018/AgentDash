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
import type {
  AgentRunCommandOnlyRequest,
  AgentRunComposerSubmitRequest,
  AgentRunMessageCommandResponse,
  AgentRunWorkspaceView,
} from "../generated/workflow-contracts";
import type { AgentRunCommandReceipt } from "../generated/project-agent-contracts";

function agentRunCommandPath(runId: string, agentId: string, route: string): string {
  return `/agent-runs/${encodeURIComponent(runId)}/agents/${encodeURIComponent(agentId)}${route}`;
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

export async function fetchProjectAgentRuns(projectId: string): Promise<AgentRunWorkspaceListView> {
  return api.get<AgentRunWorkspaceListView>(
    `/projects/${encodeURIComponent(projectId)}/agent-runs`,
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

export async function fetchAgentRunWorkspace(
  runId: string,
  agentId: string,
): Promise<AgentRunWorkspaceView> {
  return api.get<AgentRunWorkspaceView>(agentRunCommandPath(runId, agentId, "/workspace"));
}

export async function submitAgentRunComposerInput(
  runId: string,
  agentId: string,
  request: AgentRunComposerSubmitRequest,
): Promise<AgentRunMessageCommandResponse> {
  return api.post<AgentRunMessageCommandResponse>(
    agentRunCommandPath(runId, agentId, "/composer-submit"),
    request,
  );
}

export async function deleteAgentRunMailboxMessage(
  runId: string,
  agentId: string,
  messageId: string,
  request: AgentRunCommandOnlyRequest,
): Promise<AgentRunMessageCommandResponse> {
  return api.delete<AgentRunMessageCommandResponse>(
    agentRunCommandPath(
      runId,
      agentId,
      `/mailbox/messages/${encodeURIComponent(messageId)}`,
    ),
    request,
  );
}

export async function promoteAgentRunMailboxMessage(
  runId: string,
  agentId: string,
  messageId: string,
  request: AgentRunCommandOnlyRequest,
): Promise<AgentRunMessageCommandResponse> {
  return api.post<AgentRunMessageCommandResponse>(
    agentRunCommandPath(
      runId,
      agentId,
      `/mailbox/messages/${encodeURIComponent(messageId)}/promote`,
    ),
    request,
  );
}

export async function resumeAgentRunMailbox(
  runId: string,
  agentId: string,
  request: AgentRunCommandOnlyRequest,
): Promise<AgentRunMessageCommandResponse> {
  return api.post<AgentRunMessageCommandResponse>(
    agentRunCommandPath(runId, agentId, "/mailbox/resume"),
    request,
  );
}

export async function cancelAgentRun(
  runId: string,
  agentId: string,
  request: AgentRunCommandOnlyRequest,
): Promise<AgentRunCommandReceipt> {
  return api.post<AgentRunCommandReceipt>(agentRunCommandPath(runId, agentId, "/cancel"), request);
}
