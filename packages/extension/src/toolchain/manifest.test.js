// @ts-check

import test from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
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

test("validateProject accepts protocols, dependencies, and trusted host capabilities", async () => {
  const root = await fixtureProject({ withProtocol: true });
  const result = await validateProject(root);
  assert.deepEqual(result.errors, []);
});

test("validateProject accepts generated operation catalog, backend services, and fetch routes", async () => {
  const root = await fixtureProject({ withProjectionFields: true });
  const result = await validateProject(root);
  assert.deepEqual(result.errors, []);
});

test("validateProject rejects invalid backend service package contract fields", async () => {
  const root = await fixtureProject({
    withProjectionFields: true,
    omitBackendServiceEntryFile: true,
    backendServiceOverride: {
      runtime: "python",
      entry: "../server.js",
      routes: ["api/**"],
      health_path: "health",
    },
  });

  const result = await validateProject(root);
  const errors = result.errors.join("\n");
  assert.match(errors, /backend_services\[\]\.runtime 当前必须是 node/);
  assert.match(errors, /backend_services\[\]\.entry 必须是 package 内相对文件路径/);
  assert.match(errors, /backend_services\[\]\.routes 必须以 \/ 或 http\(s\):\/\/ 开头/);
  assert.match(errors, /backend_services\[\]\.health_path 必须以 \/ 开头/);
});

test("validateProject rejects missing backend service entry files", async () => {
  const root = await fixtureProject({
    withProjectionFields: true,
    omitBackendServiceEntryFile: true,
  });

  const result = await validateProject(root);
  assert.match(result.errors.join("\n"), /backend_services\[\]\.entry 文件不存在: src\/server\/index\.ts/);
});

test("validateProject rejects unknown runtime process permission key", async () => {
  const root = await fixtureProject({ withProtocol: true, unknownRuntimePermission: "process.execute" });
  const result = await validateProject(root);
  assert.match(result.errors.join("\n"), /未知 permission key: process\.execute/);
});

test("validateProject rejects invalid protocol declarations", async () => {
  const root = await fixtureProject({ withInvalidProtocol: true });
  const result = await validateProject(root);
  assert.match(result.errors.join("\n"), /protocols\[\]\.protocol_key/);
  assert.match(result.errors.join("\n"), /protocols\[\]\.methods\[\]\.name/);
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
 * @param {{ packageName?: string, scripts?: Record<string, string>, dependencies?: Record<string, string>, nativeFields?: Record<string, unknown>, rendererKind?: "webview" | "canvas_panel", withProtocol?: boolean, withProjectionFields?: boolean, withInvalidProtocol?: boolean, unknownRuntimePermission?: string, omitActionSchema?: boolean, nullActionSchema?: boolean, backendServiceOverride?: Record<string, unknown>, omitBackendServiceEntryFile?: boolean }} [options]
 * @returns {Promise<string>}
 */
async function fixtureProject(options = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-extension-"));
  const bundle = "console.log('hello');";
  await writeFile(path.join(root, "dist-extension.js"), bundle);
  if (options.withProjectionFields && !options.omitBackendServiceEntryFile) {
    await mkdir(path.join(root, "src", "server"), { recursive: true });
    await writeFile(path.join(root, "src", "server", "index.ts"), "export const ready = true;\n");
  }
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
      protocols: options.withProtocol
        ? [
            {
              protocol_key: "local-hello.api",
              version: "1.0.0",
              description: "Local hello protocol",
              methods: [
                {
                  name: "readProfile",
                  description: "Read local profile through the provider protocol",
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
                protocol_key: "api",
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
              protocols: ["local-hello.api"],
            },
          ]
        : options.withInvalidProtocol
          ? [
              {
                alias: "BadAlias",
                extension_id: "local-hello",
                version: "^1.0.0",
                protocols: ["api"],
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
            { kind: "extension_protocol", protocol_key: "local-hello.api", methods: ["readProfile"] },
          ]
        : options.withProjectionFields
          ? [
              { kind: "local_profile", access: "read" },
              { kind: "backend_service", service_key: "local-hello.api", routes: ["/api/**"] },
            ]
          : [{ kind: "local_profile", access: "read" }],
      fetch_routes: options.withProjectionFields
        ? [
            {
              route: "/api/**",
              scope: "panel_only",
              target: { kind: "backend_service", service_key: "local-hello.api" },
            },
          ]
        : undefined,
      operation_catalog: options.withProjectionFields
        ? [
            {
              operation_key: "local-hello.search",
              visibility: "agent_and_panel",
              description: "Search local hello backend service",
              input_schema: true,
              output_schema: true,
              permission_summary: ["backend_service:local-hello.api"],
              dispatch: {
                kind: "backend_service",
                service_key: "local-hello.api",
                route: "/api/search",
              },
              provenance: {
                capability_key: "api",
                exposure_key: "search",
                generated_from: "capability_exposure",
              },
            },
          ]
        : undefined,
      backend_services: options.withProjectionFields
        ? [
            {
              service_key: "local-hello.api",
              runtime: "node",
              entry: "src/server/index.ts",
              routes: ["/api/**"],
              health_path: "/health",
              ...options.backendServiceOverride,
            },
          ]
        : undefined,
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
