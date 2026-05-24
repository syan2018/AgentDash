export type {
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
  PublishLibraryAssetRequest,
  SeedBuiltinLibraryAssetsRequest,
  SharedLibrarySourceStatus,
} from "../generated/shared-library-contracts";

export type PublishLibraryAssetKind =
  | "project_agent"
  | "mcp_preset"
  | "workflow_bundle"
  | "skill_asset"
  | "vfs_mount";
