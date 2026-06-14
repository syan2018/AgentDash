import { api } from "../api/client";
import type {
  AgentRunCommandReceipt,
  AgentRunComposerSubmitRequest,
  AgentRunMailboxMessageContentView,
  AgentRunMailboxMoveRequest,
  AgentRunMessageCommandResponse,
} from "../generated/agent-run-mailbox-contracts";
import type { AgentRunCommandOnlyRequest } from "../generated/workflow-contracts";

function agentRunCommandPath(runId: string, agentId: string, route: string): string {
  return `/agent-runs/${encodeURIComponent(runId)}/agents/${encodeURIComponent(agentId)}${route}`;
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

export async function moveAgentRunMailboxMessage(
  runId: string,
  agentId: string,
  messageId: string,
  request: AgentRunMailboxMoveRequest,
): Promise<{ ok: boolean; order_key: number }> {
  return api.put<{ ok: boolean; order_key: number }>(
    agentRunCommandPath(
      runId,
      agentId,
      `/mailbox/messages/${encodeURIComponent(messageId)}/move`,
    ),
    request,
  );
}

export async function fetchAgentRunMailboxMessageContent(
  runId: string,
  agentId: string,
  messageId: string,
): Promise<AgentRunMailboxMessageContentView> {
  return api.get<AgentRunMailboxMessageContentView>(
    agentRunCommandPath(
      runId,
      agentId,
      `/mailbox/messages/${encodeURIComponent(messageId)}/content`,
    ),
  );
}

export async function cancelAgentRun(
  runId: string,
  agentId: string,
  request: AgentRunCommandOnlyRequest,
): Promise<AgentRunCommandReceipt> {
  return api.post<AgentRunCommandReceipt>(agentRunCommandPath(runId, agentId, "/cancel"), request);
}
