import { describe, expect, it } from "vitest";
import type { BackendConfig, BackendRuntimeSummary } from "../types";
import { applyBackendRuntimeSummaries, backendAvailabilitySignature } from "./backendAvailability";

function backend(id: string, online: boolean): BackendConfig {
  return {
    id,
    name: id,
    endpoint: `ws://${id}`,
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
    registration_source: "desktop_access_token",
    online,
    runtime_health: null,
    capabilities: null,
  };
}

function summary(backendId: string, online: boolean): BackendRuntimeSummary {
  return {
    backend_id: backendId,
    name: backendId,
    enabled: true,
    online,
    runtime_health: {
      backend_id: backendId,
      profile_id: "default",
      name: backendId,
      status: online ? "online" : "offline",
      online,
      version: null,
      capabilities: {},
      device: {},
      connected_at: online ? "2026-06-29T00:00:00Z" : null,
      last_seen_at: online ? "2026-06-29T00:00:00Z" : null,
      disconnected_at: online ? null : "2026-06-29T00:00:00Z",
      disconnect_reason: null,
      created_at: "2026-06-29T00:00:00Z",
      updated_at: "2026-06-29T00:00:00Z",
    },
    executors: [],
    active_session_count: 0,
    active_sessions: [],
    allocatable: online,
  };
}

describe("backendAvailability", () => {
  it("uses runtime summary online as an authoritative live overlay", () => {
    const result = applyBackendRuntimeSummaries(
      [backend("desktop-local", false)],
      [summary("desktop-local", true)],
    );

    expect(result[0].online).toBe(true);
    expect(result[0].runtime_health?.status).toBe("online");
  });

  it("does not downgrade a backend registry online state with a stale offline summary", () => {
    const result = applyBackendRuntimeSummaries(
      [backend("desktop-local", true)],
      [summary("desktop-local", false)],
    );

    expect(result[0].online).toBe(true);
  });

  it("includes runtime summary facts in the availability signature", () => {
    const signature = backendAvailabilitySignature(
      [backend("desktop-local", false)],
      [summary("desktop-local", true)],
    );

    expect(signature).toContain("desktop-local:online:online:");
  });
});
