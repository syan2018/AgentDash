// @ts-check

import test from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import {
  GENERATED_HOST_ENTRY_FILE,
  GENERATED_MANIFEST_FILE,
  GENERATED_PANEL_CLIENT_FILE,
  generateAppProject,
  packAppProject,
  resolveExtensionProjectMode,
  validateAppProject,
} from "./app-pipeline.js";
import { runAgentDashExtCli } from "./cli.js";

test("generateAppProject writes generated manifest, host entry, panel client and permission summary", async () => {
  const root = await fixtureAppProject();

  const generated = await generateAppProject(root);

  assert.equal(generated.normalized.extension_id, "repo-tools");
  assert.equal(generated.manifest.extension_id, "repo-tools");
  assert.deepEqual(generated.manifest.permissions, [
    { kind: "http", hosts: ["api.github.com"], access: "read" },
    { kind: "process", access: "execute", mode: "exec" },
    { kind: "workspace", access: "read_write" },
    {
      kind: "extension_channel",
      channel_key: "repo-tools.protocol",
      methods: ["summarize", "panelOnlyPing"],
    },
    { kind: "backend_service", service_key: "repo-tools.api", routes: ["/api/**"] },
  ]);
  const runtimeActions = recordArray(generated.manifest, "runtime_actions");
  const protocolChannels = recordArray(generated.manifest, "protocol_channels");
  const backendServices = recordArray(generated.manifest, "backend_services");
  const fetchRoutes = recordArray(generated.manifest, "fetch_routes");
  const operationCatalog = recordArray(generated.manifest, "operation_catalog");
  assert.deepEqual(
    runtimeActions.map((action) => stringField(action, "action_key")),
    ["repo-tools.github", "repo-tools.git-status", "repo-tools.files"],
  );
  assert.equal(stringField(protocolChannels[0], "channel_key"), "repo-tools.protocol");
  assert.deepEqual(
    recordArray(protocolChannels[0], "methods").map((method) => stringField(method, "name")),
    ["summarize", "panelOnlyPing"],
  );
  assert.deepEqual(generated.registered_surface.runtime_actions, runtimeActions);
  assert.deepEqual(generated.registered_surface.protocol_channels, protocolChannels);
  assert.equal(stringField(backendServices[0], "service_key"), "repo-tools.api");
  assert.equal(stringField(fetchRoutes[0], "scope"), "panel_only");
  assert.deepEqual(
    operationCatalog.map((operation) => ({
      key: stringField(operation, "operation_key"),
      visibility: stringField(operation, "visibility"),
      dispatch: stringField(recordField(operation, "dispatch"), "kind"),
    })),
    [
      { key: "repo-tools.github", visibility: "agent_and_panel", dispatch: "runtime_action" },
      { key: "repo-tools.files", visibility: "panel_only", dispatch: "runtime_action" },
      { key: "repo-tools.protocol.summarize", visibility: "agent_and_panel", dispatch: "protocol_channel" },
      { key: "repo-tools.api", visibility: "agent_and_panel", dispatch: "backend_service" },
    ],
  );
  assert.equal(
    operationCatalog.some((operation) => stringField(operation, "operation_key") === "repo-tools.git-status"),
    false,
  );
  assert.equal((await stat(path.join(root, GENERATED_MANIFEST_FILE))).isFile(), true);
  assert.equal((await stat(path.join(root, GENERATED_HOST_ENTRY_FILE))).isFile(), true);
  assert.equal((await stat(path.join(root, GENERATED_PANEL_CLIENT_FILE))).isFile(), true);

  const manifestOnDisk = JSON.parse(await readFile(path.join(root, GENERATED_MANIFEST_FILE), "utf8"));
  assert.equal(manifestOnDisk.workspace_tabs[0].renderer.entry, "dist/panel/index.html");
});

test("validateAppProject reuses manifest validation and runtime surface parity without root manifest", async () => {
  const root = await fixtureAppProject();

  const mode = await resolveExtensionProjectMode(root);
  assert.equal(mode.kind, "app");
  await assert.rejects(readFile(path.join(root, "agentdash.extension.json"), "utf8"));

  const result = await validateAppProject(root);

  assert.deepEqual(result.errors, []);
  assert.equal(result.mode, "app");
  assert.equal(result.manifest?.extension_id, "repo-tools");
  assert.equal(recordArray(result.generated?.manifest ?? {}, "operation_catalog").length, 4);
});

test("packAppProject stages generated app artifacts through existing archive validation", async () => {
  const root = await fixtureAppProject();
  const outDir = await mkdtemp(path.join(os.tmpdir(), "agentdash-app-pack-out-"));

  const packed = await packAppProject(root, { outDir });

  assert.equal(packed.mode, "app");
  assert.match(packed.archive_digest, /^sha256:[0-9a-f]{64}$/);
  assert.match(packed.archive_path, /repo-tools-0\.1\.0\.agentdash-extension\.tgz$/);
  assert.equal((await stat(packed.archive_path)).isFile(), true);

  /** @type {Record<string, unknown>} */
  const generatedManifest = JSON.parse(await readFile(path.join(root, GENERATED_MANIFEST_FILE), "utf8"));
  const generatedBundles = recordArray(generatedManifest, "bundles");
  assert.match(stringField(generatedBundles[0], "digest"), /^sha256:[0-9a-f]{64}$/);
  assert.notEqual(
    stringField(generatedBundles[0], "digest"),
    "sha256:0000000000000000000000000000000000000000000000000000000000000000",
  );
  assert.equal(stringField(recordArray(generatedManifest, "backend_services")[0], "service_key"), "repo-tools.api");
  assert.equal(
    recordArray(generatedManifest, "operation_catalog").some((operation) => {
      return stringField(recordField(operation, "dispatch"), "kind") === "backend_service";
    }),
    true,
  );
});

test("runAgentDashExtCli keeps legacy validate command path", async () => {
  const root = await fixtureLegacyProject();
  /** @type {string[]} */
  const logs = [];
  /** @type {string[]} */
  const warnings = [];
  /** @type {string[]} */
  const errors = [];

  const exitCode = await runAgentDashExtCli(["validate", "--cwd", root], {
    log(message) {
      logs.push(String(message ?? ""));
    },
    warn(message) {
      warnings.push(String(message ?? ""));
    },
    error(message) {
      errors.push(String(message ?? ""));
    },
  });

  assert.equal(exitCode, 0);
  assert.deepEqual(errors, []);
  assert.equal(logs.at(-1), "AgentDash extension manifest is valid");
  assert.match(warnings.join("\n"), /bundle dist\/extension\.js 尚未生成/);
});

/**
 * @param {Record<string, unknown>} record
 * @param {string} field
 * @returns {Record<string, unknown>[]}
 */
function recordArray(record, field) {
  const values = record[field];
  if (!Array.isArray(values)) return [];
  return values.map((value) => recordValue(value));
}

/**
 * @param {Record<string, unknown>} record
 * @param {string} field
 * @returns {Record<string, unknown>}
 */
function recordField(record, field) {
  return recordValue(record[field]);
}

/**
 * @param {Record<string, unknown>} record
 * @param {string} field
 * @returns {string}
 */
function stringField(record, field) {
  const value = record[field];
  if (typeof value !== "string") {
    throw new Error(`${field} must be a string`);
  }
  return value;
}

/**
 * @param {unknown} value
 * @returns {Record<string, unknown>}
 */
function recordValue(value) {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("expected object value");
  }
  return /** @type {Record<string, unknown>} */ (value);
}

/**
 * @returns {Promise<string>}
 */
async function fixtureAppProject() {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-app-project-"));
  await mkdir(path.join(root, "src"), { recursive: true });
  await writeFile(path.join(root, "package.json"), JSON.stringify({
    name: "@agentdash/repo-tools",
    version: "0.1.0",
    type: "module",
    dependencies: { react: "^19.0.0" },
  }));
  await writeFile(path.join(root, "agentdash.app.ts"), [
    'import { backendService, customChannel, defineApp, httpProxy, localCommand, workspaceFiles } from "@agentdash/extension";',
    "",
    "const objectSchema = {",
    '  type: "object",',
    "  additionalProperties: false,",
    "};",
    "",
    "export default defineApp({",
    '  id: "repo-tools",',
    '  name: "Repo Tools",',
    '  version: "0.1.0",',
    '  panel: { entry: "src/main.ts" },',
    "  capabilities: {",
    "    github: httpProxy({",
    '      baseUrl: "https://api.github.com",',
    '      access: "read",',
    "      expose: {",
    '        description: "Fetch GitHub repository metadata through the selected backend.",',
    "        input_schema: objectSchema,",
    "        output_schema: true,",
    "      },",
    "    }),",
    "    gitStatus: localCommand({",
    '      command: "git",',
    '      args: ["status", "--short"],',
    "    }),",
    "    files: workspaceFiles({",
    '      access: "read_write",',
    "      expose: {",
    '        description: "Panel-only workspace file helper.",',
    '        visibility: "panel_only",',
    "        input_schema: objectSchema,",
    "      },",
    "    }),",
    "    protocol: customChannel({",
    '      description: "Structured protocol escape hatch.",',
    "      methods: {",
    "        summarize: {",
    '          description: "Summarize a structured payload.",',
    "          input_schema: objectSchema,",
    '          permissions: ["extension.channel.invoke:repo-tools.protocol"],',
    "          expose: {",
    '            description: "Summarize structured payloads for the Agent.",',
    "          },",
    "        },",
    "        panelOnlyPing: {",
    '          description: "Panel-only protocol method.",',
    "          input_schema: true,",
    "          output_schema: true,",
    "        },",
    "      },",
    "    }),",
    "    api: backendService({",
    '      entry: "src/server/index.ts",',
    '      runtime: "node",',
    '      routes: ["/api/**"],',
    '      healthPath: "/health",',
    "      expose: {",
    '        description: "Invoke the extension-owned backend service through the selected backend.",',
    "        input_schema: objectSchema,",
    "      },",
    "    }),",
    "  },",
    "});",
    "",
  ].join("\n"));
  await writeFile(path.join(root, "src", "main.ts"), [
    'const target = document.querySelector("#root") ?? document.body;',
    'target.textContent = "Repo Tools";',
    "",
  ].join("\n"));
  return root;
}

/**
 * @returns {Promise<string>}
 */
async function fixtureLegacyProject() {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-legacy-project-"));
  await writeFile(path.join(root, "package.json"), JSON.stringify({
    name: "@agentdash/legacy-demo",
    version: "0.1.0",
    type: "module",
  }));
  await writeFile(path.join(root, "agentdash.extension.json"), JSON.stringify({
    manifest_version: "2",
    extension_id: "legacy-demo",
    package: { name: "@agentdash/legacy-demo", version: "0.1.0" },
    asset_version: "0.1.0",
    bundles: [
      {
        kind: "extension_host",
        entry: "dist/extension.js",
        digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
      },
    ],
  }));
  return root;
}
