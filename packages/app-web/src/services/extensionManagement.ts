import { api } from "../api/client";
import { asRecord, requireNumberField, requireStringField } from "../api/mappers";
import type { JsonValue } from "../generated/common-contracts";
import type {
  ProjectExtensionCapabilitySummaryResponse,
  ProjectExtensionInstalledSourceResponse,
  ProjectExtensionManagementItemResponse,
  ProjectExtensionManagementListResponse,
  ProjectExtensionPackageArtifactRefResponse,
  ProjectExtensionPackageModeResponse,
} from "../generated/extension-management-contracts";

function recordOrThrow(raw: unknown, label: string): Record<string, unknown> {
  const value = asRecord(raw);
  if (!value || Array.isArray(raw)) {
    throw new Error(`${label} 不是对象`);
  }
  return value;
}

function nullableField(raw: Record<string, unknown>, field: string): unknown | null {
  return raw[field] == null ? null : raw[field];
}

function optionalStringField(raw: Record<string, unknown>, field: string): string | undefined {
  const value = raw[field];
  if (value == null) {
    return undefined;
  }
  if (typeof value !== "string") {
    throw new Error(`缺少或非法的字段 ${field}`);
  }
  return value;
}

function requireBooleanField(raw: Record<string, unknown>, field: string): boolean {
  const value = raw[field];
  if (typeof value !== "boolean") {
    throw new Error(`缺少或非法的布尔字段 ${field}`);
  }
  return value;
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

function mapPackageMode(raw: unknown): ProjectExtensionPackageModeResponse {
  if (
    raw === "packaged" ||
    raw === "declaration_only" ||
    raw === "invalid_missing_artifact"
  ) {
    return raw;
  }
  throw new Error("Project Extension package_mode 非法");
}

function mapInstalledSource(raw: unknown): ProjectExtensionInstalledSourceResponse {
  const value = recordOrThrow(raw, "Project Extension installed_source");
  return {
    library_asset_id: requireStringField(value, "library_asset_id"),
    source_ref: requireStringField(value, "source_ref"),
    source_version: requireStringField(value, "source_version"),
    source_digest: requireStringField(value, "source_digest"),
    installed_at: requireStringField(value, "installed_at"),
  };
}

function mapPackageArtifact(
  raw: unknown,
): ProjectExtensionPackageArtifactRefResponse {
  const value = recordOrThrow(raw, "Project Extension package_artifact");
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

function mapCapabilitySummary(
  raw: unknown,
): ProjectExtensionCapabilitySummaryResponse {
  const value = recordOrThrow(raw, "Project Extension capability_summary");
  return {
    commands: requireNumberField(value, "commands"),
    flags: requireNumberField(value, "flags"),
    message_renderers: requireNumberField(value, "message_renderers"),
    runtime_actions: requireNumberField(value, "runtime_actions"),
    protocol_channels: requireNumberField(value, "protocol_channels"),
    workspace_tabs: requireNumberField(value, "workspace_tabs"),
    permissions: requireNumberField(value, "permissions"),
    bundles: requireNumberField(value, "bundles"),
  };
}

function mapManagementItem(raw: unknown): ProjectExtensionManagementItemResponse {
  const value = recordOrThrow(raw, "Project Extension management item");
  const installedSource = nullableField(value, "installed_source");
  const packageArtifact = nullableField(value, "package_artifact");
  const sourceStatus = nullableField(value, "source_status");
  if (sourceStatus !== null && typeof sourceStatus !== "string") {
    throw new Error("Project Extension source_status 非法");
  }
  return {
    installation_id: requireStringField(value, "installation_id"),
    extension_key: requireStringField(value, "extension_key"),
    extension_id: requireStringField(value, "extension_id"),
    display_name: requireStringField(value, "display_name"),
    enabled: requireBooleanField(value, "enabled"),
    installed_source: installedSource === null ? null : mapInstalledSource(installedSource),
    source_status: sourceStatus,
    current_source_version: optionalStringField(value, "current_source_version"),
    current_source_digest: optionalStringField(value, "current_source_digest"),
    package_mode: mapPackageMode(value.package_mode),
    package_artifact: packageArtifact === null ? null : mapPackageArtifact(packageArtifact),
    capability_summary: mapCapabilitySummary(value.capability_summary),
    manifest: mapJsonValue(value.manifest, "Project Extension manifest"),
    created_at: requireStringField(value, "created_at"),
    updated_at: requireStringField(value, "updated_at"),
  };
}

export async function fetchProjectExtensions(
  projectId: string,
): Promise<ProjectExtensionManagementListResponse> {
  const raw = await api.get<unknown>(`/projects/${encodeURIComponent(projectId)}/extensions`);
  const value = recordOrThrow(raw, "Project Extension management list");
  const extensions = value.extensions;
  if (!Array.isArray(extensions)) {
    throw new Error("Project Extension management list.extensions 不是数组");
  }
  return {
    extensions: extensions.map(mapManagementItem),
  };
}
