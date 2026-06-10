// ─── Canvas ──────────────────────────────────────────
import type {
  CanvasImportMapDto,
  CanvasRuntimeBindingDto,
  CanvasRuntimeBridgeSnapshotDto,
  CanvasRuntimeFileDto,
  CanvasRuntimeSnapshotDto,
  RuntimeActionDescriptorDto,
  RuntimeActionKindDto,
  RuntimeContextDto,
  RuntimeInvocationResultDto,
  RuntimePolicyDto,
  RuntimeSurfaceDto,
} from "../generated/canvas-contracts";

export type CanvasImportMap = CanvasImportMapDto;

export interface CanvasSandboxConfig {
  libraries: string[];
  import_map: CanvasImportMap;
}

export interface CanvasFile {
  path: string;
  content: string;
}

export interface CanvasDataBinding {
  alias: string;
  source_uri: string;
  content_type: string;
}

export interface Canvas {
  id: string;
  project_id: string;
  mount_id: string;
  title: string;
  description: string;
  entry_file: string;
  sandbox_config: CanvasSandboxConfig;
  files: CanvasFile[];
  bindings: CanvasDataBinding[];
  created_at: string;
  updated_at: string;
}

export type CanvasRuntimeFile = CanvasRuntimeFileDto;

export type CanvasRuntimeBinding = CanvasRuntimeBindingDto;

export type RuntimeActionKind = RuntimeActionKindDto;

export type RuntimePolicy = RuntimePolicyDto;

export type RuntimeActionDescriptor = RuntimeActionDescriptorDto;

export type RuntimeContext = RuntimeContextDto;

export type RuntimeSurface = RuntimeSurfaceDto;

export type CanvasRuntimeBridgeSnapshot = CanvasRuntimeBridgeSnapshotDto;

export type CanvasRuntimeSnapshot = CanvasRuntimeSnapshotDto;

export type RuntimeInvocationResult = RuntimeInvocationResultDto;
