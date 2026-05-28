import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const manifest = JSON.parse(await readFile(new URL("../agentdash.extension.json", import.meta.url), "utf8"));
const packageJson = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
const extensionSource = await readFile(new URL("../src/extension.ts", import.meta.url), "utf8");
const clientSource = await readFile(new URL("../src/protocol/demo-client.ts", import.meta.url), "utf8");
const panelSource = await readFile(new URL("../src/panel/App.tsx", import.meta.url), "utf8");

test("manifest declares protocol channel provider and consumer dependency surfaces", () => {
  assert.equal(manifest.extension_id, "protocol-demo");
  assert.equal(manifest.package.name, packageJson.name);
  assert.equal(manifest.protocol_channels[0].channel_key, "protocol-demo.api");
  assert.deepEqual(
    manifest.protocol_channels[0].methods.map((method) => method.name),
    ["greet", "inspectWorkspace", "runShell"],
  );
  assert.deepEqual(
    manifest.protocol_channels[0].methods.find((method) => method.name === "runShell").permissions,
    ["process.execute", "env.read:PATH"],
  );
  assert.deepEqual(manifest.extension_dependencies[0], {
    alias: "demo",
    extension_id: "protocol-demo",
    version: "^1.0.0",
    channels: ["protocol-demo.api"],
  });
});

test("manifest covers built-in host capabilities without lifecycle scripts", () => {
  assert.deepEqual(
    manifest.permissions.map((permission) => permission.kind),
    ["http", "workspace", "env", "process", "extension_channel"],
  );
  assert.equal(packageJson.scripts.install, undefined);
  assert.equal(packageJson.scripts.pack, "agentdash-ext pack");
});

test("source demonstrates author-owned protocol adapter and channel authoring sugar", () => {
  assert.match(extensionSource, /ctx\.channels\.register/);
  assert.match(extensionSource, /ctx\.api\.channels\.self/);
  assert.match(extensionSource, /ctx\.api\.channels\.from\("demo", "api"\)/);
  assert.match(clientSource, /api\.workspace\.writeText/);
  assert.match(clientSource, /api\.process\.shell/);
  assert.match(clientSource, /api\.http\.fetch/);
});

test("panel uses extension-ui bridge to exercise the runtime actions", () => {
  assert.match(panelSource, /@agentdash\/extension-ui/);
  assert.match(panelSource, /bridge\.invokeChannel/);
  assert.match(panelSource, /PROTOCOL_DEMO_ACTIONS\.consumeDemoChannel/);
  assert.match(panelSource, /Self Channel/);
  assert.match(panelSource, /Panel Channel/);
});
