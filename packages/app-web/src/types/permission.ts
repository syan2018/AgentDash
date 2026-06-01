import type {
  PermissionGrantResponse,
  PermissionGrantScopeDto,
  PermissionGrantStatusDto,
} from "../generated/permission-contracts";

export type GrantScope = PermissionGrantScopeDto;
export type GrantStatus = PermissionGrantStatusDto;
export type PermissionGrant = PermissionGrantResponse;

export function isGrantActive(status: GrantStatus): boolean {
  return status === "applied" || status === "scope_escalated";
}

export function isGrantPendingAction(status: GrantStatus): boolean {
  return status === "pending_user_approval";
}
