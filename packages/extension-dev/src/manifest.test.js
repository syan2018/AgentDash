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
 * @param {{ packageName?: string, scripts?: Record<string, string>, dependencies?: Record<string, string>, nativeFields?: Record<string, unknown> }} [options]
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
        {
          action_key: "local-hello.profile",
          kind: "session_runtime",
          description: "Read profile",
          input_schema: {},
          output_schema: {},
        },
      ],
      workspace_tabs: [
        {
          type_id: "local-hello.panel",
          label: "Hello",
          uri_scheme: "local-hello",
          renderer: { kind: "webview", entry: "dist/panel/index.html" },
        },
      ],
      permissions: [{ kind: "local_profile", access: "read" }],
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
