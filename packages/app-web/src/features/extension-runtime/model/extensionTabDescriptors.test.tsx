import { describe, expect, it } from "vitest";

import type { ExtensionRuntimeProjectionResponse } from "../../../types";
import { createExtensionTabDescriptors } from "./extensionTabDescriptors";

describe("createExtensionTabDescriptors", () => {
  it("从 Project extension runtime projection 生成 workspace tab descriptor", () => {
    const projection: ExtensionRuntimeProjectionResponse = {
      installations: [],
      commands: [],
      flags: [],
      message_renderers: [],
      runtime_actions: [],
      workspace_tabs: [{
        extension_key: "local-hello",
        extension_id: "local-hello",
        type_id: "local-hello.panel",
        label: "Hello",
        uri_scheme: "local-hello",
        renderer: { kind: "webview", entry: "dist/panel/index.html" },
      }, {
        extension_key: "canvas-demo",
        extension_id: "canvas-demo",
        type_id: "canvas-demo.panel",
        label: "Canvas Demo",
        uri_scheme: "canvas-demo",
        renderer: { kind: "canvas_panel", entry: "dist/canvas/runtime-snapshot.json" },
      }],
      permissions: [],
      bundles: [],
    };

    const descriptors = createExtensionTabDescriptors({ projection });

    expect(descriptors).toHaveLength(2);
    expect(descriptors[0].typeId).toBe("local-hello.panel");
    expect(descriptors[0].defaultUri).toBe("local-hello://panel");
    expect(descriptors[0].parseUri("local-hello://profile")).toEqual({ resource: "profile" });
    expect(descriptors[1].typeId).toBe("canvas-demo.panel");
    expect(descriptors[1].defaultUri).toBe("canvas-demo://panel");
    expect(descriptors[1].resolveTitle("canvas-demo://snapshot")).toBe("Canvas Demo: snapshot");
  });
});
