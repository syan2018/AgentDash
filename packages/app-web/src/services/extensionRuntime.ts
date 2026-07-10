import { buildApiPath } from "../api/origin";
import { api } from "../api/client";
import {
  agentRunScopedPath,
  type AgentRunRuntimeTarget,
} from "./agentRunRuntime";
import type {
  ExtensionRuntimeInvokeActionRequest,
  ExtensionRuntimeInvokeActionResponse,
  ExtensionRuntimeInvokeBackendServiceRequest,
  ExtensionRuntimeInvokeBackendServiceResponse,
  ExtensionRuntimeInvokeProtocolRequest,
  ExtensionRuntimeInvokeProtocolResponse,
  ExtensionRuntimeProjectionResponse,
  UninstallExtensionInstallationResponse,
} from "../generated/extension-runtime-contracts";

export async function fetchProjectExtensionRuntime(
  projectId: string,
): Promise<ExtensionRuntimeProjectionResponse> {
  return api.get<ExtensionRuntimeProjectionResponse>(
    `/projects/${encodeURIComponent(projectId)}/extension-runtime`,
  );
}

export async function invokeAgentRunExtensionRuntimeAction(
  target: AgentRunRuntimeTarget,
  request: ExtensionRuntimeInvokeActionRequest,
): Promise<ExtensionRuntimeInvokeActionResponse> {
  return api.post<ExtensionRuntimeInvokeActionResponse>(
    agentRunScopedPath(target, "/extension-runtime/invoke-action"),
    request,
  );
}

export async function invokeAgentRunExtensionRuntimeProtocol(
  target: AgentRunRuntimeTarget,
  request: ExtensionRuntimeInvokeProtocolRequest,
): Promise<ExtensionRuntimeInvokeProtocolResponse> {
  return api.post<ExtensionRuntimeInvokeProtocolResponse>(
    agentRunScopedPath(target, "/extension-runtime/invoke-protocol"),
    request,
  );
}

export async function invokeAgentRunExtensionBackendService(
  target: AgentRunRuntimeTarget,
  request: ExtensionRuntimeInvokeBackendServiceRequest,
): Promise<ExtensionRuntimeInvokeBackendServiceResponse> {
  return api.post<ExtensionRuntimeInvokeBackendServiceResponse>(
    agentRunScopedPath(target, "/extension-runtime/invoke-backend-service"),
    request,
  );
}

export async function uninstallExtensionInstallation(
  projectId: string,
  installationId: string,
): Promise<UninstallExtensionInstallationResponse> {
  return api.delete<UninstallExtensionInstallationResponse>(
    `/projects/${encodeURIComponent(projectId)}/extensions/${encodeURIComponent(installationId)}`,
  );
}

export function buildExtensionWebviewAssetUrl(
  projectId: string,
  extensionKey: string,
  assetPath: string,
): string {
  const encodedAssetPath = assetPath
    .split("/")
    .filter((segment) => segment.trim() !== "")
    .map((segment) => encodeURIComponent(segment))
    .join("/");
  return buildApiPath(
    `/projects/${encodeURIComponent(projectId)}/extension-runtime/webviews/${encodeURIComponent(extensionKey)}/${encodedAssetPath}`,
  );
}
