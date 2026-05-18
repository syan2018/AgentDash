export type LibraryAssetType =
  | "agent_template"
  | "mcp_server_template"
  | "workflow_template"
  | "skill_template";

export type LibraryAssetScope = "builtin" | "system" | "org" | "user";
export type LibraryAssetSource = "builtin" | "user_authored" | "remote_imported";
export type SharedLibrarySourceStatus = "up_to_date" | "update_available" | "source_missing";

export interface InstalledAssetSourceDto {
  library_asset_id: string;
  source_ref: string;
  source_version: string;
  source_digest: string;
  installed_at: string;
}

export interface LibraryAssetDto {
  id: string;
  asset_type: LibraryAssetType;
  scope: LibraryAssetScope;
  owner_id?: string | null;
  key: string;
  display_name: string;
  description?: string | null;
  version: string;
  source: LibraryAssetSource;
  source_ref?: string | null;
  payload_digest: string;
  deprecated: boolean;
  payload: unknown;
  created_at: string;
  updated_at: string;
}

export interface ListLibraryAssetsQuery {
  asset_type?: LibraryAssetType;
  scope?: LibraryAssetScope;
  owner_id?: string;
  include_deprecated?: boolean;
}

export interface SeedBuiltinLibraryAssetsRequest {
  asset_type?: LibraryAssetType;
  key?: string;
}

export interface InstallLibraryAssetRequest {
  library_asset_id: string;
  target_key?: string;
  overwrite?: boolean;
}

export type InstallLibraryAssetResponse =
  | { asset_kind: "project_agent"; agent_id: string; project_agent_link_id: string }
  | { asset_kind: "mcp_preset"; id: string }
  | { asset_kind: "workflow_template"; workflow_ids: string[]; lifecycle_id: string }
  | { asset_kind: "skill_asset"; id: string };

export interface ProjectAssetSourceStatusItemDto {
  asset_kind: string;
  project_asset_id: string;
  project_asset_key: string;
  installed_source: InstalledAssetSourceDto;
  source_status: SharedLibrarySourceStatus;
  current_source_version?: string | null;
  current_source_digest?: string | null;
}

export interface ProjectAssetSourceStatusDto {
  project_agents: ProjectAssetSourceStatusItemDto[];
  mcp_presets: ProjectAssetSourceStatusItemDto[];
  skill_assets: ProjectAssetSourceStatusItemDto[];
  workflow_definitions: ProjectAssetSourceStatusItemDto[];
  lifecycle_definitions: ProjectAssetSourceStatusItemDto[];
}
