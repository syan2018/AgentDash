import type { ExtensionApi, JsonObject, JsonValue } from "@agentdash/extension-sdk";

import { readText } from "../shared/schema";

export class DemoProtocolClient {
  private readonly api: ExtensionApi;

  constructor(api: ExtensionApi) {
    this.api = api;
  }

  greet(input: JsonObject): JsonObject {
    const name = readText(input.name, "AgentDash");
    return {
      message: `Hello, ${name}`,
      source: "protocol-demo.ts",
    };
  }

  async fetchText(input: JsonObject): Promise<JsonObject> {
    const url = readText(input.url, "https://example.com");
    const response = await this.api.http.fetch(url, {
      method: "GET",
      timeout_ms: 10_000,
    });
    return {
      url,
      status: response.status,
      preview: response.body.slice(0, 160),
    };
  }

  async inspectWorkspace(input: JsonObject): Promise<JsonObject> {
    const fileName = readText(input.file_name, "protocol-demo/hello.txt");
    const content = readText(input.content, "hello from protocol-demo");
    await this.api.workspace.writeText(fileName, content);
    const text = await this.api.workspace.readText(fileName);
    const stat = await this.api.workspace.stat(fileName);
    const parent = fileName.includes("/") ? fileName.slice(0, fileName.lastIndexOf("/")) : ".";
    const entries = await this.api.workspace.list(parent);
    return {
      file_name: fileName,
      text,
      stat_kind: stat.kind,
      entries: entries.map((entry) => entry.path),
    };
  }

  async runShell(input: JsonObject): Promise<JsonObject> {
    const label = readText(input.label, "protocol-demo");
    const pathValue = await this.api.env.get("PATH");
    const result = await this.api.process.shell("node -e \"console.log('protocol-demo-shell')\"", {
      timeout_ms: 10_000,
      max_output_bytes: 4096,
    });
    return {
      label,
      exit_code: result.exit_code,
      stdout: result.stdout.trim(),
      stderr: result.stderr.trim(),
      timed_out: result.timed_out,
      has_path: typeof pathValue === "string" && pathValue.length > 0,
    };
  }
}

export function asRecord(input: JsonValue): JsonObject {
  return input != null && typeof input === "object" && !Array.isArray(input)
    ? input
    : {};
}
