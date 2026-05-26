import { describe, expect, it } from "vitest";

import { mapExtensionRuntimeProjection } from "./extensionRuntime";

describe("extension runtime mapper", () => {
  it("空响应归一化为空 projection", () => {
    expect(mapExtensionRuntimeProjection(null)).toEqual({
      installations: [],
      commands: [],
      flags: [],
      message_renderers: [],
      runtime_actions: [],
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
      workspace_tabs: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        type_id: "local-hello.profile-panel",
        label: "Profile",
        uri_scheme: "local-hello",
        renderer: { kind: "webview", entry: "dist/panel/index.html" },
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
    expect(projection.workspace_tabs[0].renderer).toEqual({
      kind: "webview",
      entry: "dist/panel/index.html",
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
});
