import type {
  ExtensionBundleProjectionResponse,
  ExtensionCommandProjectionResponse,
  ExtensionFlagProjectionResponse,
  ExtensionInstallationProjectionResponse,
  ExtensionMessageRendererProjectionResponse,
  ExtensionPermissionDeclarationResponse,
  ExtensionPermissionProjectionResponse,
  ExtensionRuntimeActionProjectionResponse,
  ExtensionRuntimeProjectionResponse,
  ExtensionWorkspaceTabProjectionResponse,
} from "../../../../generated/extension-runtime-contracts";

export type InstalledExtensionSource =
  | "marketplace"
  | "local_archive"
  | "marketplace_with_archive"
  | "unknown";

export interface InstalledExtensionRowVM {
  installation: ExtensionInstallationProjectionResponse;
  source: InstalledExtensionSource;
  version: string;
  permissions: ExtensionPermissionDeclarationResponse[];
  workspaceTabs: ExtensionWorkspaceTabProjectionResponse[];
  runtimeActions: ExtensionRuntimeActionProjectionResponse[];
  commands: ExtensionCommandProjectionResponse[];
  flags: ExtensionFlagProjectionResponse[];
  messageRenderers: ExtensionMessageRendererProjectionResponse[];
  bundle: ExtensionBundleProjectionResponse | null;
}

export function classifyInstallationSource(
  installation: ExtensionInstallationProjectionResponse,
): InstalledExtensionSource {
  const hasInstalledSource = installation.installed_source != null;
  const hasArchive = installation.package_artifact != null;
  if (hasInstalledSource && hasArchive) return "marketplace_with_archive";
  if (hasInstalledSource) return "marketplace";
  if (hasArchive) return "local_archive";
  return "unknown";
}

export function resolveInstallationVersion(
  installation: ExtensionInstallationProjectionResponse,
): string {
  if (installation.package_artifact?.package_version) {
    return installation.package_artifact.package_version;
  }
  if (installation.installed_source?.source_version) {
    return installation.installed_source.source_version;
  }
  return "";
}

export function aggregateInstalledExtensions(
  projection: ExtensionRuntimeProjectionResponse,
): InstalledExtensionRowVM[] {
  return projection.installations.map((installation) => {
    const key = installation.extension_key;
    const permissions = projection.permissions
      .filter((entry) => entry.extension_key === key)
      .map((entry: ExtensionPermissionProjectionResponse) => entry.permission);
    const workspaceTabs = projection.workspace_tabs.filter(
      (entry) => entry.extension_key === key,
    );
    const runtimeActions = projection.runtime_actions.filter(
      (entry) => entry.extension_key === key,
    );
    const commands = projection.commands.filter(
      (entry) => entry.extension_key === key,
    );
    const flags = projection.flags.filter(
      (entry) => entry.extension_key === key,
    );
    const messageRenderers = projection.message_renderers.filter(
      (entry) => entry.extension_key === key,
    );
    const bundle = projection.bundles.find((entry) => entry.extension_key === key) ?? null;
    return {
      installation,
      source: classifyInstallationSource(installation),
      version: resolveInstallationVersion(installation),
      permissions,
      workspaceTabs,
      runtimeActions,
      commands,
      flags,
      messageRenderers,
      bundle,
    };
  });
}
