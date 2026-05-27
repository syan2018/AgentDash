import readline from "node:readline";

import { createExtensionRuntime } from "./context.mjs";
import { createHostApiClient } from "./host-api-client.mjs";
import { loadExtension } from "./loader.mjs";
import { send, toJsonValue } from "./protocol.mjs";

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });

let runtime = null;
const hostApiClient = createHostApiClient({
  send,
  toJsonValue,
  invocationContextParams(extensionKey) {
    return runtime?.invocationContextParams(extensionKey) ?? {
      extension_key: extensionKey,
      action_key: null,
      channel_key: null,
      channel_method: null,
    };
  },
});
runtime = createExtensionRuntime({
  loadExtension,
  requestHostApi: hostApiClient.requestHostApi,
  toJsonValue,
});

rl.on("line", (line) => {
  void (async () => {
    const message = JSON.parse(line);
    if (message.kind === "host_api_response") {
      hostApiClient.handleHostApiResponse(message);
      return;
    }
    if (message.kind !== "request") return;
    try {
      const result = await runtime.handleRequest(message);
      send({ kind: "response", id: message.id, result: toJsonValue(result) });
    } catch (error) {
      send({ kind: "response", id: message.id, error: error instanceof Error ? error.message : String(error) });
    }
  })().catch((error) => {
    send({ kind: "log", level: "error", message: error instanceof Error ? error.message : String(error) });
  });
});
