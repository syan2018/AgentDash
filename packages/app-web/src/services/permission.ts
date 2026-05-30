import { api } from "../api/client";
import type { PermissionGrant } from "../types/permission";

export async function listPermissionGrants(params: {
  sessionId?: string;
  runId?: string;
  status?: string;
}): Promise<PermissionGrant[]> {
  const searchParams = new URLSearchParams();
  if (params.sessionId) searchParams.set("session_id", params.sessionId);
  if (params.runId) searchParams.set("run_id", params.runId);
  if (params.status) searchParams.set("status", params.status);
  const query = searchParams.toString();
  return api.get<PermissionGrant[]>(`/permission-grants${query ? `?${query}` : ""}`);
}

export async function getPermissionGrant(grantId: string): Promise<PermissionGrant> {
  return api.get<PermissionGrant>(`/permission-grants/${grantId}`);
}

export async function approvePermissionGrant(grantId: string): Promise<PermissionGrant> {
  return api.post<PermissionGrant>(`/permission-grants/${grantId}/approve`, {});
}

export async function rejectPermissionGrant(grantId: string): Promise<PermissionGrant> {
  return api.post<PermissionGrant>(`/permission-grants/${grantId}/reject`, {});
}

export async function revokePermissionGrant(grantId: string): Promise<PermissionGrant> {
  return api.post<PermissionGrant>(`/permission-grants/${grantId}/revoke`, {});
}
