import { api } from "../api/client";
import { asRecord, requireStringField } from "../api/mappers";
import type {
  ExtensionBundleKindResponse,
  ExtensionBundleProjectionResponse,
  ExtensionCommandHandlerResponse,
  ExtensionCommandProjectionResponse,
  ExtensionFlagProjectionResponse,
  ExtensionFlagTypeResponse,
  ExtensionInstallationProjectionResponse,
  ExtensionInstalledAssetSourceResponse,
  ExtensionMessageRendererDeclarationResponse,
  ExtensionMessageRendererProjectionResponse,
  ExtensionPackageArtifactRefResponse,
  ExtensionPermissionAccessResponse,
  ExtensionPermissionDeclarationResponse,
  ExtensionPermissionProjectionResponse,
  ExtensionRuntimeActionKindResponse,
  ExtensionRuntimeActionProjectionResponse,
  ExtensionRuntimeInvocationOutputResponse,
  ExtensionRuntimeInvokeActionRequest,
  ExtensionRuntimeInvokeActionResponse,
  ExtensionRuntimeProjectionResponse,
  ExtensionRuntimeTraceResponse,
  ExtensionWorkspaceTabProjectionResponse,
  ExtensionWorkspaceTabRendererResponse,
  JsonValue,
} from "../generated/extension-runtime-contracts";
import { buildApiPath } from "../api/origin";

function recordOrThrow(raw: unknown, label: string): Record<string, unknown> {
  const value = asRecord(raw);
  if (!value || Array.isArray(raw)) {
    throw new Error(`${label} 不是对象`);
  }
  return value;
}

function optionalArray(raw: Record<string, unknown>, field: string): unknown[] {
  const value = raw[field];
  if (value == null) return [];
  if (!Array.isArray(value)) {
    throw new Error(`extension_runtime.${field} 不是数组`);
  }
  return value;
}

function requireNullableField(
  raw: Record<string, unknown>,
  field: string,
  label: string,
): unknown | null {
  if (!Object.prototype.hasOwnProperty.call(raw, field)) {
    throw new Error(`${label}.${field} 缺失`);
  }
  const value = raw[field];
  return value === null ? null : value;
}

function mapJsonValue(raw: unknown, label: string): JsonValue {
  if (raw == null) {
    return null;
  }
  if (typeof raw === "string" || typeof raw === "boolean") {
    return raw;
  }
  if (typeof raw === "number") {
    if (!Number.isFinite(raw)) {
      throw new Error(`${label} 包含非法数字`);
    }
    return raw;
  }
  if (Array.isArray(raw)) {
    return raw.map((item, index) => mapJsonValue(item, `${label}[${index}]`));
  }
  const record = recordOrThrow(raw, label);
  const result: { [key: string]: JsonValue } = {};
  for (const [key, value] of Object.entries(record)) {
    result[key] = mapJsonValue(value, `${label}.${key}`);
  }
  return result;
}

function mapActionKind(raw: unknown): ExtensionRuntimeActionKindResponse {
  if (raw === "session_runtime" || raw === "setup") return raw;
  throw new Error(`未知的 extension runtime action kind: ${String(raw ?? "")}`);
}

function mapFlagType(raw: unknown): ExtensionFlagTypeResponse {
  if (raw === "bool" || raw === "string") return raw;
  throw new Error(`未知的 extension flag type: ${String(raw ?? "")}`);
}

function mapPermissionAccess(raw: unknown): ExtensionPermissionAccessResponse {
  if (raw === "read" || raw === "write" || raw === "read_write") return raw;
  throw new Error(`未知的 extension permission access: ${String(raw ?? "")}`);
}

function mapBundleKind(raw: unknown): ExtensionBundleKindResponse {
  if (raw === "extension_host") return raw;
  throw new Error(`未知的 extension bundle kind: ${String(raw ?? "")}`);
}

function mapInstalledSource(raw: unknown): ExtensionInstalledAssetSourceResponse {
  const value = recordOrThrow(raw, "extension installed_source");
  return {
    library_asset_id: requireStringField(value, "library_asset_id"),
    source_ref: requireStringField(value, "source_ref"),
    source_version: requireStringField(value, "source_version"),
    source_digest: requireStringField(value, "source_digest"),
    installed_at: requireStringField(value, "installed_at"),
  };
}

function mapPackageArtifactRef(raw: unknown): ExtensionPackageArtifactRefResponse {
  const value = recordOrThrow(raw, "extension package_artifact");
  return {
    artifact_id: requireStringField(value, "artifact_id"),
    package_name: requireStringField(value, "package_name"),
    package_version: requireStringField(value, "package_version"),
    asset_version: requireStringField(value, "asset_version"),
    source_version: requireStringField(value, "source_version"),
    storage_ref: requireStringField(value, "storage_ref"),
    archive_digest: requireStringField(value, "archive_digest"),
    manifest_digest: requireStringField(value, "manifest_digest"),
  };
}

function mapCommandHandler(raw: unknown): ExtensionCommandHandlerResponse {
  const value = recordOrThrow(raw, "extension command handler");
  switch (value.kind) {
    case "inject_message":
      return {
        kind: "inject_message",
        content: requireStringField(value, "content"),
      };
    default:
      throw new Error(`未知的 extension command handler kind: ${String(value.kind ?? "")}`);
  }
}

function mapMessageRenderer(raw: unknown): ExtensionMessageRendererDeclarationResponse {
  const value = recordOrThrow(raw, "extension message renderer");
  switch (value.kind) {
    case "json_card":
      return { kind: "json_card" };
    case "markdown":
      return { kind: "markdown" };
    default:
      throw new Error(`未知的 extension message renderer kind: ${String(value.kind ?? "")}`);
  }
}

function mapWorkspaceTabRenderer(raw: unknown): ExtensionWorkspaceTabRendererResponse {
  const value = recordOrThrow(raw, "extension workspace tab renderer");
  switch (value.kind) {
    case "webview":
      return {
        kind: "webview",
        entry: requireStringField(value, "entry"),
      };
    default:
      throw new Error(`未知的 extension workspace tab renderer kind: ${String(value.kind ?? "")}`);
  }
}

function mapPermission(raw: unknown): ExtensionPermissionDeclarationResponse {
  const value = recordOrThrow(raw, "extension permission");
  switch (value.kind) {
    case "local_profile":
      return {
        kind: "local_profile",
        access: mapPermissionAccess(value.access),
      };
    case "workspace":
      return {
        kind: "workspace",
        access: mapPermissionAccess(value.access),
      };
    case "runtime_action":
      return {
        kind: "runtime_action",
        action_key: requireStringField(value, "action_key"),
      };
    default:
      throw new Error(`未知的 extension permission kind: ${String(value.kind ?? "")}`);
  }
}

function mapInstallation(raw: unknown): ExtensionInstallationProjectionResponse {
  const value = recordOrThrow(raw, "extension installation");
  const installedSource = requireNullableField(value, "installed_source", "extension installation");
  const packageArtifact = requireNullableField(value, "package_artifact", "extension installation");
  return {
    installation_id: requireStringField(value, "installation_id"),
    extension_key: requireStringField(value, "extension_key"),
    extension_id: requireStringField(value, "extension_id"),
    display_name: requireStringField(value, "display_name"),
    installed_source: installedSource === null ? null : mapInstalledSource(installedSource),
    package_artifact: packageArtifact === null ? null : mapPackageArtifactRef(packageArtifact),
  };
}

function mapCommand(raw: unknown): ExtensionCommandProjectionResponse {
  const value = recordOrThrow(raw, "extension command");
  return {
    extension_key: requireStringField(value, "extension_key"),
    extension_id: requireStringField(value, "extension_id"),
    name: requireStringField(value, "name"),
    description: requireStringField(value, "description"),
    handler: mapCommandHandler(value.handler),
  };
}

function mapFlag(raw: unknown): ExtensionFlagProjectionResponse {
  const value = recordOrThrow(raw, "extension flag");
  return {
    extension_key: requireStringField(value, "extension_key"),
    extension_id: requireStringField(value, "extension_id"),
    name: requireStringField(value, "name"),
    flag_type: mapFlagType(value.flag_type),
    default: mapJsonValue(value.default, "extension flag default"),
    description: requireStringField(value, "description"),
  };
}

function mapMessageRendererProjection(raw: unknown): ExtensionMessageRendererProjectionResponse {
  const value = recordOrThrow(raw, "extension message renderer projection");
  return {
    extension_key: requireStringField(value, "extension_key"),
    extension_id: requireStringField(value, "extension_id"),
    custom_type: requireStringField(value, "custom_type"),
    renderer: mapMessageRenderer(value.renderer),
  };
}

function mapRuntimeAction(raw: unknown): ExtensionRuntimeActionProjectionResponse {
  const value = recordOrThrow(raw, "extension runtime action");
  const permissions = optionalArray(value, "permissions").map((permission) => {
    if (typeof permission !== "string" || permission.trim() === "") {
      throw new Error("extension runtime action permission 非法");
    }
    return permission;
  });
  return {
    extension_key: requireStringField(value, "extension_key"),
    extension_id: requireStringField(value, "extension_id"),
    action_key: requireStringField(value, "action_key"),
    kind: mapActionKind(value.kind),
    description: requireStringField(value, "description"),
    input_schema: mapJsonValue(value.input_schema, "extension runtime action input_schema"),
    output_schema: mapJsonValue(value.output_schema, "extension runtime action output_schema"),
    permissions,
  };
}

function mapWorkspaceTab(raw: unknown): ExtensionWorkspaceTabProjectionResponse {
  const value = recordOrThrow(raw, "extension workspace tab");
  return {
    extension_key: requireStringField(value, "extension_key"),
    extension_id: requireStringField(value, "extension_id"),
    type_id: requireStringField(value, "type_id"),
    label: requireStringField(value, "label"),
    uri_scheme: requireStringField(value, "uri_scheme"),
    renderer: mapWorkspaceTabRenderer(value.renderer),
  };
}

function mapPermissionProjection(raw: unknown): ExtensionPermissionProjectionResponse {
  const value = recordOrThrow(raw, "extension permission projection");
  return {
    extension_key: requireStringField(value, "extension_key"),
    extension_id: requireStringField(value, "extension_id"),
    permission: mapPermission(value.permission),
  };
}

function mapBundle(raw: unknown): ExtensionBundleProjectionResponse {
  const value = recordOrThrow(raw, "extension bundle");
  return {
    extension_key: requireStringField(value, "extension_key"),
    extension_id: requireStringField(value, "extension_id"),
    kind: mapBundleKind(value.kind),
    entry: requireStringField(value, "entry"),
    digest: requireStringField(value, "digest"),
  };
}

export function mapExtensionRuntimeProjection(raw: unknown): ExtensionRuntimeProjectionResponse {
  const value = raw == null ? {} : recordOrThrow(raw, "extension_runtime");
  return {
    installations: optionalArray(value, "installations").map(mapInstallation),
    commands: optionalArray(value, "commands").map(mapCommand),
    flags: optionalArray(value, "flags").map(mapFlag),
    message_renderers: optionalArray(value, "message_renderers").map(mapMessageRendererProjection),
    runtime_actions: optionalArray(value, "runtime_actions").map(mapRuntimeAction),
    workspace_tabs: optionalArray(value, "workspace_tabs").map(mapWorkspaceTab),
    permissions: optionalArray(value, "permissions").map(mapPermissionProjection),
    bundles: optionalArray(value, "bundles").map(mapBundle),
  };
}

export async function fetchProjectExtensionRuntime(
  projectId: string,
): Promise<ExtensionRuntimeProjectionResponse> {
  const raw = await api.get<unknown>(
    `/projects/${encodeURIComponent(projectId)}/extension-runtime`,
  );
  return mapExtensionRuntimeProjection(raw);
}

function mapRuntimeTrace(raw: unknown): ExtensionRuntimeTraceResponse {
  const value = recordOrThrow(raw, "extension runtime trace");
  return {
    trace_id: requireStringField(value, "trace_id"),
    invocation_id: requireStringField(value, "invocation_id"),
    parent_trace_id: value.parent_trace_id == null ? null : String(value.parent_trace_id),
    created_at: requireStringField(value, "created_at"),
  };
}

function mapInvocationMetadata(raw: unknown): { [key: string]: JsonValue } {
  if (raw == null) return {};
  const value = recordOrThrow(raw, "extension runtime invocation metadata");
  const metadata: { [key: string]: JsonValue } = {};
  for (const [key, item] of Object.entries(value)) {
    metadata[key] = mapJsonValue(item, `extension runtime invocation metadata.${key}`);
  }
  return metadata;
}

function mapInvocationOutput(raw: unknown): ExtensionRuntimeInvocationOutputResponse {
  const value = recordOrThrow(raw, "extension runtime invocation output");
  return {
    output: mapJsonValue(value.output, "extension runtime invocation output.output"),
    metadata: mapInvocationMetadata(value.metadata),
  };
}

export function mapExtensionRuntimeInvokeActionResponse(
  raw: unknown,
): ExtensionRuntimeInvokeActionResponse {
  const value = recordOrThrow(raw, "extension runtime invoke action response");
  return {
    action_key: requireStringField(value, "action_key"),
    trace: mapRuntimeTrace(value.trace),
    output: mapInvocationOutput(value.output),
  };
}

export async function invokeProjectExtensionRuntimeAction(
  projectId: string,
  request: ExtensionRuntimeInvokeActionRequest,
): Promise<ExtensionRuntimeInvokeActionResponse> {
  const raw = await api.post<unknown>(
    `/projects/${encodeURIComponent(projectId)}/extension-runtime/invoke-action`,
    request,
  );
  return mapExtensionRuntimeInvokeActionResponse(raw);
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
