import { api, type ApiHttpError } from "../api/client";
import type { SessionProjectionViewResponse } from "../generated/session-contracts";
import type {
  AgentRunProductRuntimeCommand,
  AgentRunProductRuntimeCommandRequest as AgentRunProductRuntimeCommandRequestWire,
} from "../generated/agent-run-product-projection-contracts";
import type {
  ManagedRuntimeInteractionResponse,
} from "../generated/agent-runtime-contracts";
import {
  decodeManagedRuntimeChangePage,
  decodeManagedRuntimeOperationReceipt,
  decodeManagedRuntimeSnapshot,
  encodeRuntimeU64,
  type ManagedRuntimeChangePage,
  type ManagedRuntimeOperationReceipt,
  type ManagedRuntimeSnapshot,
} from "../generated/agent-runtime-validators";

export interface AgentRunRuntimeTarget {
  runId: string;
  agentId: string;
}

export function agentRunScopedPath(target: AgentRunRuntimeTarget, route: string): string {
  return `/agent-runs/${encodeURIComponent(target.runId)}/agents/${encodeURIComponent(target.agentId)}${route}`;
}

export async function fetchManagedRuntimeSnapshot(
  target: AgentRunRuntimeTarget,
): Promise<ManagedRuntimeSnapshot> {
  const payload = await api.get<unknown>(agentRunScopedPath(target, "/runtime/snapshot"));
  return decodeManagedRuntimeSnapshot(payload);
}

export async function fetchManagedRuntimeChangePage(
  target: AgentRunRuntimeTarget,
  after?: bigint,
  limit = 256,
): Promise<ManagedRuntimeChangePage> {
  const params = new URLSearchParams();
  params.set("limit", String(limit));
  if (after !== undefined) {
    params.set("after", encodeRuntimeU64(after));
  }
  const payload = await api.get<unknown>(
    agentRunScopedPath(target, `/runtime/changes?${params.toString()}`),
  );
  return decodeManagedRuntimeChangePage(payload);
}

export async function fetchAgentRunRuntimeContextProjection(
  target: AgentRunRuntimeTarget,
): Promise<SessionProjectionViewResponse | null> {
  try {
    return await api.get<SessionProjectionViewResponse>(
      agentRunScopedPath(target, "/runtime/context/projection"),
    );
  } catch (error) {
    if ((error as ApiHttpError).status === 404) return null;
    throw error;
  }
}

export interface AgentRunProductRuntimeCommandRequest {
  client_command_id: string;
  command: AgentRunProductRuntimeCommand;
}

export async function executeAgentRunRuntimeCommand(
  target: AgentRunRuntimeTarget,
  request: AgentRunProductRuntimeCommandRequest,
): Promise<ManagedRuntimeOperationReceipt> {
  const wireRequest: AgentRunProductRuntimeCommandRequestWire = {
    client_command_id: request.client_command_id,
    command: request.command,
  };
  const payload = await api.post<unknown>(
    agentRunScopedPath(target, "/runtime/commands"),
    wireRequest,
  );
  return decodeManagedRuntimeOperationReceipt(payload);
}

export async function compactAgentRunContext(
  target: AgentRunRuntimeTarget,
  clientCommandId: string,
): Promise<ManagedRuntimeOperationReceipt> {
  const snapshot = await fetchManagedRuntimeSnapshot(target);
  if (snapshot.command_availability.request_compaction?.status !== "available") {
    throw new Error("Managed Runtime 当前不接受 context compaction");
  }
  return executeAgentRunRuntimeCommand(target, {
    client_command_id: clientCommandId,
    command: { kind: "request_compaction" },
  });
}

export async function respondAgentRunInteraction(
  target: AgentRunRuntimeTarget,
  interactionId: string,
  response: ManagedRuntimeInteractionResponse,
  clientCommandId: string,
): Promise<ManagedRuntimeOperationReceipt> {
  return executeAgentRunRuntimeCommand(target, {
    client_command_id: clientCommandId,
    command: {
      kind: "resolve_interaction",
      interaction_id: interactionId,
      response,
    },
  });
}
