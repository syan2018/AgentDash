import { api } from "../api/client";
import { asRecord } from "../api/mappers";
import type {
  InstallLibraryAssetRequest,
  InstallLibraryAssetResponse,
  InstalledAssetSourceDto,
  LibraryAssetDto,
  LibraryAssetScope,
  LibraryAssetSource,
  LibraryAssetType,
  ListLibraryAssetsQuery,
  ProjectAssetSourceStatusDto,
  ProjectAssetSourceStatusItemDto,
  SeedBuiltinLibraryAssetsRequest,
  SharedLibrarySourceStatus,
} from "../types";

function normalizeAssetType(value: unknown): LibraryAssetType {
  if (
    value === "agent_template"
    || value === "mcp_server_template"
    || value === "workflow_template"
    || value === "skill_template"
  ) {
    return value;
  }
  throw new Error(`未知 LibraryAsset asset_type: ${String(value)}`);
}

function normalizeScope(value: unknown): LibraryAssetScope {
  if (value === "system" || value === "org" || value === "user") return value;
  return "builtin";
}

function normalizeSource(value: unknown): LibraryAssetSource {
  if (value === "user_authored" || value === "remote_imported") return value;
  return "builtin";
}

function normalizeStatus(value: unknown): SharedLibrarySourceStatus {
  if (value === "update_available" || value === "source_missing") return value;
  return "up_to_date";
}

export function mapInstalledAssetSource(raw: unknown): InstalledAssetSourceDto | null {
  if (raw == null) return null;
  const value = asRecord(raw);
  if (!value) return null;
  return {
    library_asset_id: String(value.library_asset_id ?? ""),
    source_ref: String(value.source_ref ?? ""),
    source_version: String(value.source_version ?? ""),
    source_digest: String(value.source_digest ?? ""),
    installed_at: String(value.installed_at ?? ""),
  };
}

function mapLibraryAsset(raw: Record<string, unknown>): LibraryAssetDto {
  return {
    id: String(raw.id ?? ""),
    asset_type: normalizeAssetType(raw.asset_type),
    scope: normalizeScope(raw.scope),
    owner_id: raw.owner_id == null ? null : String(raw.owner_id),
    key: String(raw.key ?? ""),
    display_name: String(raw.display_name ?? raw.key ?? ""),
    description: raw.description == null ? null : String(raw.description),
    version: String(raw.version ?? ""),
    source: normalizeSource(raw.source),
    source_ref: raw.source_ref == null ? null : String(raw.source_ref),
    payload_digest: String(raw.payload_digest ?? ""),
    deprecated: Boolean(raw.deprecated),
    payload: raw.payload,
    created_at: String(raw.created_at ?? ""),
    updated_at: String(raw.updated_at ?? ""),
  };
}

function mapSourceStatusItem(raw: unknown): ProjectAssetSourceStatusItemDto {
  const value = asRecord(raw);
  if (!value) {
    throw new Error("source status item 缺失或不是对象");
  }
  const installedSource = mapInstalledAssetSource(value.installed_source);
  if (!installedSource) {
    throw new Error("source status item 缺少 installed_source");
  }
  return {
    asset_kind: String(value.asset_kind ?? ""),
    project_asset_id: String(value.project_asset_id ?? ""),
    project_asset_key: String(value.project_asset_key ?? ""),
    installed_source: installedSource,
    source_status: normalizeStatus(value.source_status),
    current_source_version: value.current_source_version == null ? null : String(value.current_source_version),
    current_source_digest: value.current_source_digest == null ? null : String(value.current_source_digest),
  };
}

export async function fetchLibraryAssets(query: ListLibraryAssetsQuery = {}): Promise<LibraryAssetDto[]> {
  const params = new URLSearchParams();
  if (query.asset_type) params.set("asset_type", query.asset_type);
  if (query.scope) params.set("scope", query.scope);
  if (query.owner_id) params.set("owner_id", query.owner_id);
  if (query.include_deprecated) params.set("include_deprecated", "true");
  const qs = params.toString() ? `?${params}` : "";
  const raw = await api.get<Record<string, unknown>[]>(`/shared-library/assets${qs}`);
  return raw.map(mapLibraryAsset);
}

export async function seedBuiltinLibraryAssets(
  input: SeedBuiltinLibraryAssetsRequest = {},
): Promise<LibraryAssetDto[]> {
  const raw = await api.post<Record<string, unknown>[]>(
    "/shared-library/assets/seed-builtin",
    input,
  );
  return raw.map(mapLibraryAsset);
}

export async function installLibraryAsset(
  projectId: string,
  input: InstallLibraryAssetRequest,
): Promise<InstallLibraryAssetResponse> {
  return api.post<InstallLibraryAssetResponse>(
    `/projects/${encodeURIComponent(projectId)}/shared-library/install`,
    input,
  );
}

export async function fetchProjectAssetSourceStatus(
  projectId: string,
): Promise<ProjectAssetSourceStatusDto> {
  const raw = await api.get<Record<string, unknown>>(
    `/projects/${encodeURIComponent(projectId)}/shared-library/source-status`,
  );
  return {
    project_agents: Array.isArray(raw.project_agents)
      ? raw.project_agents.map(mapSourceStatusItem)
      : [],
    mcp_presets: Array.isArray(raw.mcp_presets) ? raw.mcp_presets.map(mapSourceStatusItem) : [],
    skill_assets: Array.isArray(raw.skill_assets) ? raw.skill_assets.map(mapSourceStatusItem) : [],
    workflow_definitions: Array.isArray(raw.workflow_definitions)
      ? raw.workflow_definitions.map(mapSourceStatusItem)
      : [],
    lifecycle_definitions: Array.isArray(raw.lifecycle_definitions)
      ? raw.lifecycle_definitions.map(mapSourceStatusItem)
      : [],
  };
}
