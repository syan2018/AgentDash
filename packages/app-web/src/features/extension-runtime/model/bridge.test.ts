import { describe, expect, it } from "vitest";

import { parseExtensionBridgeMessage, toJsonValue } from "./bridge";
import type { JsonValue } from "../../../generated/common-contracts";
import type {
  ExtensionRuntimeInvokeActionRequest,
  ExtensionRuntimeInvokeActionResponse,
  ExtensionRuntimeInvokeChannelRequest,
  ExtensionRuntimeInvokeChannelResponse,
  ExtensionWorkspaceTabProjectionResponse,
} from "../../../generated/extension-runtime-contracts";
import type { WorkspaceData } from "../../workspace-runtime";
import type { ExtensionBridgeRequestMessage } from "./bridge";
import {
  handleExtensionWebviewBridgeRequest,
  resolveExtensionWebviewAvailability,
  type ExtensionWebviewBridgeServices,
} from "./webviewBridge";
import { invokeExtensionChannelFromCanvas } from "./canvasBridge";

describe("extension bridge message validation", () => {
  it("只接受 agentdash extension request message", () => {
    const message = parseExtensionBridgeMessage({
      channel: "agentdash.extension",
      kind: "request",
      request_id: "request-1",
      method: "runtime.invoke_action",
      params: {
        action_key: "local-hello.profile",
      },
    });

    expect(message).toEqual({
      channel: "agentdash.extension",
      kind: "request",
      request_id: "request-1",
      method: "runtime.invoke_action",
      params: {
        action_key: "local-hello.profile",
      },
    });
    expect(parseExtensionBridgeMessage({ channel: "other" })).toBeNull();
    expect(parseExtensionBridgeMessage({
      channel: "agentdash.extension",
      kind: "request",
      request_id: "",
      method: "runtime.invoke_action",
    })).toBeNull();
  });

  it("把 bridge payload 归一化为 JSON value", () => {
    expect(toJsonValue({
      ok: true,
      value: Number.NaN,
      list: [1, undefined],
    })).toEqual({
      ok: true,
      value: null,
      list: [1, null],
    });
  });

  it("为 webview runtime action、channel 和 VFS 请求组装宿主上下文", async () => {
    const actionCalls: Array<{
      projectId: string;
      request: ExtensionRuntimeInvokeActionRequest;
    }> = [];
    const channelCalls: Array<{
      projectId: string;
      request: ExtensionRuntimeInvokeChannelRequest;
    }> = [];
    const openTabCalls: Array<{ typeId: string; uri: string }> = [];
    const readCalls: Array<{ surfaceRef: string; mountId: string; path: string }> = [];
    const writeCalls: Array<{
      surfaceRef: string;
      mountId: string;
      path: string;
      content: string;
    }> = [];
    const services: ExtensionWebviewBridgeServices = {
      openTab(typeId, uri) {
        openTabCalls.push({ typeId, uri });
      },
      async invokeAction(projectId, request) {
        actionCalls.push({ projectId, request });
        return actionResponse(request.action_key, { ok: true });
      },
      async invokeChannel(projectId, request) {
        channelCalls.push({ projectId, request });
        return channelResponse(request.channel_key, request.method, { channel: true });
      },
      async readFile(request) {
        readCalls.push(request);
        return { content: "hello" };
      },
      async writeFile(request) {
        writeCalls.push(request);
      },
    };
    const workspaceData = workspaceRuntimeData();
    const tab = webviewTab();
    const backend = { backend_id: "backend-1", label: "Local", online: true };

    await expect(handleExtensionWebviewBridgeRequest({
      message: bridgeRequest("runtime.invoke_action", {
        action_key: "protocol-demo.greet",
        input: { value: Number.NaN },
      }),
      workspaceData,
      tab,
      uri: "protocol-demo://panel",
      backend,
      services,
    })).resolves.toEqual({ ok: true });
    expect(actionCalls).toEqual([{
      projectId: "project-1",
      request: {
        session_id: "session-1",
        backend_id: "backend-1",
        action_key: "protocol-demo.greet",
        input: { value: null },
      },
    }]);

    await expect(handleExtensionWebviewBridgeRequest({
      message: bridgeRequest("extension.invoke_channel", {
        channel_key: "api",
        method: "greet",
        dependency_alias: " demo ",
        input: { source: "panel" },
      }),
      workspaceData,
      tab,
      uri: "protocol-demo://panel",
      backend,
      services,
    })).resolves.toEqual({ channel: true });
    expect(channelCalls).toEqual([{
      projectId: "project-1",
      request: {
        session_id: "session-1",
        backend_id: "backend-1",
        channel_key: "api",
        method: "greet",
        input: { source: "panel" },
        consumer_extension_key: "protocol-demo",
        dependency_alias: "demo",
      },
    }]);

    await expect(handleExtensionWebviewBridgeRequest({
      message: bridgeRequest("vfs.read", { path: "notes/hello.txt" }),
      workspaceData,
      tab,
      uri: "protocol-demo://panel",
      backend,
      services,
    })).resolves.toBe("hello");
    await expect(handleExtensionWebviewBridgeRequest({
      message: bridgeRequest("vfs.write", {
        path: "notes/hello.txt",
        content: "hello",
      }),
      workspaceData,
      tab,
      uri: "protocol-demo://panel",
      backend,
      services,
    })).resolves.toBeNull();
    expect(readCalls).toEqual([{
      surfaceRef: "surface-1",
      mountId: "mount-1",
      path: "notes/hello.txt",
    }]);
    expect(writeCalls).toEqual([{
      surfaceRef: "surface-1",
      mountId: "mount-1",
      path: "notes/hello.txt",
      content: "hello",
    }]);

    await expect(handleExtensionWebviewBridgeRequest({
      message: bridgeRequest("workspace.open_tab", {
        type_id: "protocol-demo.panel",
        uri: "protocol-demo://panel",
      }),
      workspaceData,
      tab,
      uri: "protocol-demo://panel",
      backend,
      services,
    })).resolves.toBeNull();
    expect(openTabCalls).toEqual([{
      typeId: "protocol-demo.panel",
      uri: "protocol-demo://panel",
    }]);
  });

  it("为未知 method 和 admission error 保留可诊断错误", async () => {
    const services = noopServices();
    const workspaceData = workspaceRuntimeData();
    const tab = webviewTab();
    const backend = { backend_id: "backend-1", label: "Local", online: true };

    await expect(handleExtensionWebviewBridgeRequest({
      message: bridgeRequest("runtime.unknown", {}),
      workspaceData,
      tab,
      uri: "protocol-demo://panel",
      backend,
      services,
    })).rejects.toThrow("未知 Extension bridge method: runtime.unknown");

    await expect(handleExtensionWebviewBridgeRequest({
      message: bridgeRequest("runtime.invoke_action", {
        action_key: "other-extension.action",
      }),
      workspaceData,
      tab,
      uri: "protocol-demo://panel",
      backend,
      services,
    })).rejects.toThrow("Extension action 不可用: other-extension.action");

    expect(resolveExtensionWebviewAvailability(
      workspaceRuntimeData({
        runtimeSurface: {
          ...runtimeSurface(),
          mounts: [{ ...runtimeMount(), backend_online: false }],
        },
      }),
      tab,
    )).toMatchObject({
      available: false,
      title: "Backend 离线",
      backend: { backend_id: "backend-1", online: false },
    });
  });

  it("为 Canvas-like consumer 组装 extension channel request", async () => {
    const calls: Array<{
      projectId: string;
      request: ExtensionRuntimeInvokeChannelRequest;
    }> = [];
    const result = await invokeExtensionChannelFromCanvas({
      workspaceData: workspaceRuntimeData(),
      tab: canvasTab(),
      request: {
        channel_key: "api",
        method: "greet",
        input: { value: Number.NaN },
        dependency_alias: "demo",
      },
      async invokeChannel(projectId, request) {
        calls.push({ projectId, request });
        return channelResponse(request.channel_key, request.method, { ok: true });
      },
    });

    expect(result).toEqual({ ok: true });
    expect(calls).toEqual([{
      projectId: "project-1",
      request: {
        session_id: "session-1",
        backend_id: "backend-1",
        channel_key: "api",
        method: "greet",
        input: { value: null },
        consumer_extension_key: "protocol-demo",
        dependency_alias: "demo",
      },
    }]);

    await expect(invokeExtensionChannelFromCanvas({
      workspaceData: workspaceRuntimeData({
        runtimeSurface: {
          ...runtimeSurface(),
          mounts: [{ ...runtimeMount(), backend_online: false }],
        },
      }),
      tab: canvasTab(),
      request: {
        channel_key: "api",
        method: "greet",
        input: null,
      },
      async invokeChannel() {
        throw new Error("unexpected invoke");
      },
    })).rejects.toThrow("Canvas extension channel 缺少可用 backend");
  });
});

function bridgeRequest(
  method: string,
  params: Record<string, unknown>,
): ExtensionBridgeRequestMessage {
  return {
    channel: "agentdash.extension",
    kind: "request",
    request_id: "request-1",
    method,
    params,
  };
}

function noopServices(): ExtensionWebviewBridgeServices {
  return {
    openTab() {},
    async invokeAction(_projectId, request) {
      return actionResponse(request.action_key, null);
    },
    async invokeChannel(_projectId, request) {
      return channelResponse(request.channel_key, request.method, null);
    },
    async readFile() {
      return { content: "" };
    },
    async writeFile() {},
  };
}

function actionResponse(
  actionKey: string,
  output: JsonValue,
): ExtensionRuntimeInvokeActionResponse {
  return {
    action_key: actionKey,
    trace: runtimeTrace(),
    output: {
      output,
      metadata: {},
    },
  };
}

function channelResponse(
  channelKey: string,
  method: string,
  output: JsonValue,
): ExtensionRuntimeInvokeChannelResponse {
  return {
    channel_key: channelKey,
    method,
    trace: runtimeTrace(),
    output: {
      output,
      metadata: {},
    },
  };
}

function runtimeTrace() {
  return {
    trace_id: "trace-1",
    invocation_id: "invoke-1",
    parent_trace_id: null,
    created_at: "2026-05-27T00:00:00Z",
  };
}

function workspaceRuntimeData(overrides: Partial<WorkspaceData> = {}): WorkspaceData {
  return {
    projectId: "project-1",
    sessionId: "session-1",
    runtimeStatus: "ready",
    runtimeError: null,
    extensionRuntime: {
      project_id: "project-1",
      status: "ready",
      projection: {
        installations: [{
          installation_id: "installation-1",
          extension_key: "protocol-demo",
          extension_id: "protocol-demo",
          display_name: "Protocol Demo",
          installed_source: null,
          package_artifact: {
            artifact_id: "artifact-1",
            package_name: "@agentdash/protocol-demo",
            package_version: "0.1.0",
            asset_version: "0.1.0",
            source_version: "0.1.0",
            storage_ref: "extensions/protocol-demo.tgz",
            archive_digest: "sha256:archive",
            manifest_digest: "sha256:manifest",
          },
        }],
        commands: [],
        flags: [],
        message_renderers: [],
        runtime_actions: [{
          extension_key: "protocol-demo",
          extension_id: "protocol-demo",
          action_key: "protocol-demo.greet",
          kind: "session_runtime",
          description: "Greet",
          input_schema: true,
          output_schema: true,
          permissions: [],
        }],
        protocol_channels: [],
        extension_dependencies: [],
        workspace_tabs: [],
        permissions: [],
        bundles: [],
      },
      error: null,
    },
    contextSnapshot: null,
    ownerStory: null,
    ownerProjectName: "Project",
    executorSummary: null,
    runtimeSurface: runtimeSurface(),
    workspaceBackend: null,
    hookRuntime: null,
    sessionCapabilities: null,
    lifecycleRuns: [],
    activeCanvasId: null,
    ...overrides,
  };
}

function runtimeSurface(): NonNullable<WorkspaceData["runtimeSurface"]> {
  return {
    surface_ref: "surface-1",
    source: { source_type: "session_runtime", session_id: "session-1" },
    default_mount_id: "mount-1",
    mounts: [runtimeMount()],
  };
}

function runtimeMount() {
  return {
    id: "mount-1",
    display_name: "Local",
    provider: "local",
    backend_id: "backend-1",
    capabilities: ["read", "write", "list"],
    default_write: true,
    purpose: "workspace",
    backend_online: true,
    edit_capabilities: {
      create: true,
      delete: true,
      rename: true,
    },
  } satisfies NonNullable<WorkspaceData["runtimeSurface"]>["mounts"][number];
}

function webviewTab(): ExtensionWorkspaceTabProjectionResponse {
  return {
    extension_key: "protocol-demo",
    extension_id: "protocol-demo",
    type_id: "protocol-demo.panel",
    label: "Protocol Demo",
    uri_scheme: "protocol-demo",
    renderer: {
      kind: "webview",
      entry: "dist/panel/index.html",
    },
  };
}

function canvasTab(): ExtensionWorkspaceTabProjectionResponse {
  return {
    ...webviewTab(),
    renderer: {
      kind: "canvas_panel",
      entry: "dist/canvas/runtime.json",
    },
  };
}
