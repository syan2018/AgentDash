import { authenticatedFetch, type ApiHttpError } from "../api/client";
import { buildApiPath } from "../api/origin";
import { asRecord, requireStringField } from "../api/mappers";
import type { JsonValue } from "../generated/common-contracts";
import type {
  ExtensionPackageArtifactResponse,
  ExtensionPackageInstallationResponse,
  ImportExtensionPackageResponse,
} from "../generated/extension-package-contracts";

function recordOrThrow(raw: unknown, label: string): Record<string, unknown> {
  const value = asRecord(raw);
  if (!value || Array.isArray(raw)) {
    throw new Error(`${label} 不是对象`);
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

function requireBigIntField(
  raw: Record<string, unknown>,
  field: string,
  label: string,
): bigint {
  const value = raw[field];
  if (typeof value === "number") {
    if (!Number.isFinite(value) || !Number.isInteger(value)) {
      throw new Error(`${label}.${field} 不是合法整数`);
    }
    return BigInt(value);
  }
  if (typeof value === "string" && value.trim() !== "") {
    try {
      return BigInt(value);
    } catch {
      throw new Error(`${label}.${field} 无法解析为整数`);
    }
  }
  if (typeof value === "bigint") {
    return value;
  }
  throw new Error(`${label}.${field} 缺失或非法`);
}

function mapPackageArtifact(raw: unknown): ExtensionPackageArtifactResponse {
  const value = recordOrThrow(raw, "extension package artifact");
  return {
    id: requireStringField(value, "id"),
    owner_kind: requireStringField(value, "owner_kind"),
    owner_id: requireStringField(value, "owner_id"),
    extension_id: requireStringField(value, "extension_id"),
    package_name: requireStringField(value, "package_name"),
    package_version: requireStringField(value, "package_version"),
    asset_version: requireStringField(value, "asset_version"),
    source_version: requireStringField(value, "source_version"),
    storage_ref: requireStringField(value, "storage_ref"),
    archive_digest: requireStringField(value, "archive_digest"),
    manifest_digest: requireStringField(value, "manifest_digest"),
    manifest: mapJsonValue(value.manifest, "extension package artifact.manifest"),
    byte_size: requireBigIntField(value, "byte_size", "extension package artifact"),
    created_at: requireStringField(value, "created_at"),
    updated_at: requireStringField(value, "updated_at"),
  };
}

function mapPackageImport(raw: unknown): ImportExtensionPackageResponse {
  const value = recordOrThrow(raw, "extension package import");
  return {
    artifact: mapPackageArtifact(value.artifact),
    installation: mapPackageInstallation(value.installation),
  };
}

function mapPackageInstallation(raw: unknown): ExtensionPackageInstallationResponse {
  const value = recordOrThrow(raw, "extension package installation");
  return {
    installation_id: requireStringField(value, "installation_id"),
    extension_key: requireStringField(value, "extension_key"),
    extension_id: requireStringField(value, "extension_id"),
    package_artifact_id: requireStringField(value, "package_artifact_id"),
    archive_digest: requireStringField(value, "archive_digest"),
  };
}

async function throwApiError(response: Response): Promise<never> {
  const body = await response.json().catch(() => ({ error: response.statusText }));
  const record = asRecord(body);
  const message =
    record && typeof record.error === "string" && record.error.trim() !== ""
      ? record.error
      : `HTTP ${response.status}`;
  const error = new Error(message);
  (error as ApiHttpError).status = response.status;
  throw error;
}

export interface ImportExtensionPackageOptions {
  extension_key?: string | null;
  display_name?: string | null;
  overwrite: boolean;
}

export async function importExtensionPackage(
  projectId: string,
  file: File,
  archiveDigest: string,
  options: ImportExtensionPackageOptions,
): Promise<ImportExtensionPackageResponse> {
  const form = new FormData();
  form.append("archive_digest", archiveDigest);
  form.append("archive", file, file.name);
  form.append("overwrite", options.overwrite ? "true" : "false");
  if (options.extension_key && options.extension_key.trim() !== "") {
    form.append("extension_key", options.extension_key);
  }
  if (options.display_name && options.display_name.trim() !== "") {
    form.append("display_name", options.display_name);
  }
  const response = await authenticatedFetch(
    buildApiPath(`/projects/${encodeURIComponent(projectId)}/extensions/import-package`),
    {
      method: "POST",
      body: form,
    },
  );
  if (!response.ok) {
    await throwApiError(response);
  }
  const raw = await response.json();
  return mapPackageImport(raw);
}

export interface ExtensionArtifactDownload {
  blob: Blob;
  filename: string;
}

export async function downloadExtensionArtifact(
  projectId: string,
  artifactId: string,
): Promise<ExtensionArtifactDownload> {
  const response = await authenticatedFetch(
    buildApiPath(
      `/projects/${encodeURIComponent(projectId)}/extension-artifacts/${encodeURIComponent(artifactId)}/archive`,
    ),
    { method: "GET" },
  );
  if (!response.ok) {
    await throwApiError(response);
  }
  const blob = await response.blob();
  const filename = parseContentDispositionFilename(
    response.headers.get("content-disposition"),
  );
  return { blob, filename };
}

export function parseContentDispositionFilename(header: string | null): string {
  if (!header) return "";
  // RFC 5987 filename* takes precedence
  const starMatch = /filename\*\s*=\s*(?:UTF-8|utf-8)''([^;]+)/i.exec(header);
  if (starMatch && starMatch[1]) {
    try {
      return decodeURIComponent(starMatch[1].trim());
    } catch {
      // fall through to plain filename
    }
  }
  const plainMatch = /filename\s*=\s*("([^"]*)"|([^;]+))/i.exec(header);
  if (plainMatch) {
    const value = (plainMatch[2] ?? plainMatch[3] ?? "").trim();
    return value;
  }
  return "";
}
