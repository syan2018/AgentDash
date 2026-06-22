import type {
  BackendWorkspaceInventoryResponse,
  BackendResponse,
  BackendWithStatusResponse,
  ProjectBackendAccessResponse,
} from "../generated/backend-contracts";
import type {
  WorkspaceBindingSyncResult as GeneratedWorkspaceBindingSyncResult,
  WorkspaceBindingResponse,
  WorkspaceIdentityKind,
  WorkspaceInventoryCandidate as GeneratedWorkspaceInventoryCandidate,
  WorkspaceResponse,
} from "../generated/workspace-contracts";

// ─── Generated aliases with remaining local wire gaps ──

export type Workspace = WorkspaceResponse;
export type WorkspaceBinding = WorkspaceBindingResponse;
export type {
  BackendType,
} from "../generated/backend-contracts";
export type {
  WorkspaceBindingStatus,
  WorkspaceIdentityKind,
  WorkspaceResolutionPolicy,
  WorkspaceStatus,
} from "../generated/workspace-contracts";

// ─── 基础枚举 ─────────────────────────────────────────

export type BackendConfig = BackendWithStatusResponse;
export type BackendSafeConfig = BackendResponse;
export type {
  AuthGroup,
  AuthMode,
  AuthStartRequest,
  AuthStartResponse,
  CurrentUser,
  DirectoryGroup,
  DirectoryUser,
  LoginCredentials,
  LoginFieldDescriptor,
  LoginMetadata,
  LoginMode,
  LoginResponse,
} from "../generated/auth-contracts";
export type {
  BackendWorkspaceInventorySource,
  BackendWorkspaceInventoryStatus,
  ProjectBackendAccessMode,
  ProjectBackendAccessStatus,
} from "../generated/backend-contracts";

// ─── Workspace ────────────────────────────────────────

export type WorkspaceDetectionResult = {
  identity_kind: WorkspaceIdentityKind;
  identity_payload: Record<string, unknown>;
  binding: WorkspaceBinding;
  confidence: string;
  warnings: string[];
  matched_workspace_ids: string[];
};

export type ProjectBackendAccess = ProjectBackendAccessResponse;
export type BackendWorkspaceInventory = BackendWorkspaceInventoryResponse;
export type WorkspaceInventoryCandidate = GeneratedWorkspaceInventoryCandidate;
export type WorkspaceBindingSyncResult = GeneratedWorkspaceBindingSyncResult;

// ─── Re-exports from domain-split files ──────────────────

export * from "./capability";
export * from "./project";
export * from "./project-agent";
export * from "./story-task";
export * from "./routine";
export * from "./context";
export * from "./workflow";
export * from "./canvas";
export * from "./session";
export * from "./mcp-preset";
export * from "./skill-asset";
export * from "./extension-runtime";
export * from "./shared-library";
export * from "./acp";
export * from "./lifecycle-views";
