import { api, type ApiHttpError } from "../api/client";
import type { SessionProjectionViewResponse } from "../generated/session-contracts";
import type {
  AgentRunCommandOnlyRequest,
  AgentRunContextCompactionCommandResponse,
  AgentRunToolCallApprovalResponse,
  AgentRunToolCallRejectionResponse,
} from "../generated/agent-run-mailbox-contracts";
import type {
  ManagedRuntimeInteractionResponse,
  ManagedRuntimeOperationReceipt,
} from "../generated/agent-runtime-contracts";
import {
  decodeManagedRuntimeChangePage,
  decodeManagedRuntimeSnapshot,
  encodeRuntimeU64,
  type ManagedRuntimeChangePage,
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

export async function compactAgentRunContext(
  runId: string,
  agentId: string,
  request: AgentRunCommandOnlyRequest,
): Promise<AgentRunContextCompactionCommandResponse> {
  return api.post<AgentRunContextCompactionCommandResponse>(
    agentRunScopedPath({ runId, agentId }, "/runtime/context/compact"),
    request,
  );
}

export async function approveAgentRunToolCall(
  target: AgentRunRuntimeTarget,
  toolCallId: string,
): Promise<AgentRunToolCallApprovalResponse> {
  return api.post<AgentRunToolCallApprovalResponse>(
    agentRunScopedPath(target, `/runtime/tool-approvals/${encodeURIComponent(toolCallId)}/approve`),
    undefined,
  );
}

export async function rejectAgentRunToolCall(
  target: AgentRunRuntimeTarget,
  toolCallId: string,
  reason?: string,
): Promise<AgentRunToolCallRejectionResponse> {
  return api.post<AgentRunToolCallRejectionResponse>(
    agentRunScopedPath(target, `/runtime/tool-approvals/${encodeURIComponent(toolCallId)}/reject`),
    { reason: reason ?? null },
  );
}

export async function respondAgentRunInteraction(
  target: AgentRunRuntimeTarget,
  interactionId: string,
  response: ManagedRuntimeInteractionResponse,
): Promise<ManagedRuntimeOperationReceipt> {
  return api.post<ManagedRuntimeOperationReceipt>(
    agentRunScopedPath(target, `/runtime/interactions/${encodeURIComponent(interactionId)}/respond`),
    response,
  );
}
