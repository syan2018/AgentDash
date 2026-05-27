import { api } from "../api/client";
import { asRecord } from "../api/mappers";
import type {
  InstallLibraryAssetRequest,
  InstallLibraryAssetResponse,
  InstalledAssetSourceDto,
  LibraryAssetDto,
  ListLibraryAssetsQuery,
  ProjectAssetSourceStatusDto,
  PublishLibraryAssetRequest,
} from "../types";

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

export async function fetchLibraryAssets(query: Partial<ListLibraryAssetsQuery> = {}): Promise<LibraryAssetDto[]> {
  const params = new URLSearchParams();
  if (query.asset_type) params.set("asset_type", query.asset_type);
  if (query.scope) params.set("scope", query.scope);
  if (query.owner_id) params.set("owner_id", query.owner_id);
  if (query.include_deprecated) params.set("include_deprecated", "true");
  const qs = params.toString() ? `?${params}` : "";
  const assets = await api.get<LibraryAssetDto[]>(`/shared-library/assets${qs}`);
  return assets.map(normalizeLibraryAsset);
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

export async function publishLibraryAsset(
  projectId: string,
  input: PublishLibraryAssetRequest,
): Promise<LibraryAssetDto> {
  const asset = await api.post<LibraryAssetDto>(
    `/projects/${encodeURIComponent(projectId)}/shared-library/publish`,
    input,
  );
  return normalizeLibraryAsset(asset);
}

export async function fetchProjectAssetSourceStatus(
  projectId: string,
): Promise<ProjectAssetSourceStatusDto> {
  return api.get<ProjectAssetSourceStatusDto>(
    `/projects/${encodeURIComponent(projectId)}/shared-library/source-status`,
  );
}

function normalizeLibraryAsset(asset: LibraryAssetDto): LibraryAssetDto {
  if (!asset.extension_package_artifact) return asset;
  return {
    ...asset,
    extension_package_artifact: {
      ...asset.extension_package_artifact,
      byte_size: BigInt(asset.extension_package_artifact.byte_size),
    },
  };
}
