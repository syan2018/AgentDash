import { api } from "../api/client";
import type {
  AgentRunCommandReceipt,
  AgentRunCommandOnlyRequest,
  AgentRunForkRequest,
  AgentRunForkResponse,
  AgentRunForkSubmitRequest,
  AgentRunComposerSubmitRequest,
  AgentRunMailboxMessageContentView,
  AgentRunMailboxMoveRequest,
  AgentRunMessageCommandResponse,
} from "../generated/agent-run-mailbox-contracts";
import { agentRunScopedPath } from "./agentRunRuntime";

export async function submitAgentRunComposerInput(
  runId: string,
  agentId: string,
  request: AgentRunComposerSubmitRequest,
): Promise<AgentRunMessageCommandResponse> {
  return api.post<AgentRunMessageCommandResponse>(
    agentRunScopedPath({ runId, agentId }, "/composer-submit"),
    request,
  );
}

export async function forkAgentRun(
  runId: string,
  agentId: string,
  request: AgentRunForkRequest,
): Promise<AgentRunForkResponse> {
  return api.post<AgentRunForkResponse>(
    agentRunScopedPath({ runId, agentId }, "/fork"),
    request,
  );
}

export async function submitAgentRunForkInput(
  runId: string,
  agentId: string,
  request: AgentRunForkSubmitRequest,
): Promise<AgentRunMessageCommandResponse> {
  return api.post<AgentRunMessageCommandResponse>(
    agentRunScopedPath({ runId, agentId }, "/fork-submit"),
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
    agentRunScopedPath(
      { runId, agentId },
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
    agentRunScopedPath(
      { runId, agentId },
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
    agentRunScopedPath({ runId, agentId }, "/mailbox/resume"),
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
    agentRunScopedPath(
      { runId, agentId },
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
    agentRunScopedPath(
      { runId, agentId },
      `/mailbox/messages/${encodeURIComponent(messageId)}/content`,
    ),
  );
}

export async function cancelAgentRun(
  runId: string,
  agentId: string,
  request: AgentRunCommandOnlyRequest,
): Promise<AgentRunCommandReceipt> {
  return api.post<AgentRunCommandReceipt>(
    agentRunScopedPath({ runId, agentId }, "/cancel"),
    request,
  );
}
