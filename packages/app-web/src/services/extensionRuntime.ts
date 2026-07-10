import { buildApiPath } from "../api/origin";
import { api } from "../api/client";
import type {
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

export function buildExactExtensionComponentAssetUrl(
  projectId: string,
  artifactId: string,
  archiveDigest: string,
  componentKey: string,
  assetPath: string,
): string {
  const digestHex = archiveDigest.startsWith("sha256:")
    ? archiveDigest.slice("sha256:".length)
    : archiveDigest;
  if (!/^[a-fA-F0-9]{64}$/.test(digestHex)) {
    throw new Error("Extension component archive digest 必须为 sha256");
  }
  const encodedAssetPath = assetPath
    .split("/")
    .filter((segment) => segment.trim() !== "")
    .map((segment) => encodeURIComponent(segment))
    .join("/");
  return buildApiPath(
    `/projects/${encodeURIComponent(projectId)}/extension-runtime/artifacts/${encodeURIComponent(artifactId)}/${digestHex}/components/${encodeURIComponent(componentKey)}/${encodedAssetPath}`,
  );
}
