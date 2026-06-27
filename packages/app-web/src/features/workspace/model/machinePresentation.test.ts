import { describe, expect, it } from "vitest";
import type { BackendConfig, ProjectBackendAccess, Workspace } from "../../../types";
import {
  classifyMachine,
  machineKindLabel,
  workspaceMachineAvailability,
} from "./machinePresentation";

function backend(overrides: Partial<BackendConfig> & { id: string }): BackendConfig {
  return {
    online: true,
    runtime_health: null,
    capabilities: null,
    name: overrides.id,
    endpoint: "",
    enabled: true,
    backend_type: "local",
    owner_user_id: null,
    profile_id: null,
    device_id: null,
    machine_id: null,
    machine_label: null,
    visibility: "private",
    share_scope_kind: "user",
    share_scope_id: null,
    capability_slot: "default",
    device: {},
    last_claimed_at: null,
    registration_source: null,
    ...overrides,
  } as BackendConfig;
}

function access(backendId: string, status: ProjectBackendAccess["status"] = "active"): ProjectBackendAccess {
  return {
    id: `acc-${backendId}`,
    project_id: "proj-1",
    backend_id: backendId,
    status,
    access_mode: "explicit_grant",
    priority: 0,
    root_policy: {},
    capability_policy: {},
    note: null,
    created_by: null,
    created_at: "2026-06-27T00:00:00.000Z",
    updated_at: "2026-06-27T00:00:00.000Z",
  };
}

function workspace(bindings: Workspace["bindings"]): Workspace {
  return {
    id: "ws-1",
    name: "ws",
    bindings,
  } as Workspace;
}

describe("classifyMachine", () => {
  it("badges desktop local runtime as the local device", () => {
    expect(classifyMachine({ registration_source: "desktop_access_token" })).toBe("local_device");
    expect(machineKindLabel("local_device")).toBe("本机（这台设备）");
  });

  it("badges runner registration token backends as server runners", () => {
    expect(classifyMachine({ registration_source: "runner_registration_token" })).toBe(
      "server_runner",
    );
    expect(machineKindLabel("server_runner")).toBe("服务器 runner");
  });

  it("falls back to other for unknown / missing source", () => {
    expect(classifyMachine({ registration_source: null })).toBe("other");
    expect(classifyMachine({ registration_source: "something_else" })).toBe("other");
  });
});

describe("workspaceMachineAvailability", () => {
  it("lists authorized machines, marks located ones, and orders local device first", () => {
    const backends = [
      backend({ id: "runner-a", name: "服务器A", registration_source: "runner_registration_token" }),
      backend({ id: "local-1", name: "我的电脑", registration_source: "desktop_access_token" }),
    ];
    const accesses = [access("runner-a"), access("local-1")];
    const ws = workspace([
      { id: "b1", backend_id: "local-1", root_ref: "/code", status: "ready", priority: 0 },
    ] as Workspace["bindings"]);

    const result = workspaceMachineAvailability(ws, backends, accesses);

    expect(result.map((entry) => entry.name)).toEqual(["我的电脑", "服务器A"]);
    expect(result[0]).toMatchObject({ kind: "local_device", located: true });
    expect(result[1]).toMatchObject({ kind: "server_runner", located: false });
  });

  it("excludes machines without an active grant", () => {
    const backends = [
      backend({ id: "runner-a", registration_source: "runner_registration_token" }),
      backend({ id: "runner-b", registration_source: "runner_registration_token" }),
    ];
    const accesses = [access("runner-a"), access("runner-b", "revoked")];
    const ws = workspace([] as Workspace["bindings"]);

    const result = workspaceMachineAvailability(ws, backends, accesses);

    expect(result.map((entry) => entry.backendId)).toEqual(["runner-a"]);
  });
});
