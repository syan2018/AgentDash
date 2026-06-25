// ─── Canvas ──────────────────────────────────────────
import type {
  CanvasAccessDto,
  CanvasAgentRunRuntimeSnapshotDto,
  CanvasFileDto,
  CanvasImportMapDto,
  CanvasListScopeDto,
  CanvasResponse,
  CanvasRuntimeBindingDto,
  CanvasRuntimeBindingUpsertRequest,
  CanvasRuntimeBridgeSnapshotDto,
  CanvasRuntimeFileDto,
  CanvasRuntimeSnapshotDto,
  CanvasSandboxConfigDto,
  CanvasScopeDto,
  CopyCanvasToPersonalRequest,
  CreateCanvasRequest,
  DeleteCanvasResponse,
  ListCanvasesQuery,
  PublishCanvasToProjectRequest,
  RuntimeActionDescriptorDto,
  RuntimeActionKindDto,
  RuntimeContextDto,
  RuntimeInvocationResultDto,
  RuntimePolicyDto,
  RuntimeSurfaceDto,
  UnpublishCanvasResponse,
  UpdateCanvasRequest,
} from "../generated/canvas-contracts";

export type CanvasImportMap = CanvasImportMapDto;

export type CanvasSandboxConfig = CanvasSandboxConfigDto;

export type CanvasFile = CanvasFileDto;

export type CanvasScope = CanvasScopeDto;

export type CanvasListScope = CanvasListScopeDto;

export type CanvasAccess = CanvasAccessDto;

export type ListCanvasesInput = ListCanvasesQuery;

export type Canvas = CanvasResponse;

export type CreateCanvasInput = CreateCanvasRequest;

export type UpdateCanvasInput = UpdateCanvasRequest;

export type DeleteCanvasResult = DeleteCanvasResponse;

export type PublishCanvasToProjectInput = PublishCanvasToProjectRequest;

export type CopyCanvasToPersonalInput = CopyCanvasToPersonalRequest;

export type UnpublishCanvasResult = UnpublishCanvasResponse;

export type CanvasRuntimeFile = CanvasRuntimeFileDto;

export type CanvasRuntimeBinding = CanvasRuntimeBindingDto;

export type CanvasRuntimeBindingUpsertInput = CanvasRuntimeBindingUpsertRequest;

export type RuntimeActionKind = RuntimeActionKindDto;

export type RuntimePolicy = RuntimePolicyDto;

export type RuntimeActionDescriptor = RuntimeActionDescriptorDto;

export type RuntimeContext = RuntimeContextDto;

export type RuntimeSurface = RuntimeSurfaceDto;

export type CanvasRuntimeBridgeSnapshot = CanvasRuntimeBridgeSnapshotDto;

export type CanvasRuntimeSnapshot = CanvasRuntimeSnapshotDto | CanvasAgentRunRuntimeSnapshotDto;

export type RuntimeInvocationResult = RuntimeInvocationResultDto;
