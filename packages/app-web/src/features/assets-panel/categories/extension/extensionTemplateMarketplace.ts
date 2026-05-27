import type { LibraryAssetDto } from "../../../../types";

export interface ExtensionTemplateMarketplaceParsed {
  manifestVersion: string;
  extensionId: string;
  commands: Array<{ name: string; description: string; handlerKind: string }>;
  flags: Array<{ name: string; type: string; defaultValue: string; description: string }>;
  renderers: Array<{ customType: string; kind: string }>;
  runtimeActions: Array<{ name: string; kind: string; description: string | null }>;
  protocolChannels: string[];
  workspaceTabs: string[];
  bundles: string[];
  requiresPackageArtifact: boolean;
}

export function parseExtensionTemplateMarketplacePayload(
  raw: unknown,
): ExtensionTemplateMarketplaceParsed | null {
  if (!isObject(raw)) return null;
  const manifestVersion = asString(raw.manifest_version);
  const extensionId = asString(raw.extension_id);
  if (!manifestVersion || !extensionId) return null;
  const commandsRaw = Array.isArray(raw.commands) ? raw.commands : [];
  const commands = commandsRaw.flatMap((item) => {
    if (!isObject(item)) return [];
    const name = asString(item.name);
    if (!name) return [];
    const handler = isObject(item.handler) ? item.handler : null;
    return [
      {
        name,
        description: asString(item.description) ?? "",
        handlerKind: handler ? (asString(handler.kind) ?? "unknown") : "unknown",
      },
    ];
  });
  const flagsRaw = Array.isArray(raw.flags) ? raw.flags : [];
  const flags = flagsRaw.flatMap((item) => {
    if (!isObject(item)) return [];
    const name = asString(item.name);
    if (!name) return [];
    return [
      {
        name,
        type: asString(item.type) ?? "unknown",
        defaultValue: stringifyLite(item.default),
        description: asString(item.description) ?? "",
      },
    ];
  });
  const renderersRaw = Array.isArray(raw.message_renderers) ? raw.message_renderers : [];
  const renderers = renderersRaw.flatMap((item) => {
    if (!isObject(item)) return [];
    const customType = asString(item.custom_type);
    const renderer = isObject(item.renderer) ? item.renderer : null;
    if (!customType) return [];
    return [{ customType, kind: renderer ? (asString(renderer.kind) ?? "unknown") : "unknown" }];
  });
  const runtimeActionsRaw = Array.isArray(raw.runtime_actions) ? raw.runtime_actions : [];
  const runtimeActions = runtimeActionsRaw.flatMap((item) => {
    if (!isObject(item)) return [];
    const name = asString(item.name);
    if (!name) return [];
    return [
      {
        name,
        kind: asString(item.kind) ?? "runtime",
        description: asString(item.description),
      },
    ];
  });
  const protocolChannels = stringKeysFromArray(raw.protocol_channels, "name");
  const workspaceTabs = stringKeysFromArray(raw.workspace_tabs, "id");
  const bundles = stringKeysFromArray(raw.bundles, "id");
  return {
    manifestVersion,
    extensionId,
    commands,
    flags,
    renderers,
    runtimeActions,
    protocolChannels,
    workspaceTabs,
    bundles,
    requiresPackageArtifact:
      runtimeActions.length > 0 ||
      protocolChannels.length > 0 ||
      workspaceTabs.length > 0 ||
      bundles.length > 0,
  };
}

export function getMarketplaceInstallBlocker(asset: LibraryAssetDto): string | null {
  if (asset.asset_type !== "extension_template") return null;
  const parsed = parseExtensionTemplateMarketplacePayload(asset.payload);
  if (!parsed?.requiresPackageArtifact) return null;
  if (asset.extension_package_artifact) return null;
  return "Extension 模板缺少 package_artifact，无法安装到项目";
}

function stringKeysFromArray(raw: unknown, preferredKey: string): string[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((item, index) => {
    if (!isObject(item)) return [];
    const key = asString(item[preferredKey]) ?? asString(item.name) ?? asString(item.id);
    return [key ?? `#${index + 1}`];
  });
}

function stringifyLite(value: unknown): string {
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "boolean" || typeof value === "number") return String(value);
  if (value == null) return "null";
  return "json";
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function asString(v: unknown): string | null {
  return typeof v === "string" ? v : null;
}
