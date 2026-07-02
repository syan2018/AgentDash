import { describe, expect, it } from "vitest";
import type { BackendConfig, CurrentUser, ProjectBackendAccess } from "../../types";
import { selectSidebarBackendGroups } from "./sidebarBackendVisibility";

function backend(id: string, patch: Partial<BackendConfig> = {}): BackendConfig {
  return {
    id,
    name: id,
    endpoint: `http://${id}`,
    enabled: true,
    backend_type: "remote",
    owner_user_id: null,
    profile_id: null,
    device_id: null,
    machine_id: null,
    machine_label: null,
    visibility: "private",
    share_scope_kind: "system",
    share_scope_id: null,
    capability_slot: "default",
    device: {},
    last_claimed_at: null,
    registration_source: null,
    online: false,
    runtime_health: null,
    capabilities: null,
    ...patch,
  };
}

function access(backendId: string, status: ProjectBackendAccess["status"] = "active"): ProjectBackendAccess {
  return {
    id: `access-${backendId}`,
    project_id: "project-1",
    backend_id: backendId,
    status,
    access_mode: "explicit_grant",
    priority: 100,
    root_policy: {},
    capability_policy: {},
    note: null,
    created_by: null,
    created_at: "2026-07-02T00:00:00Z",
    updated_at: "2026-07-02T00:00:00Z",
  };
}

const currentUser: CurrentUser = {
  user_id: "user-1",
  subject: "user-1",
  auth_mode: "enterprise",
  display_name: "User One",
  email: "user@example.com",
  provider: "local",
  groups: [],
  is_admin: false,
  extra: {},
};

describe("selectSidebarBackendGroups", () => {
  it("按当前项目可用和当前用户自己的 backend 独立分组，允许同一连接重复出现", () => {
    const result = selectSidebarBackendGroups(
      [
        backend("project-backend"),
        backend("owned-project-backend", { owner_user_id: "user-1" }),
        backend("paused-project-backend"),
        backend("owned-backend", { owner_user_id: "user-1" }),
        backend("user-scope-backend", { share_scope_kind: "user", share_scope_id: "user-1" }),
        backend("other-backend", { owner_user_id: "user-2" }),
      ],
      [
        access("project-backend"),
        access("owned-project-backend"),
        access("paused-project-backend", "paused"),
      ],
      currentUser,
    );

    expect(result.projectBackends.map((item) => item.id)).toEqual([
      "project-backend",
      "owned-project-backend",
    ]);
    expect(result.personalBackends.map((item) => item.id)).toEqual([
      "owned-project-backend",
      "owned-backend",
      "user-scope-backend",
    ]);
  });
});
