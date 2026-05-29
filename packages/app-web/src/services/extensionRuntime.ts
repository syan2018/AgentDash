import { buildApiPath } from "../api/origin";
import { api } from "../api/client";
import type {
  ExtensionRuntimeInvokeActionRequest,
  ExtensionRuntimeInvokeActionResponse,
  ExtensionRuntimeInvokeChannelRequest,
  ExtensionRuntimeInvokeChannelResponse,
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

export async function invokeProjectExtensionRuntimeAction(
  projectId: string,
  request: ExtensionRuntimeInvokeActionRequest,
): Promise<ExtensionRuntimeInvokeActionResponse> {
  return api.post<ExtensionRuntimeInvokeActionResponse>(
    `/projects/${encodeURIComponent(projectId)}/extension-runtime/invoke-action`,
    request,
  );
}

export async function invokeProjectExtensionRuntimeChannel(
  projectId: string,
  request: ExtensionRuntimeInvokeChannelRequest,
): Promise<ExtensionRuntimeInvokeChannelResponse> {
  return api.post<ExtensionRuntimeInvokeChannelResponse>(
    `/projects/${encodeURIComponent(projectId)}/extension-runtime/invoke-channel`,
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
