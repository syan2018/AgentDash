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
      }],
      permissions: [],
      bundles: [],
    };

    const descriptors = createExtensionTabDescriptors({ projection });

    expect(descriptors).toHaveLength(1);
    expect(descriptors[0].typeId).toBe("local-hello.panel");
    expect(descriptors[0].defaultUri).toBe("local-hello://panel");
    expect(descriptors[0].parseUri("local-hello://profile")).toEqual({ resource: "profile" });
  });
});
