import { describe, expect, it, vi } from "vitest";

import type { ExtensionWorkspaceTabProjectionResponse } from "../../../generated/extension-runtime-contracts";
import type { WorkspaceData } from "../../workspace-runtime";
import { handleExtensionWebviewBridgeRequest } from "./webviewBridge";

describe("Extension webview canonical Operation bridge", () => {
  it("把 panel action 映射为 exact Extension Operation 且不需要 AgentRun context", async () => {
    const invokeOperation = vi.fn(async () => ({
      result: { value: { kind: "inline", value: { ok: true } } },
    }));
    const workspaceData = {
      projectId: "project-1",
      extensionRuntime: {
        status: "ready",
        projection: {
          installations: [{
            installation_id: "11111111-1111-1111-1111-111111111111",
            extension_key: "demo",
            extension_id: "demo",
            display_name: "Demo",
            installed_source: null,
            package_artifact: null,
          }],
          operation_catalog: [{
            extension_key: "demo",
            extension_id: "demo",
            operation_key: "demo.greet",
            description: "Greet",
            visibility: "agent_and_panel",
            input_schema: true,
            output_schema: true,
            permission_summary: [],
            dispatch: { kind: "runtime_action", action_key: "demo.greet" },
            provenance: { capability_key: "greet", exposure_key: "greet", generated_from: "test" },
          }],
        },
      },
    } as unknown as WorkspaceData;
    const tab = {
      extension_key: "demo",
      extension_id: "demo",
      type_id: "demo.panel",
      label: "Demo",
      uri_scheme: "demo",
      renderer: { kind: "webview", entry: "dist/panel/index.html" },
      loadability: { available: true, mode: "extension_host", reason: null },
    } satisfies ExtensionWorkspaceTabProjectionResponse;

    const result = await handleExtensionWebviewBridgeRequest({
      message: {
        channel: "agentdash.extension",
        kind: "request",
        request_id: "request-1",
        method: "runtime.invoke_action",
        params: { action_key: "demo.greet", input: { name: "Ada" } },
      },
      workspaceData,
      tab,
      uri: "demo://panel",
      services: {
        openTab: vi.fn(),
        invokeOperation,
        readFile: vi.fn(),
        writeFile: vi.fn(),
      },
    });

    expect(result).toEqual({ ok: true });
    expect(invokeOperation).toHaveBeenCalledWith("project-1", {
      context: {
        kind: "extension_panel",
        installation_id: "11111111-1111-1111-1111-111111111111",
      },
      operation_ref: {
        namespace: "extension",
        provider_key: "demo",
        operation_key: "demo.greet",
        contract_version: 1,
      },
      input: { name: "Ada" },
      idempotency_key: null,
    });
  });
});
