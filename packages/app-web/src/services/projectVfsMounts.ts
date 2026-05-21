import { api } from "../api/client";
import type { ProjectVfsMount, ProjectVfsMountContent } from "../types";

export interface CreateProjectVfsMountPayload {
  mount_id: string;
  display_name: string;
  description?: string | null;
  capabilities: ProjectVfsMount["capabilities"];
  content: ProjectVfsMountContent;
}

export interface UpdateProjectVfsMountPayload {
  mount_id: string;
  display_name: string;
  description?: string | null;
  capabilities: ProjectVfsMount["capabilities"];
  content: ProjectVfsMountContent;
}

export async function listProjectVfsMounts(projectId: string): Promise<ProjectVfsMount[]> {
  return api.get<ProjectVfsMount[]>(`/projects/${projectId}/vfs-mounts`);
}

export async function getProjectVfsMount(
  projectId: string,
  mountId: string,
): Promise<ProjectVfsMount> {
  return api.get<ProjectVfsMount>(
    `/projects/${projectId}/vfs-mounts/${encodeURIComponent(mountId)}`,
  );
}

export async function createProjectVfsMount(
  projectId: string,
  payload: CreateProjectVfsMountPayload,
): Promise<ProjectVfsMount> {
  return api.post<ProjectVfsMount>(`/projects/${projectId}/vfs-mounts`, payload);
}

export async function updateProjectVfsMount(
  projectId: string,
  mountId: string,
  payload: UpdateProjectVfsMountPayload,
): Promise<ProjectVfsMount> {
  return api.put<ProjectVfsMount>(
    `/projects/${projectId}/vfs-mounts/${encodeURIComponent(mountId)}`,
    payload,
  );
}

export async function deleteProjectVfsMount(
  projectId: string,
  mountId: string,
): Promise<{ ok: boolean }> {
  return api.delete<{ ok: boolean }>(
    `/projects/${projectId}/vfs-mounts/${encodeURIComponent(mountId)}`,
  );
}
