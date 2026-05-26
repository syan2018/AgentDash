import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const manifest = JSON.parse(await readFile(new URL("../agentdash.extension.json", import.meta.url), "utf8"));
const packageJson = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
const extensionSource = await readFile(new URL("../src/extension.ts", import.meta.url), "utf8");
const panelSource = await readFile(new URL("../src/panel/App.tsx", import.meta.url), "utf8");

test("manifest declares the Local Hello packaged runtime surface", () => {
  assert.equal(manifest.extension_id, "local-hello");
  assert.equal(manifest.package.name, packageJson.name);
  assert.equal(manifest.runtime_actions[0].action_key, "local-hello.profile");
  assert.equal(manifest.runtime_actions[0].kind, "session_runtime");
  assert.equal(manifest.workspace_tabs[0].type_id, "local-hello.panel");
  assert.equal(manifest.workspace_tabs[0].renderer.entry, "dist/panel/index.html");
  assert.deepEqual(manifest.permissions, [{ kind: "local_profile", access: "read" }]);
});

test("source uses public SDK and bridge contracts only", () => {
  assert.match(extensionSource, /@agentdash\/extension-sdk/);
  assert.match(panelSource, /@agentdash\/extension-ui/);
  assert.match(panelSource, /local-hello-username/);
});

test("package scripts cover authoring commands without npm lifecycle hooks", () => {
  assert.equal(packageJson.scripts.dev, "agentdash-ext dev");
  assert.equal(packageJson.scripts.validate, "agentdash-ext validate");
  assert.equal(packageJson.scripts.pack, "agentdash-ext pack");
  assert.equal(packageJson.scripts["agentdash:install"], "agentdash-ext install");
  assert.equal(packageJson.scripts.install, undefined);
});
