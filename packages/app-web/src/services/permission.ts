import { api } from "../api/client";
import type {
  ListPermissionGrantsQuery,
  PermissionGrantResponse,
} from "../generated/permission-contracts";

export async function listPermissionGrants(params: ListPermissionGrantsQuery): Promise<PermissionGrantResponse[]> {
  const searchParams = new URLSearchParams();
  if (params.effect_frame_id) searchParams.set("effect_frame_id", params.effect_frame_id);
  if (params.run_id) searchParams.set("run_id", params.run_id);
  if (params.status) searchParams.set("status", params.status);
  if (params.status_group) searchParams.set("status_group", params.status_group);
  const query = searchParams.toString();
  return api.get<PermissionGrantResponse[]>(`/permission-grants${query ? `?${query}` : ""}`);
}

export async function getPermissionGrant(grantId: string): Promise<PermissionGrantResponse> {
  return api.get<PermissionGrantResponse>(`/permission-grants/${grantId}`);
}

export async function approvePermissionGrant(grantId: string): Promise<PermissionGrantResponse> {
  return api.post<PermissionGrantResponse>(`/permission-grants/${grantId}/approve`, {});
}

export async function rejectPermissionGrant(grantId: string): Promise<PermissionGrantResponse> {
  return api.post<PermissionGrantResponse>(`/permission-grants/${grantId}/reject`, {});
}

export async function revokePermissionGrant(grantId: string): Promise<PermissionGrantResponse> {
  return api.post<PermissionGrantResponse>(`/permission-grants/${grantId}/revoke`, {});
}
