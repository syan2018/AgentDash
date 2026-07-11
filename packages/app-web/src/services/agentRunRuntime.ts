import { api } from "../api/client";
import type { AgentRunRuntimeCommandRequest } from "../generated/workflow-contracts";
import type {
  BoundRuntimeHookPlan,
  DriverThreadId,
  ProfileDigest,
  ProfileProvenance,
  RuntimeBindingId,
  RuntimeDriverGeneration,
  RuntimeEventEnvelope,
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
  | { kind: "event"; durable_cursor: number | null; envelope: RuntimeEventEnvelope }
  | { kind: "error"; error: RuntimeSubscribeError };

export function agentRunScopedPath(target: AgentRunRuntimeTarget, route: string): string {
  return `/agent-runs/${encodeURIComponent(target.runId)}/agents/${encodeURIComponent(target.agentId)}${route}`;
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

export async function compactAgentRunContext(
  runId: string,
  agentId: string,
  request: AgentRunRuntimeCommandRequest,
): Promise<OperationReceipt> {
  return api.post<OperationReceipt>(
    agentRunScopedPath({ runId, agentId }, "/runtime/context/compact"),
    request,
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
