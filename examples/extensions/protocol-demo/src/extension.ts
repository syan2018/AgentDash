import { defineExtension, type JsonObject, type JsonValue } from "@agentdash/extension/host";

import { DemoProtocolClient, asRecord } from "./protocol/demo-client";
import {
  PROTOCOL_DEMO_ACTIONS,
  PROTOCOL_DEMO_PROTOCOL_KEY,
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
      kind: "extension_protocol",
      protocol_key: PROTOCOL_DEMO_PROTOCOL_KEY,
      methods: ["greet", "inspectWorkspace", "runShell"],
    });

    const client = new DemoProtocolClient(ctx.api);

    ctx.protocols.register({
      protocol_key: "api",
      version: "1.0.0",
      description: "Protocol Demo API channel",
      methods: {
        greet: {
          description: "Return a greeting from the provider protocol",
          input_schema: true,
          output_schema: true,
          invoke(input: JsonValue) {
            return client.greet(asRecord(input));
          },
        },
        inspectWorkspace: {
          description: "Use workspace VFS through the provider protocol",
          input_schema: true,
          output_schema: true,
          permissions: ["workspace.vfs.write", "workspace.vfs.read", "workspace.vfs.list"],
          invoke(input: JsonValue) {
            return client.inspectWorkspace(asRecord(input));
          },
        },
        runShell: {
          description: "Run a trusted local shell command through the provider protocol",
          input_schema: true,
          output_schema: true,
          permissions: ["process.shell", "env.read:PATH"],
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
      input_schema: true,
      output_schema: true,
      invoke(input) {
        return client.greet(input);
      },
    });

    ctx.runtime.registerAction<JsonObject, JsonObject>({
      action_key: PROTOCOL_DEMO_ACTIONS.fetchDemo,
      kind: "session_runtime",
      description: "Fetch a text response through the built-in HTTP facade",
      input_schema: true,
      output_schema: true,
      permissions: ["http.fetch"],
      invoke(input) {
        return client.fetchText(input);
      },
    });

    ctx.runtime.registerAction<JsonObject, JsonObject>({
      action_key: PROTOCOL_DEMO_ACTIONS.workspaceDemo,
      kind: "session_runtime",
      description: "Write, read, stat and list a workspace file",
      input_schema: true,
      output_schema: true,
      permissions: ["workspace.vfs.write", "workspace.vfs.read", "workspace.vfs.list"],
      invoke(input) {
        return client.inspectWorkspace(input);
      },
    });

    ctx.runtime.registerAction<JsonObject, JsonObject>({
      action_key: PROTOCOL_DEMO_ACTIONS.shellDemo,
      kind: "session_runtime",
      description: "Run a local shell command through the trusted tool facade",
      input_schema: true,
      output_schema: true,
      permissions: ["process.shell", "env.read:PATH"],
      invoke(input) {
        return client.runShell(input);
      },
    });

    ctx.runtime.registerAction<JsonObject, JsonObject>({
      action_key: PROTOCOL_DEMO_ACTIONS.consumeDemoProtocol,
      kind: "session_runtime",
      description: "Consume the extension's own protocol through the self shortcut",
      input_schema: true,
      output_schema: true,
      permissions: [
        "extension.protocol.invoke:protocol-demo.api.greet",
        "extension.protocol.invoke:protocol-demo.api.runShell",
      ],
      async invoke(input) {
        const selfProtocol = ctx.api.protocols.self("api");
        const dependencyProtocol = ctx.api.protocols.from("demo", "api");
        const greeting = await selfProtocol.invoke<JsonObject, JsonObject>("greet", input);
        const shell = await dependencyProtocol.invoke<JsonObject, JsonObject>("runShell", {
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
