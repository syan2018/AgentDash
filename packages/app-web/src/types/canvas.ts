import type {
  ArchiveInteractionDefinitionResponse,
  CanvasDefinitionDto,
  CanvasDefinitionListScopeDto,
  CanvasRuntimeFeaturesDto,
  InteractionDefinitionAccessDto,
  InteractionSourceFileDto,
  OperationWorkshopDescriptorDto,
} from "../generated/interaction-contracts";
import type { JsonValue } from "../generated/common-contracts";

export type CanvasListScope = CanvasDefinitionListScopeDto;
export type CanvasAccess = InteractionDefinitionAccessDto;
export type CanvasFile = InteractionSourceFileDto;

export interface Canvas {
  canvas_id: string;
  canvas_mount_id: string;
  vfs_mount_id: string;
  project_id: string;
  scope: "personal" | "project";
  access: CanvasAccess;
  title: string;
  description: string;
  entry_file: string;
  files: CanvasFile[];
  sandbox_config: {
    libraries: string[];
    import_map: { imports: Record<string, string> };
  };
  current_revision_id: string;
  source_bundle_digest: string;
  created_at: string;
  updated_at: string;
  definition: CanvasDefinitionDto;
}

export interface CreateCanvasInput {
  canvas_mount_id?: string;
  title: string;
  description?: string;
}

export interface UpdateCanvasInput {
  title?: string;
  description?: string;
  entry_file?: string;
  files?: CanvasFile[];
  sandbox_config?: {
    libraries: string[];
    import_map: { imports: Record<string, string> };
  };
}

export type DeleteCanvasResult = ArchiveInteractionDefinitionResponse;

export interface PublishCanvasToProjectInput {
  title?: string;
  description?: string;
}

export interface CopyCanvasToPersonalInput {
  canvas_mount_id?: string;
  title?: string;
  description?: string;
}

export type UnpublishCanvasResult = ArchiveInteractionDefinitionResponse;

export interface CanvasRuntimeFile extends CanvasFile {
  file_type: "code" | "data" | "style";
}

export interface CanvasRuntimeBinding {
  alias: string;
  source_uri: string;
  data_path: string;
  content_type: string;
  resolved: boolean;
}

export interface CanvasRuntimeSnapshot {
  project_id: string;
  canvas_id: string;
  definition_revision_id: string;
  interaction_instance_id?: string;
  interaction_state?: JsonValue;
  interaction_state_revision?: number;
  canvas_mount_id: string;
  vfs_mount_id: string;
  resource_surface_ref?: string;
  entry: string;
  files: CanvasRuntimeFile[];
  bindings: CanvasRuntimeBinding[];
  import_map: { imports: Record<string, string> };
  libraries: string[];
  operations: OperationWorkshopDescriptorDto[];
  features: CanvasRuntimeFeaturesDto;
}

export type CanvasOperationDescriptor = OperationWorkshopDescriptorDto;
