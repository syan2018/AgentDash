import { api } from "../api/client";
import type {
  AgentRunForkRequest,
  AgentRunForkResponse,
  AgentRunForkSubmitRequest,
  AgentRunMessageCommandResponse,
} from "../generated/agent-run-interaction-contracts";
import { agentRunScopedPath } from "./agentRunRuntime";

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
