import type { JsonObject, JsonValue } from "@agentdash/extension-sdk";

export const PROTOCOL_DEMO_ACTIONS = {
  greet: "protocol-demo.greet",
  fetchDemo: "protocol-demo.fetch_demo",
  workspaceDemo: "protocol-demo.workspace_demo",
  shellDemo: "protocol-demo.shell_demo",
  consumeDemoChannel: "protocol-demo.consume_demo_channel",
} as const;

export const PROTOCOL_DEMO_CHANNEL_KEY = "protocol-demo.api";

export function readText(value: JsonValue | undefined, fallback: string): string {
  return typeof value === "string" && value.trim() !== "" ? value : fallback;
}

export function asJsonObject(value: JsonValue | undefined): JsonObject {
  return value != null && typeof value === "object" && !Array.isArray(value)
    ? value
    : {};
}

export function displayJson(value: JsonValue): string {
  return JSON.stringify(value, null, 2);
}
