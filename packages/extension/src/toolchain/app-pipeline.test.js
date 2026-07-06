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
    { kind: "process", access: "execute" },
    { kind: "workspace", access: "read_write" },
  ]);
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
});

test("packAppProject stages generated app artifacts through existing archive validation", async () => {
  const root = await fixtureAppProject();
  const outDir = await mkdtemp(path.join(os.tmpdir(), "agentdash-app-pack-out-"));

  const packed = await packAppProject(root, { outDir });

  assert.equal(packed.mode, "app");
  assert.match(packed.archive_digest, /^sha256:[0-9a-f]{64}$/);
  assert.match(packed.archive_path, /repo-tools-0\.1\.0\.agentdash-extension\.tgz$/);
  assert.equal((await stat(packed.archive_path)).isFile(), true);

  const generatedManifest = JSON.parse(await readFile(path.join(root, GENERATED_MANIFEST_FILE), "utf8"));
  assert.match(generatedManifest.bundles[0].digest, /^sha256:[0-9a-f]{64}$/);
  assert.notEqual(
    generatedManifest.bundles[0].digest,
    "sha256:0000000000000000000000000000000000000000000000000000000000000000",
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
    "export default {",
    '  id: "repo-tools",',
    '  name: "Repo Tools",',
    '  version: "0.1.0",',
    '  panel: { entry: "src/main.ts" },',
    "  capabilities: {",
    '    github: { kind: "httpProxy", baseUrl: "https://api.github.com", access: "read" },',
    '    gitStatus: { kind: "localCommand", command: "git", args: ["status", "--short"] },',
    '    files: { kind: "workspaceFiles", access: "read_write" },',
    "  },",
    "};",
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
