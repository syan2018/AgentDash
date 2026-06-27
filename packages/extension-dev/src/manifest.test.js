// @ts-check

import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { validateProject } from "./manifest.js";

test("validateProject accepts a packaged manifest shape", async () => {
  const root = await fixtureProject();
  const result = await validateProject(root);
  assert.deepEqual(result.errors, []);
});

test("validateProject accepts canvas panel renderer", async () => {
  const root = await fixtureProject({ rendererKind: "canvas_panel" });
  const result = await validateProject(root);
  assert.deepEqual(result.errors, []);
});

test("validateProject accepts protocol channels, dependencies, and trusted host capabilities", async () => {
  const root = await fixtureProject({ withProtocol: true });
  const result = await validateProject(root);
  assert.deepEqual(result.errors, []);
});

test("validateProject rejects unknown runtime process permission key", async () => {
  const root = await fixtureProject({ withProtocol: true, unknownRuntimePermission: "process.execute" });
  const result = await validateProject(root);
  assert.match(result.errors.join("\n"), /未知 permission key: process\.execute/);
});

test("validateProject rejects invalid protocol channel declarations", async () => {
  const root = await fixtureProject({ withInvalidProtocol: true });
  const result = await validateProject(root);
  assert.match(result.errors.join("\n"), /protocol_channels\[\]\.channel_key/);
  assert.match(result.errors.join("\n"), /protocol_channels\[\]\.methods\[\]\.name/);
  assert.match(result.errors.join("\n"), /extension_dependencies\[\]\.alias/);
});

test("validateProject rejects missing or null runtime schema declarations", async () => {
  const missing = await fixtureProject({ omitActionSchema: true });
  const missingResult = await validateProject(missing);
  assert.match(missingResult.errors.join("\n"), /runtime_actions\[\]\.input_schema 必须存在/);

  const nullSchema = await fixtureProject({ nullActionSchema: true });
  const nullResult = await validateProject(nullSchema);
  assert.match(nullResult.errors.join("\n"), /runtime_actions\[\]\.input_schema 必须是 JSON Schema 对象或布尔值/);
});

test("validateProject rejects lifecycle scripts and package mismatch", async () => {
  const root = await fixtureProject({
    packageName: "@agentdash/other",
    scripts: { postinstall: "node install.js" },
  });
  const result = await validateProject(root);
  assert.match(result.errors.join("\n"), /manifest\.package\.name/);
  assert.match(result.errors.join("\n"), /scripts\.postinstall/);
});

test("validateProject rejects non self-contained dependencies and native constraints", async () => {
  const root = await fixtureProject({
    dependencies: { sharp: "^0.34.0" },
    nativeFields: { os: ["darwin"], gypfile: true },
  });
  const result = await validateProject(root);
  assert.match(result.errors.join("\n"), /package\.json\.dependencies/);
  assert.match(result.errors.join("\n"), /package\.json\.os/);
  assert.match(result.errors.join("\n"), /package\.json\.gypfile/);
});

/**
 * @param {{ packageName?: string, scripts?: Record<string, string>, dependencies?: Record<string, string>, nativeFields?: Record<string, unknown>, rendererKind?: "webview" | "canvas_panel", withProtocol?: boolean, withInvalidProtocol?: boolean, unknownRuntimePermission?: string, omitActionSchema?: boolean, nullActionSchema?: boolean }} [options]
 * @returns {Promise<string>}
 */
async function fixtureProject(options = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-extension-"));
  const bundle = "console.log('hello');";
  await writeFile(path.join(root, "dist-extension.js"), bundle);
  await writeFile(
    path.join(root, "package.json"),
    JSON.stringify({
      name: options.packageName ?? "@agentdash/local-hello",
      version: "0.1.0",
      scripts: options.scripts,
      dependencies: options.dependencies,
      ...options.nativeFields,
    }),
  );
  await writeFile(
    path.join(root, "agentdash.extension.json"),
    JSON.stringify({
      manifest_version: "2",
      extension_id: "local-hello",
      package: { name: "@agentdash/local-hello", version: "0.1.0" },
      asset_version: "0.1.0",
      runtime_actions: [
        runtimeAction(options),
      ],
      protocol_channels: options.withProtocol
        ? [
            {
              channel_key: "local-hello.api",
              version: "1.0.0",
              description: "Local hello protocol channel",
              methods: [
                {
                  name: "readProfile",
                  description: "Read local profile through the provider channel",
                  input_schema: true,
                  output_schema: true,
                  permissions: ["local.profile.read"],
                },
              ],
            },
          ]
        : options.withInvalidProtocol
          ? [
              {
                channel_key: "api",
                version: "1.0.0",
                description: "Invalid channel",
                methods: [{ name: "bad-name", description: "bad" }],
              },
            ]
          : undefined,
      extension_dependencies: options.withProtocol
        ? [
            {
              alias: "hello",
              extension_id: "local-hello",
              version: "^1.0.0",
              channels: ["local-hello.api"],
            },
          ]
        : options.withInvalidProtocol
          ? [
              {
                alias: "BadAlias",
                extension_id: "local-hello",
                version: "^1.0.0",
                channels: ["api"],
              },
            ]
          : undefined,
      workspace_tabs: [
        {
          type_id: "local-hello.panel",
          label: "Hello",
          uri_scheme: "local-hello",
          renderer: { kind: options.rendererKind ?? "webview", entry: "dist/panel/index.html" },
        },
      ],
      permissions: options.withProtocol
        ? [
            { kind: "local_profile", access: "read" },
            { kind: "http", hosts: ["example.com"], access: "read" },
            { kind: "env", names: ["DEMO_TOKEN"], access: "read" },
            { kind: "process", access: "execute" },
            { kind: "extension_channel", channel_key: "local-hello.api", methods: ["readProfile"] },
          ]
        : [{ kind: "local_profile", access: "read" }],
      bundles: [
        {
          kind: "extension_host",
          entry: "dist-extension.js",
          digest: "sha256:b98785ede1f35602a98818397e292fd8d4dcb66267c427d7d5486196b8b3bcd1",
        },
      ],
    }),
  );
  return root;
}

/**
 * @param {{ withProtocol?: boolean, unknownRuntimePermission?: string, omitActionSchema?: boolean, nullActionSchema?: boolean }} options
 * @returns {Record<string, unknown>}
 */
function runtimeAction(options) {
  const permissions = options.withProtocol
    ? [
        "local.profile.read",
        "http.fetch:example.com",
        "env.read:DEMO_TOKEN",
        "process.exec",
        "process.env.set:DEMO_TOKEN",
        ...(options.unknownRuntimePermission ? [options.unknownRuntimePermission] : []),
      ]
    : ["local.profile.read"];
  const action = {
    action_key: "local-hello.profile",
    kind: "session_runtime",
    description: "Read profile",
    output_schema: {},
    permissions,
  };
  if (options.nullActionSchema) {
    return { ...action, input_schema: null };
  }
  if (options.omitActionSchema) {
    return action;
  }
  return { ...action, input_schema: {} };
}
