import { api } from "../api/client";
import type {
  AgentInteractionResponse,
} from "../generated/agent-runtime-contracts";
import type {
  AgentRunProductRuntimeCommand,
  AgentRunProductRuntimeCommandRequest as AgentRunProductRuntimeCommandRequestWire,
} from "../generated/agent-run-product-projection-contracts";
import {
  decodeAgentRuntimeOperationReceipt,
  decodeAgentRuntimeContextProjection,
  decodeAgentRuntimeView,
  type AgentRuntimeContextProjection,
  type AgentRuntimeOperationReceipt,
  type AgentRuntimeView,
} from "../generated/agent-runtime-codecs";

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
  required: AgentRuntimeView["observation"]["context"],
  signal?: AbortSignal,
): Promise<AgentRuntimeContextProjection> {
  const path = agentRunScopedPath(target, "/runtime/context/projection");
  const query = new URLSearchParams({
    snapshot_revision: required.snapshot_revision.toString(),
    recipe_digest: required.recipe_digest,
    authority: required.authority,
    fidelity: required.fidelity,
  });
  if (required.context_revision != null) {
    query.set("context_revision", required.context_revision);
  }
  const payload = await api.get<unknown>(`${path}?${query}`, { signal });
  return decodeAgentRuntimeContextProjection(payload);
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
  response: AgentInteractionResponse,
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
