import { api } from "../api/client";
import type { ProjectFilespace, ProjectVfsMountBinding } from "../types";

export interface CreateProjectFilespacePayload {
  key: string;
  display_name: string;
  description?: string | null;
}

export interface UpdateProjectFilespacePayload {
  key: string;
  display_name: string;
  description?: string | null;
}

export interface UpdateProjectVfsMountBindingPayload {
  mount_id: string;
  display_name: string;
  source: ProjectVfsMountBinding["source"];
  capabilities: ProjectVfsMountBinding["capabilities"];
  default_write: boolean;
}

export interface CreateProjectVfsMountBindingPayload {
  mount_id: string;
  display_name: string;
  source: ProjectVfsMountBinding["source"];
  capabilities: ProjectVfsMountBinding["capabilities"];
  default_write: boolean;
}

export async function listProjectFilespaces(projectId: string): Promise<ProjectFilespace[]> {
  return api.get<ProjectFilespace[]>(`/projects/${projectId}/filespaces`);
}

export async function createProjectFilespace(
  projectId: string,
  payload: CreateProjectFilespacePayload,
): Promise<ProjectFilespace> {
  return api.post<ProjectFilespace>(`/projects/${projectId}/filespaces`, payload);
}

export async function updateProjectFilespace(
  projectId: string,
  filespaceId: string,
  payload: UpdateProjectFilespacePayload,
): Promise<ProjectFilespace> {
  return api.put<ProjectFilespace>(`/projects/${projectId}/filespaces/${filespaceId}`, payload);
}

export async function deleteProjectFilespace(
  projectId: string,
  filespaceId: string,
): Promise<{ ok: boolean }> {
  return api.delete<{ ok: boolean }>(`/projects/${projectId}/filespaces/${filespaceId}`);
}

export async function listProjectVfsMountBindings(projectId: string): Promise<ProjectVfsMountBinding[]> {
  return api.get<ProjectVfsMountBinding[]>(`/projects/${projectId}/vfs-mount-bindings`);
}

export async function updateProjectVfsMountBinding(
  projectId: string,
  bindingId: string,
  payload: UpdateProjectVfsMountBindingPayload,
): Promise<ProjectVfsMountBinding> {
  return api.put<ProjectVfsMountBinding>(
    `/projects/${projectId}/vfs-mount-bindings/${bindingId}`,
    payload,
  );
}

export async function createProjectVfsMountBinding(
  projectId: string,
  payload: CreateProjectVfsMountBindingPayload,
): Promise<ProjectVfsMountBinding> {
  return api.post<ProjectVfsMountBinding>(
    `/projects/${projectId}/vfs-mount-bindings`,
    payload,
  );
}

export async function deleteProjectVfsMountBinding(
  projectId: string,
  bindingId: string,
): Promise<{ ok: boolean }> {
  return api.delete<{ ok: boolean }>(
    `/projects/${projectId}/vfs-mount-bindings/${bindingId}`,
  );
}
