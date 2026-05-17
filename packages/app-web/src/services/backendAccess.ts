import { api } from "../api/client";
import type {
  BackendWorkspaceInventory,
  InventoryRefreshResult,
  ProjectBackendAccess,
  ProjectBackendAccessStatus,
  WorkspaceBindingSyncResult,
  WorkspaceInventoryCandidate,
} from "../types";

export interface CreateProjectBackendAccessPayload {
  backend_id: string;
  priority?: number;
  root_policy?: Record<string, unknown>;
  capability_policy?: Record<string, unknown>;
  note?: string | null;
}

export interface UpdateProjectBackendAccessPayload {
  status?: ProjectBackendAccessStatus;
  priority?: number;
  root_policy?: Record<string, unknown>;
  capability_policy?: Record<string, unknown>;
  note?: string | null;
}

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

export function refreshBackendWorkspaceInventory(
  projectId: string,
  accessId: string,
): Promise<InventoryRefreshResult> {
  return api.post<InventoryRefreshResult>(
    `/projects/${projectId}/backend-access/${accessId}/inventory/refresh`,
    {},
  );
}

export function registerBackendWorkspaceInventory(
  projectId: string,
  accessId: string,
  payload: { root_ref: string },
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
