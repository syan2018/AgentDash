import { describe, expect, it } from "vitest";

import { tabTypeRegistry, type TabTypeDescriptor } from "../features/workspace-panel/tab-type-registry";
import { useWorkspaceTabStore } from "./workspaceTabStore";

function TestIcon() {
  return null;
}

function pluginDescriptor(): TabTypeDescriptor {
  return {
    typeId: "local-hello.panel",
    label: "Hello",
    icon: TestIcon,
    allowMultiple: true,
    pinned: false,
    renderContent: () => null,
    resolveTitle: () => "Hello",
    parseUri: () => ({ resource: "profile" }),
    buildUri: () => "local-hello://profile",
  };
}

describe("workspaceTabStore extension tab layout", () => {
  it("恢复 plugin tab 的 type_id 与 uri", () => {
    const ownerKey = "test-extension-runtime:workspace-tab-store";
    useWorkspaceTabStore.getState().reset();
    tabTypeRegistry.registerContribution(ownerKey, [pluginDescriptor()]);

    useWorkspaceTabStore.getState().initialize("session-1", {
      tabs: [{
        type_id: "local-hello.panel",
        uri: "local-hello://profile",
        title: "Hello",
        pinned: false,
      }],
      active_tab_uri: "local-hello://profile",
    });

    const layout = useWorkspaceTabStore.getState().exportLayout();
    expect(layout.tabs[0]).toMatchObject({
      type_id: "local-hello.panel",
      uri: "local-hello://profile",
    });
    expect(layout.active_tab_uri).toBe("local-hello://profile");

    tabTypeRegistry.unregisterContribution(ownerKey);
    useWorkspaceTabStore.getState().reset();
  });
});
