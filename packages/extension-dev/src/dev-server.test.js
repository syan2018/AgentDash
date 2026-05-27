// @ts-check

import test from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { startDevProject } from "./dev-server.js";
import { createPreviewHtml } from "./dev-preview.js";

test("createPreviewHtml includes iframe and bridge endpoint", () => {
  const html = createPreviewHtml({
    extensionId: "demo",
    label: "Demo",
    panelPath: "/src/panel/index.html",
    bridgeEndpoint: "/__agentdash_dev/bridge",
  });
  assert.match(html, /Extension panel preview/);
  assert.match(html, /__agentdash_dev\/bridge/);
  assert.match(html, /Bridge Requests/);
});

test("startDevProject serves preview and bridge endpoint", async () => {
  const root = await fixtureProject();
  const dev = await startDevProject(root, { port: 0 });
  try {
    const preview = await fetch(dev.previewUrl);
    assert.equal(preview.status, 200);
    assert.match(await preview.text(), /AgentDash Extension Preview/);

    const panel = await fetch(dev.panelUrl);
    assert.equal(panel.status, 200);
    assert.match(await panel.text(), /Server Demo/);

    const response = await fetch(new URL("/__agentdash_dev/bridge", dev.previewUrl), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        method: "runtime.invoke_action",
        params: { action_key: "server.greet", input: { name: "Preview" } },
      }),
    });
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), {
      result: { message: "Hello Preview" },
    });
  } finally {
    await dev.stop();
  }
});

/**
 * @returns {Promise<string>}
 */
async function fixtureProject() {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-dev-server-"));
  await mkdir(path.join(root, "src", "panel"), { recursive: true });
  await writeFile(path.join(root, "package.json"), JSON.stringify({
    name: "@agentdash/server-demo",
    version: "0.1.0",
    type: "module",
  }));
  await writeFile(path.join(root, "agentdash.extension.json"), JSON.stringify({
    manifest_version: "2",
    extension_id: "server",
    package: { name: "@agentdash/server-demo", version: "0.1.0" },
    asset_version: "0.1.0",
    workspace_tabs: [{
      type_id: "server.panel",
      label: "Server Demo",
      uri_scheme: "server",
      renderer: { kind: "webview", entry: "dist/panel/index.html" },
    }],
    bundles: [{
      kind: "extension_host",
      entry: "dist/extension.js",
      digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    }],
  }));
  await writeFile(path.join(root, "src", "extension.ts"), `import { defineExtension, type JsonObject, type JsonValue } from "@agentdash/extension-sdk";

function readName(input: JsonValue): string {
  return input && typeof input === "object" && !Array.isArray(input) && typeof input.name === "string"
    ? input.name
    : "AgentDash";
}

export default defineExtension({
  manifest: {
    manifest_version: "2",
    extension_id: "server",
    package: { name: "@agentdash/server-demo", version: "0.1.0" },
    asset_version: "0.1.0",
  },
  activate(ctx) {
    ctx.runtime.registerAction({
      action_key: "server.greet",
      kind: "session_runtime",
      description: "Greet",
      invoke(input: JsonValue): JsonObject {
        return { message: "Hello " + readName(input) };
      },
    });
  },
});
`);
  await writeFile(path.join(root, "src", "panel", "index.html"), `<!doctype html>
<html>
  <body>
    <main id="root">Server Demo</main>
  </body>
</html>
`);
  return root;
}
