import { api, type ApiHttpError } from "../api/client";
import type {
  SessionEventsPageResponse,
  SessionProjectionViewResponse,
} from "../generated/session-contracts";
import type {
  AgentRunToolCallApprovalResponse,
  AgentRunToolCallRejectionResponse,
} from "../generated/agent-run-mailbox-contracts";

export interface AgentRunRuntimeTarget {
  runId: string;
  agentId: string;
}

export function agentRunScopedPath(target: AgentRunRuntimeTarget, route: string): string {
  return `/agent-runs/${encodeURIComponent(target.runId)}/agents/${encodeURIComponent(target.agentId)}${route}`;
}

export async function fetchAgentRunJournalEvents(
  target: AgentRunRuntimeTarget,
  afterSeq = 0,
  limit = 500,
): Promise<SessionEventsPageResponse> {
  const params = new URLSearchParams();
  params.set("after_seq", String(afterSeq));
  params.set("limit", String(limit));
  return api.get<SessionEventsPageResponse>(
    agentRunScopedPath(target, `/journal/events?${params.toString()}`),
  );
}

export async function fetchAgentRunRuntimeContextProjection(
  target: AgentRunRuntimeTarget,
): Promise<SessionProjectionViewResponse | null> {
  try {
    return await api.get<SessionProjectionViewResponse>(
      agentRunScopedPath(target, "/runtime/context/projection"),
    );
  } catch (err) {
    if ((err as ApiHttpError).status === 404) return null;
    throw err;
  }
}

export async function approveAgentRunToolCall(
  target: AgentRunRuntimeTarget,
  toolCallId: string,
): Promise<AgentRunToolCallApprovalResponse> {
  return api.post<AgentRunToolCallApprovalResponse>(
    agentRunScopedPath(target, `/runtime/tool-approvals/${encodeURIComponent(toolCallId)}/approve`),
    {},
  );
}

export async function rejectAgentRunToolCall(
  target: AgentRunRuntimeTarget,
  toolCallId: string,
  reason?: string,
): Promise<AgentRunToolCallRejectionResponse> {
  return api.post<AgentRunToolCallRejectionResponse>(
    agentRunScopedPath(target, `/runtime/tool-approvals/${encodeURIComponent(toolCallId)}/reject`),
    { reason },
  );
}
