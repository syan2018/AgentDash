import type { BackendConfig, CurrentUser, ProjectBackendAccess } from "../../types";

export interface SidebarBackendGroups {
  projectBackends: BackendConfig[];
  personalBackends: BackendConfig[];
}

function isCurrentUserBackend(backend: BackendConfig, currentUser: CurrentUser | null): boolean {
  if (!currentUser) return false;
  if (backend.owner_user_id === currentUser.user_id) return true;
  return backend.share_scope_kind === "user" && backend.share_scope_id === currentUser.user_id;
}

export function selectSidebarBackendGroups(
  backends: BackendConfig[],
  projectAccesses: ProjectBackendAccess[],
  currentUser: CurrentUser | null,
): SidebarBackendGroups {
  const activeProjectBackendIds = new Set(
    projectAccesses
      .filter((access) => access.status === "active")
      .map((access) => access.backend_id),
  );

  const projectBackends = backends.filter((backend) => activeProjectBackendIds.has(backend.id));
  const personalBackends = backends.filter((backend) => isCurrentUserBackend(backend, currentUser));

  return { projectBackends, personalBackends };
}
