import { describe, expect, it } from "vitest";

import {
  mapExtensionRuntimeInvokeActionResponse,
  mapExtensionRuntimeInvokeChannelResponse,
  mapExtensionRuntimeProjection,
  mapUninstallExtensionInstallationResponse,
} from "./extensionRuntime";

describe("extension runtime mapper", () => {
  it("空响应归一化为空 projection", () => {
    expect(mapExtensionRuntimeProjection(null)).toEqual({
      installations: [],
      commands: [],
      flags: [],
      message_renderers: [],
      runtime_actions: [],
      protocol_channels: [],
      extension_dependencies: [],
      workspace_tabs: [],
      permissions: [],
      bundles: [],
    });
  });

  it("解析 Project 级 extension runtime projection", () => {
    const projection = mapExtensionRuntimeProjection({
      installations: [{
        installation_id: "installation-1",
        extension_key: "local-hello",
        extension_id: "local-hello",
        display_name: "Local Hello",
        installed_source: {
          library_asset_id: "asset-1",
          source_ref: "plugin:local-hello",
          source_version: "0.1.0",
          source_digest: "sha256:digest",
          installed_at: "2026-05-26T00:00:00Z",
        },
        package_artifact: null,
      }, {
        installation_id: "installation-2",
        extension_key: "packaged-hello",
        extension_id: "packaged-hello",
        display_name: "Packaged Hello",
        installed_source: null,
        package_artifact: {
          artifact_id: "artifact-1",
          package_name: "@agentdash/local-hello",
          package_version: "0.1.0",
          asset_version: "2026.05.26",
          source_version: "0.1.0",
          storage_ref: "extension-packages/project-1/digest.agentdash-extension.tgz",
          archive_digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
          manifest_digest: "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
        },
      }],
      commands: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        name: "local-hello:run",
        description: "Run",
        handler: { kind: "inject_message", content: "hello" },
      }],
      flags: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        name: "local-hello.verbose",
        flag_type: "bool",
        default: false,
        description: "Verbose",
      }],
      message_renderers: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        custom_type: "local-hello.card",
        renderer: { kind: "json_card" },
      }],
      runtime_actions: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        action_key: "local-hello.profile",
        kind: "session_runtime",
        description: "Read profile",
        input_schema: {},
        output_schema: {},
        permissions: ["local.profile.read"],
      }],
      protocol_channels: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        channel_key: "local-hello.api",
        version: "1.0.0",
        description: "Local API",
        methods: [{
          name: "echo",
          description: "Echo",
          input_schema: true,
          output_schema: true,
          permissions: [],
        }],
      }],
      extension_dependencies: [{
        extension_key: "packaged-hello",
        extension_id: "packaged-hello",
        dependency: {
          alias: "hello",
          extension_id: "local-hello",
          version: "^1.0.0",
          channels: ["local-hello.api"],
        },
      }],
      workspace_tabs: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        type_id: "local-hello.profile-panel",
        label: "Profile",
        uri_scheme: "local-hello",
        renderer: { kind: "webview", entry: "dist/panel/index.html" },
      }, {
        extension_key: "packaged-canvas",
        extension_id: "packaged-canvas",
        type_id: "packaged-canvas.panel",
        label: "Canvas",
        uri_scheme: "packaged-canvas",
        renderer: { kind: "canvas_panel", entry: "dist/canvas/runtime-snapshot.json" },
      }],
      permissions: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        permission: { kind: "local_profile", access: "read" },
      }],
      bundles: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        kind: "extension_host",
        entry: "dist/extension.js",
        digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      }],
    });

    expect(projection.runtime_actions[0].action_key).toBe("local-hello.profile");
    expect(projection.protocol_channels[0].channel_key).toBe("local-hello.api");
    expect(projection.extension_dependencies[0].dependency.alias).toBe("hello");
    expect(projection.installations[0].installed_source?.source_ref).toBe("plugin:local-hello");
    expect(projection.installations[0].package_artifact).toBeNull();
    expect(projection.installations[1].installed_source).toBeNull();
    expect(projection.installations[1].package_artifact?.artifact_id).toBe("artifact-1");
    expect(projection.workspace_tabs[0].renderer).toEqual({
      kind: "webview",
      entry: "dist/panel/index.html",
    });
    expect(projection.workspace_tabs[1].renderer).toEqual({
      kind: "canvas_panel",
      entry: "dist/canvas/runtime-snapshot.json",
    });
    expect(projection.permissions[0].permission).toEqual({
      kind: "local_profile",
      access: "read",
    });
  });

  it("非法 shape 会被拒绝", () => {
    expect(() => mapExtensionRuntimeProjection({
      runtime_actions: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        action_key: "local-hello.profile",
        kind: "unknown",
        description: "Read profile",
        input_schema: {},
        output_schema: {},
      }],
    })).toThrow(/action kind/);
  });

  it("解析 uninstall extension installation 响应", () => {
    const response = mapUninstallExtensionInstallationResponse({
      installation_id: "installation-1",
      extension_key: "local-hello",
    });
    expect(response.installation_id).toBe("installation-1");
    expect(response.extension_key).toBe("local-hello");
  });

  it("uninstall 响应缺字段时拒绝", () => {
    expect(() =>
      mapUninstallExtensionInstallationResponse({ installation_id: "x" }),
    ).toThrow();
  });

  it("解析 extension runtime invoke response 并归一化 metadata", () => {
    const response = mapExtensionRuntimeInvokeActionResponse({
      action_key: "local-hello.profile",
      trace: {
        trace_id: "trace-1",
        invocation_id: "rtinv-1",
        created_at: "2026-05-26T00:00:00Z",
      },
      output: {
        output: { username: "local-user" },
      },
    });

    expect(response.output.output).toEqual({ username: "local-user" });
    expect(response.output.metadata).toEqual({});
  });

  it("解析 extension runtime channel invoke response", () => {
    const response = mapExtensionRuntimeInvokeChannelResponse({
      channel_key: "protocol-demo.api",
      method: "greet",
      trace: {
        trace_id: "trace-1",
        invocation_id: "rtinv-1",
        created_at: "2026-05-26T00:00:00Z",
      },
      output: {
        output: { greeting: "hello" },
        metadata: { dependency_alias: "demo" },
      },
    });

    expect(response.channel_key).toBe("protocol-demo.api");
    expect(response.method).toBe("greet");
    expect(response.output.output).toEqual({ greeting: "hello" });
    expect(response.output.metadata).toEqual({ dependency_alias: "demo" });
  });
});
