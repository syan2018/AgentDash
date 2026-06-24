import { api } from "../api/client";
import type {
  BackendWorkspaceInventoryResponse,
  CreateProjectBackendAccessRequest,
  ProjectBackendAccessResponse,
  RegisterBackendWorkspaceInventoryRequest,
  UpdateProjectBackendAccessRequest,
} from "../generated/backend-contracts";
import type {
  BindDiscoveredWorkspaceBindingsRequest,
  BindDiscoveredWorkspaceBindingsResponse,
  DiscoverLocalWorkspaceBindingsRequest,
  DiscoverLocalWorkspaceBindingsResponse,
  WorkspaceBindingSyncResult,
  WorkspaceInventoryCandidate,
} from "../generated/workspace-contracts";

export type ProjectBackendAccess = ProjectBackendAccessResponse;
export type BackendWorkspaceInventory = BackendWorkspaceInventoryResponse;
export type CreateProjectBackendAccessPayload = CreateProjectBackendAccessRequest;
export type UpdateProjectBackendAccessPayload = UpdateProjectBackendAccessRequest;

export function listProjectBackendAccess(projectId: string): Promise<ProjectBackendAccess[]> {
  return api.get<ProjectBackendAccess[]>(`/projects/${projectId}/backend-access`);
}

export function createProjectBackendAccess(
  projectId: string,
  payload: CreateProjectBackendAccessPayload,
): Promise<ProjectBackendAccess> {
  return api.post<ProjectBackendAccess>(`/projects/${projectId}/backend-access`, payload);
}

export function updateProjectBackendAccess(
  projectId: string,
  accessId: string,
  payload: UpdateProjectBackendAccessPayload,
): Promise<ProjectBackendAccess> {
  return api.patch<ProjectBackendAccess>(
    `/projects/${projectId}/backend-access/${accessId}`,
    payload,
  );
}

export function revokeProjectBackendAccess(projectId: string, accessId: string): Promise<unknown> {
  return api.delete(`/projects/${projectId}/backend-access/${accessId}`);
}

export function listBackendWorkspaceInventory(
  projectId: string,
  accessId: string,
): Promise<BackendWorkspaceInventory[]> {
  return api.get<BackendWorkspaceInventory[]>(
    `/projects/${projectId}/backend-access/${accessId}/inventory`,
  );
}

export function registerBackendWorkspaceInventory(
  projectId: string,
  accessId: string,
  payload: RegisterBackendWorkspaceInventoryRequest,
): Promise<BackendWorkspaceInventory> {
  return api.post<BackendWorkspaceInventory>(
    `/projects/${projectId}/backend-access/${accessId}/inventory/register`,
    payload,
  );
}

export function listWorkspaceInventoryCandidates(
  projectId: string,
): Promise<WorkspaceInventoryCandidate[]> {
  return api.get<WorkspaceInventoryCandidate[]>(`/projects/${projectId}/workspaces/candidates`);
}

export function syncWorkspaceBackendBindings(
  projectId: string,
): Promise<WorkspaceBindingSyncResult> {
  return api.post<WorkspaceBindingSyncResult>(
    `/projects/${projectId}/workspaces/sync-backend-bindings`,
    {},
  );
}

export function discoverLocalWorkspaceBindings(
  projectId: string,
  payload: DiscoverLocalWorkspaceBindingsRequest,
): Promise<DiscoverLocalWorkspaceBindingsResponse> {
  return api.post<DiscoverLocalWorkspaceBindingsResponse>(
    `/projects/${projectId}/workspaces/discover-local-bindings`,
    payload,
  );
}

export function bindDiscoveredWorkspaceBindings(
  projectId: string,
  payload: BindDiscoveredWorkspaceBindingsRequest,
): Promise<BindDiscoveredWorkspaceBindingsResponse> {
  return api.post<BindDiscoveredWorkspaceBindingsResponse>(
    `/projects/${projectId}/workspaces/bind-discovered`,
    payload,
  );
}
