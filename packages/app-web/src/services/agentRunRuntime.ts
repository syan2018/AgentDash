import { api, type ApiHttpError } from "../api/client";
import type {
  ApproveToolCallResponse,
  RejectToolCallResponse,
  SessionEventsPageResponse,
  SessionProjectionViewResponse,
} from "../generated/session-contracts";
import type { AgentConversationFeedSnapshot } from "../generated/workflow-contracts";
import type { SessionRuntimeControlView } from "../types";

export interface AgentRunRuntimeTarget {
  runId: string;
  agentId: string;
}

export function agentRunScopedPath(target: AgentRunRuntimeTarget, route: string): string {
  return `/agent-runs/${encodeURIComponent(target.runId)}/agents/${encodeURIComponent(target.agentId)}${route}`;
}

export async function fetchAgentRunRuntimeEvents(
  target: AgentRunRuntimeTarget,
  afterSeq = 0,
  limit = 500,
): Promise<SessionEventsPageResponse> {
  const params = new URLSearchParams();
  params.set("after_seq", String(afterSeq));
  params.set("limit", String(limit));
  return api.get<SessionEventsPageResponse>(
    agentRunScopedPath(target, `/runtime/events?${params.toString()}`),
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

export async function fetchAgentRunConversationFeed(
  target: AgentRunRuntimeTarget,
): Promise<AgentConversationFeedSnapshot | null> {
  try {
    return await api.get<AgentConversationFeedSnapshot>(
      agentRunScopedPath(target, "/conversation/feed"),
    );
  } catch (err) {
    if ((err as ApiHttpError).status === 404) return null;
    throw err;
  }
}

export async function fetchAgentRunRuntimeControl(
  target: AgentRunRuntimeTarget,
): Promise<SessionRuntimeControlView> {
  return api.get<SessionRuntimeControlView>(agentRunScopedPath(target, "/runtime/control"));
}

export async function approveAgentRunToolCall(
  target: AgentRunRuntimeTarget,
  toolCallId: string,
): Promise<ApproveToolCallResponse> {
  return api.post<ApproveToolCallResponse>(
    agentRunScopedPath(target, `/runtime/tool-approvals/${encodeURIComponent(toolCallId)}/approve`),
    {},
  );
}

export async function rejectAgentRunToolCall(
  target: AgentRunRuntimeTarget,
  toolCallId: string,
  reason?: string,
): Promise<RejectToolCallResponse> {
  return api.post<RejectToolCallResponse>(
    agentRunScopedPath(target, `/runtime/tool-approvals/${encodeURIComponent(toolCallId)}/reject`),
    { reason },
  );
}
