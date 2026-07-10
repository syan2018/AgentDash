// @ts-check

import test from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { createDevRuntime } from "./dev-runtime.js";

test("ExtensionDevRuntime dispatches metadata, actions and protocols", async () => {
  const root = await fixtureProject();
  const runtime = createDevRuntime(root);
  await runtime.load();

  const context = await runtime.dispatch({ method: "metadata.get_context" });
  assert.equal(asRecord(context).extension_id, "demo");

  const action = await runtime.dispatch({
    method: "runtime.invoke_action",
    params: { action_key: "demo.greet", input: { name: "Codex" } },
  });
  assert.deepEqual(action, { message: "Hello Codex", source: "action" });

  const channel = await runtime.dispatch({
    method: "extension.invoke_protocol",
    params: { protocol_key: "api", method: "greet", input: { name: "Bridge" } },
  });
  assert.deepEqual(channel, { message: "Hello Bridge", source: "channel" });

  const selfAction = await runtime.dispatch({
    method: "runtime.invoke_action",
    params: { action_key: "demo.self", input: { name: "Self" } },
  });
  assert.deepEqual(selfAction, { message: "Hello Self", source: "channel" });

  const aliasAction = await runtime.dispatch({
    method: "runtime.invoke_action",
    params: { action_key: "demo.alias", input: { name: "Alias" } },
  });
  assert.deepEqual(aliasAction, { message: "Hello Alias", source: "channel" });
});

test("ExtensionDevRuntime reloads extension source before dispatch", async () => {
  const root = await fixtureProject({ greeting: "Hello" });
  const runtime = createDevRuntime(root);
  await runtime.load();

  const first = await runtime.dispatch({
    method: "runtime.invoke_action",
    params: { action_key: "demo.greet", input: { name: "Reload" } },
  });
  assert.deepEqual(first, { message: "Hello Reload", source: "action" });

  await writeExtension(root, "Hi");
  const second = await runtime.dispatch({
    method: "runtime.invoke_action",
    params: { action_key: "demo.greet", input: { name: "Reload" } },
  });
  assert.deepEqual(second, { message: "Hi Reload", source: "action" });
});

test("ExtensionDevRuntime rejects TS runtime surface not declared by manifest", async () => {
  const root = await fixtureProject({ omitManifestAction: "demo.alias" });
  const runtime = createDevRuntime(root);

  await assert.rejects(
    runtime.load(),
    /TS 注册了 manifest 未声明的 runtime action: demo\.alias/,
  );
});

/**
 * @param {{ greeting?: string, omitManifestAction?: string }} [options]
 * @returns {Promise<string>}
 */
async function fixtureProject(options = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "agentdash-dev-runtime-"));
  await mkdir(path.join(root, "src"), { recursive: true });
  await writeFile(path.join(root, "package.json"), JSON.stringify({
    name: "@agentdash/demo",
    version: "0.1.0",
    type: "module",
  }));
  const runtimeActions = [{
    action_key: "demo.greet",
    kind: "session_runtime",
    description: "Greet through action",
    input_schema: true,
    output_schema: true,
  }, {
    action_key: "demo.self",
    kind: "session_runtime",
    description: "Greet through self channel",
    input_schema: true,
    output_schema: true,
  }, {
    action_key: "demo.alias",
    kind: "session_runtime",
    description: "Greet through dependency alias",
    input_schema: true,
    output_schema: true,
  }].filter((action) => action.action_key !== options.omitManifestAction);
  await writeFile(path.join(root, "agentdash.extension.json"), JSON.stringify({
    manifest_version: "2",
    extension_id: "demo",
    package: { name: "@agentdash/demo", version: "0.1.0" },
    asset_version: "0.1.0",
    workspace_tabs: [{
      type_id: "demo.panel",
      label: "Demo",
      uri_scheme: "demo",
      renderer: { kind: "webview", entry: "dist/panel/index.html" },
    }],
    extension_dependencies: [{
      alias: "demo",
      extension_id: "demo",
      version: "^1.0.0",
      protocols: ["demo.api"],
    }],
    runtime_actions: runtimeActions,
    protocols: [{
      protocol_key: "demo.api",
      version: "1.0.0",
      description: "Demo channel",
      methods: [{
        name: "greet",
        description: "Greet through channel",
        input_schema: true,
        output_schema: true,
      }],
    }],
    bundles: [{
      kind: "extension_host",
      entry: "dist/extension.js",
      digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    }],
  }));
  await writeExtension(root, options.greeting ?? "Hello");
  return root;
}

/**
 * @param {string} root
 * @param {string} greeting
 * @returns {Promise<void>}
 */
async function writeExtension(root, greeting) {
  await writeFile(path.join(root, "src", "extension.ts"), `import { defineExtension, type JsonObject, type JsonValue } from "@agentdash/extension/host";

function nameFrom(input: JsonValue): string {
  return input && typeof input === "object" && !Array.isArray(input) && typeof input.name === "string"
    ? input.name
    : "AgentDash";
}

export default defineExtension({
  manifest: {
    manifest_version: "2",
    extension_id: "demo",
    package: { name: "@agentdash/demo", version: "0.1.0" },
    asset_version: "0.1.0",
  },
  activate(ctx) {
    ctx.protocols.register({
      protocol_key: "api",
      version: "1.0.0",
      description: "Demo channel",
      methods: {
        greet: {
          description: "Greet through channel",
          input_schema: true,
          output_schema: true,
          invoke(input: JsonValue): JsonObject {
            return { message: ${JSON.stringify(`${greeting} `)} + nameFrom(input), source: "channel" };
          },
        },
      },
    });
    ctx.runtime.registerAction({
      action_key: "demo.greet",
      kind: "session_runtime",
      description: "Greet through action",
      input_schema: true,
      output_schema: true,
      invoke(input: JsonValue): JsonObject {
        return { message: ${JSON.stringify(`${greeting} `)} + nameFrom(input), source: "action" };
      },
    });
    ctx.runtime.registerAction({
      action_key: "demo.self",
      kind: "session_runtime",
      description: "Greet through self channel",
      input_schema: true,
      output_schema: true,
      async invoke(input: JsonValue): Promise<JsonObject> {
        return await ctx.api.protocols.self("api").invoke<JsonValue, JsonObject>("greet", input);
      },
    });
    ctx.runtime.registerAction({
      action_key: "demo.alias",
      kind: "session_runtime",
      description: "Greet through dependency alias",
      input_schema: true,
      output_schema: true,
      async invoke(input: JsonValue): Promise<JsonObject> {
        return await ctx.api.protocols.from("demo", "api").invoke<JsonValue, JsonObject>("greet", input);
      },
    });
  },
});
`);
}

/**
 * @param {unknown} value
 * @returns {Record<string, unknown>}
 */
function asRecord(value) {
  assert.equal(typeof value, "object");
  assert.notEqual(value, null);
  assert.equal(Array.isArray(value), false);
  return /** @type {Record<string, unknown>} */ (value);
}
