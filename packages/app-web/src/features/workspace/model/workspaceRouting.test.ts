import { describe, expect, it, vi } from "vitest";
import type {
  BackendConfig,
  ProjectBackendAccess,
  Workspace,
  WorkspaceInventoryCandidate,
} from "../../../types";
import {
  candidateToDraft,
  identitySummary,
  summarizeAvailability,
  summarizeResolution,
} from "./workspaceRouting";

vi.stubGlobal("crypto", {
  randomUUID: () => "generated-binding-id",
});

function backend(id: string, online: boolean, backend_type: "local" | "remote" = "local"): BackendConfig {
  return {
    id,
    name: id,
    endpoint: `ws://${id}`,
    enabled: true,
    backend_type,
    owner_user_id: null,
    profile_id: null,
    device_id: null,
    machine_id: null,
    machine_label: null,
    legacy_machine_ids: [],
    visibility: "private",
    share_scope_kind: "user",
    share_scope_id: null,
    capability_slot: "default",
    device: {},
    last_claimed_at: null,
    online,
    runtime_health: null,
    workspace_roots: null,
    capabilities: null,
  };
}

function access(backend_id: string, status: ProjectBackendAccess["status"] = "active"): ProjectBackendAccess {
  return {
    id: `access-${backend_id}`,
    project_id: "project-1",
    backend_id,
    status,
    access_mode: "use_inventory",
    priority: 0,
    root_policy: {},
    capability_policy: {},
    created_at: "2026-05-17T00:00:00Z",
    updated_at: "2026-05-17T00:00:00Z",
  };
}

function workspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    id: "workspace-1",
    project_id: "project-1",
    name: "Main",
    identity_kind: "git_repo",
    identity_payload: { repo_key: "github.com/example/app", branch: "main" },
    resolution_policy: "prefer_online",
    default_binding_id: null,
    status: "ready",
    mount_capabilities: ["read", "write", "list", "search", "exec"],
    bindings: [],
    created_at: "2026-05-17T00:00:00Z",
    updated_at: "2026-05-17T00:00:00Z",
    ...overrides,
  };
}

describe("workspaceRouting", () => {
  it("summarizes git identity with branch", () => {
    expect(identitySummary("git_repo", { repo_key: "repo", branch: "dev" })).toBe("repo · dev");
  });

  it("blocks resolution when workspace has no bindings", () => {
    const summary = summarizeResolution(workspace(), [], []);
    expect(summary.state).toBe("blocked");
    expect(summary.binding).toBeNull();
  });

  it("selects online authorized ready binding", () => {
    const target = workspace({
      bindings: [{
        id: "binding-1",
        workspace_id: "workspace-1",
        backend_id: "backend-1",
        root_ref: "D:/Repo",
        status: "ready",
        detected_facts: {},
        priority: 0,
        created_at: "2026-05-17T00:00:00Z",
        updated_at: "2026-05-17T00:00:00Z",
        last_verified_at: null,
      }],
    });
    const summary = summarizeResolution(target, [backend("backend-1", true)], [access("backend-1")]);
    expect(summary.state).toBe("resolved");
    expect(summary.binding?.id).toBe("binding-1");
  });

  it("counts only ready authorized online bindings as online availability", () => {
    const target = workspace({
      bindings: [
        {
          id: "binding-1",
          workspace_id: "workspace-1",
          backend_id: "backend-1",
          root_ref: "D:/Repo",
          status: "ready",
          detected_facts: {},
          priority: 0,
          created_at: "2026-05-17T00:00:00Z",
          updated_at: "2026-05-17T00:00:00Z",
          last_verified_at: null,
        },
        {
          id: "binding-2",
          workspace_id: "workspace-1",
          backend_id: "backend-2",
          root_ref: "E:/Repo",
          status: "ready",
          detected_facts: {},
          priority: 0,
          created_at: "2026-05-17T00:00:00Z",
          updated_at: "2026-05-17T00:00:00Z",
          last_verified_at: null,
        },
      ],
    });
    const summary = summarizeAvailability(
      target,
      [backend("backend-1", true), backend("backend-2", false)],
      [access("backend-1"), access("backend-2")],
    );
    expect(summary).toEqual({ total: 2, ready: 2, online: 1, authorized: 2 });
  });

  it("creates a workspace draft from candidate", () => {
    const candidate: WorkspaceInventoryCandidate = {
      backend_id: "backend-1",
      root_ref: "D:/Workspaces/App",
      identity_kind: "local_dir",
      identity_payload: { path_key: "d:/workspaces/app" },
      detected_facts: { local: true },
      status: "available",
      matched_workspace_ids: [],
      reason: "未匹配",
    };
    expect(candidateToDraft(candidate)).toEqual({
      name: "App",
      identity_kind: "local_dir",
      identity_payload: { path_key: "d:/workspaces/app" },
      binding: {
        id: "generated-binding-id",
        backend_id: "backend-1",
        root_ref: "D:/Workspaces/App",
        status: "ready",
        detected_facts: { local: true },
        priority: 0,
      },
    });
  });
});
