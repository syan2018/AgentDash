import { describe, expect, it } from "vitest";
import {
  createRuntimeDiagnosticsSnapshot,
  redactRuntimeDiagnosticText,
  type LocalRuntimeStatus,
} from "@agentdash/core/local-runtime";
import type { BackendConfig, BackendRuntimeSummary } from "../../../types";
import {
  backendDiagnosticsFacts,
  createCloudApiDiagnosticsInput,
  runtimeSummaryDiagnosticsFacts,
} from "./runtimeDiagnostics";

describe("runtime diagnostics view model", () => {
  it("uses explicit backend registration_source and does not infer relay state from online", () => {
    const localRuntime: LocalRuntimeStatus = {
      state: "running",
      owner: "desktop_embedded_runner",
      registration_source: "desktop_access_token",
      backend_id: "backend-local",
      name: "desktop-local",
      workspace_roots: [],
      executor_enabled: true,
      mcp_server_count: 0,
      capability_health: [],
      message: null,
      last_error: null,
      last_attempt_at: null,
      next_retry_at: null,
      retry_count: null,
    };
    const snapshot = createRuntimeDiagnosticsSnapshot({
      generated_at: "2026-06-26T00:00:00.000Z",
      cloud_api: createCloudApiDiagnosticsInput({
        apiError: null,
        isChecking: false,
        target: "http://127.0.0.1:17301",
        eventConnectionState: "connected",
      }),
      local_runtime: localRuntime,
      backends: backendDiagnosticsFacts([backend("backend-local", "desktop_access_token", true)]),
      runtime_summaries: runtimeSummaryDiagnosticsFacts([summary("backend-local", true)]),
      logs: [],
      settings: null,
    });

    expect(snapshot.registration?.source).toBe("desktop_access_token");
    expect(snapshot.registration?.backend_id).toBe("backend-local");
    expect(snapshot.relay_connection).toBeNull();
  });

  it("projects native supervisor retry facts without leaking token fields", () => {
    const localRuntime: LocalRuntimeStatus = {
      state: "waiting_for_api",
      owner: "desktop_embedded_runner",
      registration_source: "desktop_access_token",
      backend_id: "",
      name: "Desktop Local Runtime",
      workspace_roots: ["D:/work"],
      executor_enabled: true,
      mcp_server_count: 0,
      capability_health: [],
      message: "Dashboard API 暂不可用",
      last_error: "connect failed",
      last_attempt_at: "2026-06-26T00:00:00Z",
      next_retry_at: "2026-06-26T00:00:01Z",
      retry_count: 1,
    };
    const snapshot = createRuntimeDiagnosticsSnapshot({
      generated_at: "2026-06-26T00:00:00.000Z",
      cloud_api: {
        state: "healthy",
        target: "https://agentdash.example",
        message: null,
        event_stream_state: "connected",
      },
      local_runtime: localRuntime,
      backends: [],
      runtime_summaries: [],
      logs: [],
      settings: null,
    });

    expect(snapshot.local_runtime?.raw_state).toBe("waiting_for_api");
    expect(snapshot.local_runtime?.state).toBe("checking");
    expect(snapshot.local_runtime?.last_error).toBe("connect failed");
    expect(snapshot.local_runtime?.retry_count).toBe(1);
    expect(snapshot.registration?.source).toBe("desktop_access_token");
  });

  it("projects an independent runner as read-only runner layer", () => {
    const snapshot = createRuntimeDiagnosticsSnapshot({
      generated_at: "2026-06-26T00:00:00.000Z",
      cloud_api: {
        state: "healthy",
        target: "https://agentdash.example",
        message: null,
        event_stream_state: "connected",
      },
      local_runtime: null,
      backends: backendDiagnosticsFacts([backend("runner-1", "runner_registration_token", true)]),
      runtime_summaries: runtimeSummaryDiagnosticsFacts([summary("runner-1", false)]),
      logs: [],
      settings: null,
    });

    expect(snapshot.runner?.backend_id).toBe("runner-1");
    expect(snapshot.runner?.state).toBe("degraded");
    expect(snapshot.registration?.source).toBe("runner_registration_token");
    expect(snapshot.local_runtime?.state).toBe("disabled");
  });

  it("does not treat a stopped desktop local backend as an independent runner", () => {
    const snapshot = createRuntimeDiagnosticsSnapshot({
      generated_at: "2026-06-26T00:00:00.000Z",
      cloud_api: {
        state: "healthy",
        target: "https://agentdash.example",
        message: null,
        event_stream_state: "connected",
      },
      local_runtime: null,
      backends: backendDiagnosticsFacts([backend("desktop-local", "desktop_access_token", false)]),
      runtime_summaries: runtimeSummaryDiagnosticsFacts([summary("desktop-local", false)]),
      logs: [],
      settings: null,
    });

    expect(snapshot.runner).toBeNull();
    expect(snapshot.registration).toBeNull();
    expect(snapshot.local_runtime?.state).toBe("disabled");
  });

  it("redacts copied diagnostic text at the frontend boundary", () => {
    const raw = [
      "Authorization: Bearer access-secret",
      "wss://example.test/ws?token=relay&relay_token=relay2",
      "{\"registration_token\":\"adrt_secret\",\"auth_token\":\"auth\"}",
      "ACCESS_TOKEN=upper",
    ].join("\n");

    expect(redactRuntimeDiagnosticText(raw)).toBe([
      "Authorization: Bearer ***",
      "wss://example.test/ws?token=***&relay_token=***",
      "{\"registration_token\":\"***\",\"auth_token\":\"***\"}",
      "ACCESS_TOKEN=***",
    ].join("\n"));
  });
});

function backend(
  id: string,
  registration_source: "desktop_access_token" | "runner_registration_token",
  online: boolean,
): BackendConfig {
  return {
    id,
    name: id,
    endpoint: "",
    enabled: true,
    backend_type: "local",
    owner_user_id: null,
    profile_id: "default",
    device_id: null,
    machine_id: "machine-1",
    machine_label: "Workstation",
    visibility: "private",
    share_scope_kind: "user",
    share_scope_id: null,
    capability_slot: "default",
    device: {},
    last_claimed_at: "2026-06-26T00:00:00Z",
    registration_source,
    online,
    runtime_health: {
      backend_id: id,
      profile_id: "default",
      name: id,
      status: online ? "online" : "offline",
      online,
      version: "0.1.0",
      capabilities: {},
      device: {},
      connected_at: online ? "2026-06-26T00:00:00Z" : null,
      last_seen_at: "2026-06-26T00:00:00Z",
      disconnected_at: null,
      disconnect_reason: null,
      created_at: "2026-06-26T00:00:00Z",
      updated_at: "2026-06-26T00:00:00Z",
    },
    capabilities: null,
  };
}

function summary(backend_id: string, allocatable: boolean): BackendRuntimeSummary {
  return {
    backend_id,
    name: backend_id,
    enabled: true,
    online: true,
    runtime_health: null,
    executors: [],
    capability_health: [],
    active_session_count: allocatable ? 0 : 1,
    active_sessions: [],
    allocatable,
  };
}
