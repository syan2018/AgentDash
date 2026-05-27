import { defineExtension, type JsonObject, type JsonValue } from "@agentdash/extension-sdk";

import { DemoProtocolClient, asRecord } from "./protocol/demo-client";
import {
  PROTOCOL_DEMO_ACTIONS,
  PROTOCOL_DEMO_CHANNEL_KEY,
} from "./shared/schema";

export default defineExtension({
  manifest: {
    manifest_version: "2",
    extension_id: "protocol-demo",
    package: {
      name: "@agentdash/example-protocol-demo",
      version: "0.1.0",
    },
    asset_version: "0.1.0",
  },
  activate(ctx) {
    ctx.permissions.require({ kind: "http", hosts: ["*"], access: "read" });
    ctx.permissions.require({ kind: "workspace", access: "read_write" });
    ctx.permissions.require({ kind: "env", names: ["PATH"], access: "read" });
    ctx.permissions.require({ kind: "process", access: "execute" });
    ctx.permissions.require({
      kind: "extension_channel",
      channel_key: PROTOCOL_DEMO_CHANNEL_KEY,
      methods: ["greet", "inspectWorkspace", "runShell"],
    });

    const client = new DemoProtocolClient(ctx.api);

    ctx.channels.register({
      channel_key: "api",
      version: "1.0.0",
      description: "Protocol Demo API channel",
      methods: {
        greet: {
          description: "Return a greeting from the provider channel",
          invoke(input: JsonValue) {
            return client.greet(asRecord(input));
          },
        },
        inspectWorkspace: {
          description: "Use workspace VFS through the provider channel",
          permissions: ["workspace.vfs.write", "workspace.vfs.read", "workspace.vfs.list"],
          invoke(input: JsonValue) {
            return client.inspectWorkspace(asRecord(input));
          },
        },
        runShell: {
          description: "Run a trusted local shell command through the provider channel",
          permissions: ["process.execute", "env.read:PATH"],
          invoke(input: JsonValue) {
            return client.runShell(asRecord(input));
          },
        },
      },
    });

    ctx.runtime.registerAction<JsonObject, JsonObject>({
      action_key: PROTOCOL_DEMO_ACTIONS.greet,
      kind: "session_runtime",
      description: "Return a pure TypeScript greeting",
      invoke(input) {
        return client.greet(input);
      },
    });

    ctx.runtime.registerAction<JsonObject, JsonObject>({
      action_key: PROTOCOL_DEMO_ACTIONS.fetchDemo,
      kind: "session_runtime",
      description: "Fetch a text response through the built-in HTTP facade",
      permissions: ["http.fetch"],
      invoke(input) {
        return client.fetchText(input);
      },
    });

    ctx.runtime.registerAction<JsonObject, JsonObject>({
      action_key: PROTOCOL_DEMO_ACTIONS.workspaceDemo,
      kind: "session_runtime",
      description: "Write, read, stat and list a workspace file",
      permissions: ["workspace.vfs.write", "workspace.vfs.read", "workspace.vfs.list"],
      invoke(input) {
        return client.inspectWorkspace(input);
      },
    });

    ctx.runtime.registerAction<JsonObject, JsonObject>({
      action_key: PROTOCOL_DEMO_ACTIONS.shellDemo,
      kind: "session_runtime",
      description: "Run a local shell command through the trusted tool facade",
      permissions: ["process.execute", "env.read:PATH"],
      invoke(input) {
        return client.runShell(input);
      },
    });

    ctx.runtime.registerAction<JsonObject, JsonObject>({
      action_key: PROTOCOL_DEMO_ACTIONS.consumeDemoChannel,
      kind: "session_runtime",
      description: "Consume the extension's own protocol channel through the self shortcut",
      permissions: [
        "extension.channel.invoke:protocol-demo.api.greet",
        "extension.channel.invoke:protocol-demo.api.runShell",
      ],
      async invoke(input) {
        const selfChannel = ctx.api.channels.self("api");
        const dependencyChannel = ctx.api.channels.from("demo", "api");
        const greeting = await selfChannel.invoke<JsonObject, JsonObject>("greet", input);
        const shell = await dependencyChannel.invoke<JsonObject, JsonObject>("runShell", {
          label: "dependency-alias",
        });
        return {
          greeting,
          shell,
        };
      },
    });
  },
});
