import { api, type ApiHttpError } from "../api/client";
import type {
  SessionEventsPageResponse,
  SessionProjectionViewResponse,
} from "../generated/session-contracts";
import type {
  AgentRunCommandOnlyRequest,
  AgentRunContextCompactionCommandResponse,
  AgentRunToolCallApprovalResponse,
  AgentRunToolCallRejectionResponse,
} from "../generated/agent-run-mailbox-contracts";
import type {
  BoundRuntimeHookPlan,
  DriverThreadId,
  ProfileDigest,
  ProfileProvenance,
  RuntimeBindingId,
  RuntimeDriverGeneration,
  RuntimeEventEnvelope,
  RuntimeTransientCoordinate,
  InteractionResponse,
  OperationReceipt,
  RuntimeContextView,
  RuntimeProfile,
  RuntimeSnapshot,
  RuntimeSubscribeError,
  RuntimeThreadId,
  SurfaceDigest,
} from "../generated/agent-runtime-contracts";

export interface AgentRunRuntimeTarget {
  runId: string;
  agentId: string;
}

export interface AgentRunRuntimeBindingView {
  target: { run_id: string; agent_id: string };
  thread_id: RuntimeThreadId;
  binding_id: RuntimeBindingId;
  driver_generation: RuntimeDriverGeneration;
  source_thread_id: DriverThreadId;
  profile_digest: ProfileDigest;
  profile_provenance: ProfileProvenance;
  bound_profile: RuntimeProfile;
  surface_digest: SurfaceDigest;
  hook_plan: BoundRuntimeHookPlan;
}

export interface AgentRunRuntimeInspectResponse {
  target: { run_id: string; agent_id: string };
  binding: AgentRunRuntimeBindingView | null;
  snapshot: RuntimeSnapshot | null;
}

export type AgentRunRuntimeEventStreamItem =
  | { kind: "event"; durable_cursor: number | null; transient_cursor: RuntimeTransientCoordinate | null; envelope: RuntimeEventEnvelope }
  | { kind: "error"; error: RuntimeSubscribeError };

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

export async function fetchAgentRunRuntimeInspect(
  target: AgentRunRuntimeTarget,
): Promise<AgentRunRuntimeInspectResponse> {
  return api.get<AgentRunRuntimeInspectResponse>(agentRunScopedPath(target, "/runtime"));
}

export async function fetchAgentRunRuntimeContext(
  target: AgentRunRuntimeTarget,
): Promise<RuntimeContextView> {
  return api.get<RuntimeContextView>(agentRunScopedPath(target, "/runtime/context"));
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
  response: InteractionResponse,
): Promise<OperationReceipt> {
  return api.post<OperationReceipt>(
    agentRunScopedPath(target, `/runtime/interactions/${encodeURIComponent(interactionId)}/respond`),
    response,
  );
}
