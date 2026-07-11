import { api } from "../api/client";
import type {
  AgentRunComposerSubmitRequest,
  AgentRunMessageCommandResponse,
} from "../generated/agent-run-mailbox-contracts";
import type { AgentRunCommandOnlyRequest } from "../generated/workflow-contracts";
import type { OperationReceipt } from "../generated/agent-runtime-contracts";
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

export async function cancelAgentRun(
  runId: string,
  agentId: string,
  request: AgentRunCommandOnlyRequest,
): Promise<OperationReceipt> {
  return api.post<OperationReceipt>(
    agentRunScopedPath({ runId, agentId }, "/cancel"),
    request,
  );
}
