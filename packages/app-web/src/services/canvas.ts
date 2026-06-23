import { api } from "../api/client";
import type { ExtensionPackageInstallationResponse } from "../generated/extension-package-contracts";
import type {
  Canvas,
  CanvasRuntimeSnapshot,
  CreateCanvasInput,
  DeleteCanvasResult,
  RuntimeInvocationResult,
  UpdateCanvasInput,
} from "../types";

export async function fetchProjectCanvases(projectId: string): Promise<Canvas[]> {
  return api.get<Canvas[]>(
    `/projects/${encodeURIComponent(projectId)}/canvases`,
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

export async function fetchCanvasRuntimeSnapshot(
  canvasId: string,
  sessionId?: string | null,
): Promise<CanvasRuntimeSnapshot> {
  const params = new URLSearchParams();
  if (sessionId) {
    params.set("session_id", sessionId);
  }
  const query = params.toString();
  return api.get<CanvasRuntimeSnapshot>(
    query
      ? `/canvases/${encodeURIComponent(canvasId)}/runtime-snapshot?${query}`
      : `/canvases/${encodeURIComponent(canvasId)}/runtime-snapshot`,
  );
}

export interface CanvasRuntimeInvokeInput {
  session_id: string;
  action_key: string;
  input?: unknown;
}

export interface PromoteCanvasToExtensionInput {
  extension_key?: string;
  display_name?: string;
  package_version?: string;
  asset_version?: string;
  overwrite?: boolean;
}

export async function invokeCanvasRuntimeAction(
  canvasId: string,
  input: CanvasRuntimeInvokeInput,
): Promise<RuntimeInvocationResult> {
  return api.post<RuntimeInvocationResult>(
    `/canvases/${encodeURIComponent(canvasId)}/runtime-invoke`,
    input,
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
