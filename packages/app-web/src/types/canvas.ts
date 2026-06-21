// ─── Canvas ──────────────────────────────────────────
import type {
  CanvasDataBindingDto,
  CanvasFileDto,
  CanvasImportMapDto,
  CanvasResponse,
  CanvasRuntimeBindingDto,
  CanvasRuntimeBridgeSnapshotDto,
  CanvasRuntimeFileDto,
  CanvasRuntimeSnapshotDto,
  CanvasSandboxConfigDto,
  CreateCanvasRequest,
  DeleteCanvasResponse,
  RuntimeActionDescriptorDto,
  RuntimeActionKindDto,
  RuntimeContextDto,
  RuntimeInvocationResultDto,
  RuntimePolicyDto,
  RuntimeSurfaceDto,
  UpdateCanvasRequest,
} from "../generated/canvas-contracts";

export type CanvasImportMap = CanvasImportMapDto;

export type CanvasSandboxConfig = CanvasSandboxConfigDto;

export type CanvasFile = CanvasFileDto;

export type CanvasDataBinding = CanvasDataBindingDto;

export type Canvas = CanvasResponse;

export type CreateCanvasInput = CreateCanvasRequest;

export type UpdateCanvasInput = UpdateCanvasRequest;

export type DeleteCanvasResult = DeleteCanvasResponse;

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
