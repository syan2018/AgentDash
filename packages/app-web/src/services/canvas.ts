import { api } from "../api/client";
import type { AgentRunMessageCommandResponse } from "../generated/agent-run-interaction-contracts";
import type { AgentInputContent } from "../generated/agent-service-api";
import type { JsonValue } from "../generated/common-contracts";
import type {
  CanvasAgentInputSubmitRequest,
  CanvasAgentRunRuntimeSnapshotDto,
  CanvasInteractionEventDto,
  CanvasInteractionSnapshot,
  CanvasInteractionSnapshotUpsertRequest,
  CanvasRuntimeDiagnosticDto,
  CanvasRuntimeBindingUpsertRequest,
  CanvasRuntimeInvokeRequest,
  CanvasRuntimeObservation,
  CanvasRuntimeObservationUpsertRequest,
} from "../generated/canvas-contracts";
import type { ExtensionPackageInstallationResponse } from "../generated/extension-package-contracts";
import type {
  Canvas,
  CanvasListScope,
  CanvasRuntimeSnapshot,
  CopyCanvasToPersonalInput,
  CreateCanvasInput,
  DeleteCanvasResult,
  PublishCanvasToProjectInput,
  RuntimeInvocationResult,
  UnpublishCanvasResult,
  UpdateCanvasInput,
} from "../types";

export interface AgentRunCanvasBridgeIdentity {
  run_id: string;
  agent_id: string;
  canvas_mount_id: string;
  project_id: string;
}

export type CanvasRuntimeDiagnosticEntry = CanvasRuntimeDiagnosticDto;

export type UploadCanvasRenderObservationInput = CanvasRuntimeObservationUpsertRequest;

export type CanvasInteractionEventInput = CanvasInteractionEventDto;

export type UploadCanvasInteractionSnapshotInput = CanvasInteractionSnapshotUpsertRequest;

export interface SubmitCanvasAgentInput {
  text?: string;
  input?: AgentInputContent[];
  include_interaction_state?: boolean;
  include_render_observation?: boolean;
  delivery_intent?: "queue" | "steer";
  client_command_id?: string;
  interaction_snapshot_id?: string;
  render_observation_id?: string;
}

export interface CanvasRuntimeInvokeInput extends Omit<CanvasRuntimeInvokeRequest, "input"> {
  input?: JsonValue;
}

export type UpsertCanvasRuntimeBindingInput = CanvasRuntimeBindingUpsertRequest & {
  alias: string;
};

function agentRunCanvasPath(
  bridge: AgentRunCanvasBridgeIdentity,
  route: string,
): string {
  return `/agent-runs/${encodeURIComponent(bridge.run_id)}`
    + `/agents/${encodeURIComponent(bridge.agent_id)}`
    + `/canvases/${encodeURIComponent(bridge.canvas_mount_id)}${route}`;
}

export async function fetchProjectCanvases(
  projectId: string,
  scope?: CanvasListScope,
): Promise<Canvas[]> {
  const params = new URLSearchParams();
  if (scope) {
    params.set("scope", scope);
  }
  const query = params.toString();
  return api.get<Canvas[]>(
    query
      ? `/projects/${encodeURIComponent(projectId)}/canvases?${query}`
      : `/projects/${encodeURIComponent(projectId)}/canvases`,
  );
}

export async function createCanvas(
  projectId: string,
  input: CreateCanvasInput,
): Promise<Canvas> {
  return api.post<Canvas>(
    `/projects/${encodeURIComponent(projectId)}/canvases`,
    input,
  );
}

export async function fetchCanvas(canvasId: string): Promise<Canvas> {
  return api.get<Canvas>(`/canvases/${encodeURIComponent(canvasId)}`);
}

export async function fetchCanvasByMountId(
  projectId: string,
  canvasMountId: string,
): Promise<Canvas> {
  return api.get<Canvas>(
    `/projects/${encodeURIComponent(projectId)}/canvases/by-mount/${encodeURIComponent(canvasMountId)}`,
  );
}

export async function updateCanvas(
  canvasId: string,
  input: UpdateCanvasInput,
): Promise<Canvas> {
  return api.put<Canvas>(
    `/canvases/${encodeURIComponent(canvasId)}`,
    input,
  );
}

export async function deleteCanvas(canvasId: string): Promise<DeleteCanvasResult> {
  return api.delete<DeleteCanvasResult>(`/canvases/${encodeURIComponent(canvasId)}`);
}

export async function publishCanvasToProject(
  canvasId: string,
  input: PublishCanvasToProjectInput = {},
): Promise<Canvas> {
  return api.post<Canvas>(
    `/canvases/${encodeURIComponent(canvasId)}/publish-to-project`,
    input,
  );
}

export async function copyCanvasToPersonal(
  canvasId: string,
  input: CopyCanvasToPersonalInput = {},
): Promise<Canvas> {
  return api.post<Canvas>(
    `/canvases/${encodeURIComponent(canvasId)}/copy-to-personal`,
    input,
  );
}

export async function unpublishCanvas(canvasId: string): Promise<UnpublishCanvasResult> {
  return api.post<UnpublishCanvasResult>(
    `/canvases/${encodeURIComponent(canvasId)}/unpublish`,
    {},
  );
}

export async function fetchCanvasRuntimeSnapshot(
  canvasId: string,
): Promise<CanvasRuntimeSnapshot> {
  return api.get<CanvasRuntimeSnapshot>(
    `/canvases/${encodeURIComponent(canvasId)}/runtime-snapshot`,
  );
}

export interface PromoteCanvasToExtensionInput {
  extension_key?: string;
  display_name?: string;
  package_version?: string;
  asset_version?: string;
  overwrite?: boolean;
}

export async function invokeCanvasRuntimeAction(
  bridge: AgentRunCanvasBridgeIdentity,
  input: CanvasRuntimeInvokeInput,
): Promise<RuntimeInvocationResult> {
  const request: CanvasRuntimeInvokeRequest = {
    action_key: input.action_key,
    input: input.input ?? {},
  };
  return api.post<RuntimeInvocationResult>(
    agentRunCanvasPath(bridge, "/runtime-invoke"),
    request,
  );
}

export async function fetchAgentRunCanvasRuntimeSnapshot(
  bridge: AgentRunCanvasBridgeIdentity,
): Promise<CanvasAgentRunRuntimeSnapshotDto> {
  return api.get<CanvasAgentRunRuntimeSnapshotDto>(agentRunCanvasPath(bridge, "/runtime-snapshot"));
}

export async function upsertAgentRunCanvasRuntimeBinding(
  bridge: AgentRunCanvasBridgeIdentity,
  input: UpsertCanvasRuntimeBindingInput,
): Promise<CanvasAgentRunRuntimeSnapshotDto> {
  return api.put<CanvasAgentRunRuntimeSnapshotDto>(
    agentRunCanvasPath(bridge, `/runtime-bindings/${encodeURIComponent(input.alias)}`),
    {
      source_uri: input.source_uri,
      content_type: input.content_type,
    },
  );
}

export async function uploadCanvasRenderObservation(
  bridge: AgentRunCanvasBridgeIdentity,
  input: UploadCanvasRenderObservationInput,
): Promise<CanvasRuntimeObservation> {
  return api.post<CanvasRuntimeObservation>(agentRunCanvasPath(bridge, "/runtime-observation"), input);
}

export async function uploadCanvasInteractionSnapshot(
  bridge: AgentRunCanvasBridgeIdentity,
  input: UploadCanvasInteractionSnapshotInput,
): Promise<CanvasInteractionSnapshot> {
  return api.post<CanvasInteractionSnapshot>(agentRunCanvasPath(bridge, "/interaction-snapshot"), input);
}

export async function submitCanvasAgentInput(
  bridge: AgentRunCanvasBridgeIdentity,
  input: SubmitCanvasAgentInput,
): Promise<AgentRunMessageCommandResponse> {
  const request = toCanvasAgentInputSubmitRequest(input);
  return api.post<AgentRunMessageCommandResponse>(
    agentRunCanvasPath(bridge, "/agent-input-submit"),
    request,
  );
}

export async function promoteCanvasToExtension(
  canvasId: string,
  input: PromoteCanvasToExtensionInput = {},
): Promise<ExtensionPackageInstallationResponse> {
  return api.post<ExtensionPackageInstallationResponse>(
    `/canvases/${encodeURIComponent(canvasId)}/promote-extension`,
    input,
  );
}

function toCanvasAgentInputSubmitRequest(input: SubmitCanvasAgentInput): CanvasAgentInputSubmitRequest {
  const userInput = normalizeCanvasAgentInput(input);
  if (userInput.length === 0) {
    throw new Error("Canvas Agent submit 需要 input 或 text");
  }
  return {
    input: userInput,
    client_command_id: input.client_command_id ?? createCanvasClientCommandId(),
    delivery_intent: input.delivery_intent,
    interaction_snapshot_id: input.interaction_snapshot_id,
    render_observation_id: input.render_observation_id,
  };
}

function normalizeCanvasAgentInput(input: SubmitCanvasAgentInput): AgentInputContent[] {
  if (input.input && input.input.length > 0) {
    return input.input;
  }
  const text = input.text?.trim();
  if (!text) {
    return [];
  }
  return [{ kind: "text", text }];
}

function createCanvasClientCommandId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `canvas-agent-${crypto.randomUUID()}`;
  }
  return `canvas-agent-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
