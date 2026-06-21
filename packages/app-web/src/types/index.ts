import type {
  BackendWorkspaceInventoryResponse,
  BackendResponse,
  BackendWithStatusResponse,
  InventoryRefreshResponse,
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
export type AuthMode = "personal" | "enterprise";
export type {
  BackendWorkspaceInventorySource,
  BackendWorkspaceInventoryStatus,
  ProjectBackendAccessMode,
  ProjectBackendAccessStatus,
} from "../generated/backend-contracts";

export function isAuthMode(value: unknown): value is AuthMode {
  return value === "personal" || value === "enterprise";
}

// ─── 登录 / 认证 ──────────────────────────────────────

export interface LoginFieldDescriptor {
  name: string;
  label: string;
  field_type: string;
  placeholder?: string | null;
  required: boolean;
}

export interface LoginMetadata {
  provider_type: string;
  display_name: string;
  description?: string | null;
  fields: LoginFieldDescriptor[];
  login_mode?: "form" | "redirect";
  start_url?: string | null;
  requires_login: boolean;
}

export interface AuthStartRequest {
  return_to?: string | null;
}

export interface AuthStartResponse {
  auth_url: string;
  state: string;
  expires_at_epoch_seconds: number;
}

export interface LoginCredentials {
  username: string;
  password: string;
  extra?: unknown;
}

export interface LoginResponse {
  access_token: string;
  identity: CurrentUser;
}

// ─── 当前用户 / 身份 ─────────────────────────────────

export interface AuthGroup {
  group_id: string;
  display_name?: string | null;
}

export interface CurrentUser {
  auth_mode: AuthMode;
  user_id: string;
  subject: string;
  display_name?: string | null;
  email?: string | null;
  avatar_url?: string | null;
  groups: AuthGroup[];
  is_admin: boolean;
  provider?: string | null;
  extra: unknown;
}

export interface DirectoryUser {
  user_id: string;
  subject: string;
  auth_mode: string;
  display_name?: string | null;
  email?: string | null;
  avatar_url?: string | null;
  is_admin: boolean;
  provider?: string | null;
  created_at: string;
  updated_at: string;
}

export interface DirectoryGroup {
  group_id: string;
  display_name?: string | null;
  created_at: string;
  updated_at: string;
}

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
export type InventoryRefreshResult = InventoryRefreshResponse;

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
