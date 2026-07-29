import { api } from "../api/client";
import type { AgentContextSnapshot } from "../generated/agent-service-api";
import type {
  AgentRunProductRuntimeCommand,
  AgentRunProductRuntimeCommandRequest as AgentRunProductRuntimeCommandRequestWire,
} from "../generated/agent-run-product-projection-contracts";
import type {
  AgentRuntimeInteractionResponse,
} from "../generated/agent-runtime-contracts";
import {
  decodeAgentRuntimeOperationReceipt,
  decodeAgentRuntimeView,
  type AgentRuntimeOperationReceipt,
  type AgentRuntimeView,
} from "../generated/agent-runtime-validators";

export interface AgentRunRuntimeTarget {
  runId: string;
  agentId: string;
}

export function agentRunScopedPath(target: AgentRunRuntimeTarget, route: string): string {
  return `/agent-runs/${encodeURIComponent(target.runId)}/agents/${encodeURIComponent(target.agentId)}${route}`;
}

export async function fetchAgentRuntimeView(
  target: AgentRunRuntimeTarget,
): Promise<AgentRuntimeView> {
  const payload = await api.get<unknown>(agentRunScopedPath(target, "/runtime/view"));
  return decodeAgentRuntimeView(payload);
}

export async function fetchAgentRunRuntimeContextProjection(
  target: AgentRunRuntimeTarget,
  requiredRevision: bigint,
  signal?: AbortSignal,
): Promise<AgentContextSnapshot> {
  const path = agentRunScopedPath(target, "/runtime/context/projection");
  return api.get<AgentContextSnapshot>(
    `${path}?required_revision=${requiredRevision}`,
    { signal },
  );
}

export interface AgentRunProductRuntimeCommandRequest {
  client_command_id: string;
  command: AgentRunProductRuntimeCommand;
}

export async function executeAgentRunRuntimeCommand(
  target: AgentRunRuntimeTarget,
  request: AgentRunProductRuntimeCommandRequest,
): Promise<AgentRuntimeOperationReceipt> {
  const wireRequest: AgentRunProductRuntimeCommandRequestWire = {
    client_command_id: request.client_command_id,
    command: request.command,
  };
  const payload = await api.post<unknown>(
    agentRunScopedPath(target, "/runtime/commands"),
    wireRequest,
  );
  return decodeAgentRuntimeOperationReceipt(payload);
}

export async function respondAgentRunInteraction(
  target: AgentRunRuntimeTarget,
  interactionId: string,
  response: AgentRuntimeInteractionResponse,
  clientCommandId: string,
): Promise<AgentRuntimeOperationReceipt> {
  return executeAgentRunRuntimeCommand(target, {
    client_command_id: clientCommandId,
    command: {
      kind: "resolve_interaction",
      interaction_id: interactionId,
      response,
    },
  });
}
