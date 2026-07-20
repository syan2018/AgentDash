import { api } from "../api/client";
import type {
  AgentRunCommandReceipt,
  AgentRunCommandOnlyRequest,
  AgentRunForkRequest,
  AgentRunForkResponse,
  AgentRunForkSubmitRequest,
  AgentRunComposerSubmitRequest,
  AgentRunMessageCommandResponse,
} from "../generated/agent-run-interaction-contracts";
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
