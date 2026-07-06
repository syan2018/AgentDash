import type {
  DesktopAutostartStatus,
  DesktopRuntimeSettings,
  DesktopUpdatePolicySnapshot,
  LocalRuntimeClient,
  LocalRuntimeProfile,
  LocalRuntimeStatus,
  RuntimeStartRequest,
} from "@agentdash/core/local-runtime";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./defaults", () => ({
  ensureDesktopDefaultsLoaded: vi.fn(async () => ({})),
  resolveDefaultLocalRuntimeServerUrl: vi.fn(() => "http://10.22.71.7:8080"),
}));

function createProfile(autoStart: boolean, serverUrl = "http://127.0.0.1:3001"): LocalRuntimeProfile {
  return {
    server_url: serverUrl,
    access_token: "",
    profile_id: "default",
    machine_id: "machine-local",
    machine_label: "Local Desktop",
    name: "Desktop Local Runtime",
    workspace_roots: [],
    executor_enabled: true,
    auto_start: autoStart,
    backend_id: null,
    relay_ws_url: null,
  };
}

function createStatus(state: LocalRuntimeStatus["state"]): LocalRuntimeStatus {
  return {
    state,
    owner: "desktop_embedded_runner",
    registration_source: "desktop_access_token",
    backend_id: state === "running" ? "backend-local" : "",
    name: "Desktop Local Runtime",
    workspace_roots: [],
    executor_enabled: true,
    mcp_server_count: 0,
    capability_health: [],
    message: null,
    last_error: null,
    last_attempt_at: null,
    next_retry_at: null,
    retry_count: null,
    relay_connection: null,
    registration: null,
  };
}

function installDesktopBridge(client: LocalRuntimeClient): void {
  const settings: DesktopRuntimeSettings = {
    launch_at_login: false,
    start_minimized_to_tray: false,
    auto_connect_local_runtime: true,
  };
  const autostart: DesktopAutostartStatus = {
    supported: true,
    enabled: false,
    message: null,
  };
  const updatePolicy: DesktopUpdatePolicySnapshot = {
    current_version: "0.1.0",
    status: "ready",
    force_update_required: false,
    checked_at: null,
    latest_version: null,
    min_desktop_version: null,
    recommended_desktop_version: null,
    update_available: null,
    manifest_url_configured: false,
    diagnostics_code: "desktop_manifest_url_unconfigured",
    diagnostics_message: null,
    last_error: null,
  };
  const windowMock = {
    __AGENTDASH_DESKTOP_LOCAL_RUNTIME__: client,
    __AGENTDASH_DESKTOP_APP__: {
      loadSettings: vi.fn(async () => settings),
      saveSettings: vi.fn(async () => settings),
      getAutostartStatus: vi.fn(async () => autostart),
      setAutostartEnabled: vi.fn(async () => autostart),
      getDesktopApiSnapshot: vi.fn(async () => null),
      getUpdatePolicySnapshot: vi.fn(async () => updatePolicy),
      refreshUpdatePolicy: vi.fn(async () => updatePolicy),
      installUpdate: vi.fn(async () => ({
        installed: false,
        version: null,
        message: "当前没有可安装的桌面更新",
      })),
      quit: vi.fn(async () => undefined),
    },
    setTimeout: globalThis.setTimeout,
    clearTimeout: globalThis.clearTimeout,
  };

  Object.defineProperty(globalThis, "window", {
    value: windowMock,
    configurable: true,
  });
}

function createClient(runtimeStart: (request: RuntimeStartRequest) => Promise<LocalRuntimeStatus>): LocalRuntimeClient {
  const profile = createProfile(true);
  return {
    profileLoad: vi.fn(async () => profile),
    profileSave: vi.fn(async (nextProfile: LocalRuntimeProfile) => nextProfile),
    profileDelete: vi.fn(async () => undefined),
    runtimeSnapshot: vi.fn(async () => createStatus("idle")),
    runtimeStart: vi.fn(runtimeStart),
    runtimeStop: vi.fn(async () => undefined),
    runtimeRestart: vi.fn(async () => createStatus("running")),
    logsTail: vi.fn(async () => []),
    logsClear: vi.fn(async () => undefined),
    mcpServersLoad: vi.fn(async () => []),
    mcpServersSave: vi.fn(async () => undefined),
    mcpServerProbe: vi.fn(async () => ({ ok: true, tool_count: 0, message: "ok" })),
  };
}

describe("desktop local runtime bridge", () => {
  beforeEach(() => {
    vi.resetModules();
    Reflect.deleteProperty(globalThis, "window");
  });

  it("未拿到 currentUser 时不会用空 token 启动 native runtime", async () => {
    const runtimeStart = vi.fn(async () => createStatus("running"));
    const client = createClient(runtimeStart);
    installDesktopBridge(client);

    const { ensureDesktopLocalRuntimeStarted } = await import("./localRuntimeBridge");
    await ensureDesktopLocalRuntimeStarted("", { currentUserAvailable: false });

    expect(runtimeStart).not.toHaveBeenCalled();
  });

  it("非空 bearer token 的旧式调用默认视为已登录 intent", async () => {
    const runtimeStart = vi.fn(async () => createStatus("running"));
    const client = createClient(runtimeStart);
    installDesktopBridge(client);

    const { ensureDesktopLocalRuntimeStarted } = await import("./localRuntimeBridge");
    await ensureDesktopLocalRuntimeStarted("token-current");

    expect(runtimeStart).toHaveBeenCalledWith(expect.objectContaining({
      access_token: "token-current",
    }));
  });

  it("currentUser 已存在但 bearer token 为空时仍会请求 native ensure", async () => {
    const runtimeStart = vi.fn(async () => createStatus("waiting_for_api"));
    const client = createClient(runtimeStart);
    installDesktopBridge(client);

    const { ensureDesktopLocalRuntimeStarted } = await import("./localRuntimeBridge");
    await ensureDesktopLocalRuntimeStarted("", { currentUserAvailable: true });

    expect(runtimeStart).toHaveBeenCalledWith(expect.objectContaining({
      access_token: "",
      server_url: "http://10.22.71.7:8080",
    }));
  });

  it("打包默认后端会覆盖旧 profile 中的开发默认 server URL", async () => {
    const runtimeStart = vi.fn(async () => createStatus("running"));
    const client = createClient(runtimeStart);
    installDesktopBridge(client);

    const { ensureDesktopLocalRuntimeStarted } = await import("./localRuntimeBridge");
    await ensureDesktopLocalRuntimeStarted("token-current");

    expect(runtimeStart).toHaveBeenCalledWith(expect.objectContaining({
      server_url: "http://10.22.71.7:8080",
    }));
  });

  it("旧 profile 的非默认 server URL 不会覆盖当前 Dashboard API origin", async () => {
    const runtimeStart = vi.fn(async () => createStatus("running"));
    const client = createClient(runtimeStart);
    vi.mocked(client.profileLoad).mockResolvedValue(createProfile(true, "http://192.168.1.9:9000"));
    installDesktopBridge(client);

    const { ensureDesktopLocalRuntimeStarted } = await import("./localRuntimeBridge");
    await ensureDesktopLocalRuntimeStarted("token-current");

    expect(runtimeStart).toHaveBeenCalledWith(expect.objectContaining({
      server_url: "http://10.22.71.7:8080",
    }));
  });

  it("强制更新状态下不会通过 Web bridge 自动启动 native runtime", async () => {
    const runtimeStart = vi.fn(async () => createStatus("running"));
    const client = createClient(runtimeStart);
    installDesktopBridge(client);
    const desktopApp = window.__AGENTDASH_DESKTOP_APP__;
    expect(desktopApp).toBeDefined();
    if (!desktopApp) {
      throw new Error("desktop app bridge should be installed");
    }
    vi.mocked(desktopApp.getUpdatePolicySnapshot).mockResolvedValue({
      current_version: "0.1.0",
      status: "force_update_required",
      force_update_required: true,
      checked_at: "2026-07-06T00:00:00Z",
      latest_version: "0.2.0",
      min_desktop_version: "0.2.0",
      recommended_desktop_version: "0.2.0",
      update_available: true,
      manifest_url_configured: true,
      diagnostics_code: "update_available",
      diagnostics_message: null,
      last_error: null,
    });

    const { ensureDesktopLocalRuntimeStarted } = await import("./localRuntimeBridge");
    await ensureDesktopLocalRuntimeStarted("token-current", { currentUserAvailable: true });

    expect(runtimeStart).not.toHaveBeenCalled();
  });
});
