import { describe, expect, it } from "vitest";

import { useWorkspaceTabStore, type WorkspaceTabLayoutOptions } from "./workspaceTabStore";

const layoutOptions: WorkspaceTabLayoutOptions = {
  tabTypes: [{
    typeId: "local-hello.panel",
    label: "Hello",
    allowMultiple: true,
    pinned: false,
    defaultUri: "local-hello://profile",
  }],
  resolveTitle: () => "Hello",
};

describe("workspaceTabStore extension tab layout", () => {
  it("恢复 plugin tab 的 type_id 与 uri", () => {
    useWorkspaceTabStore.getState().reset();

    useWorkspaceTabStore.getState().initialize("session-1", {
      tabs: [{
        type_id: "local-hello.panel",
        uri: "local-hello://profile",
        title: "Hello",
        pinned: false,
      }],
      active_tab_uri: "local-hello://profile",
    }, layoutOptions);

    const layout = useWorkspaceTabStore.getState().exportLayout();
    expect(layout.tabs[0]).toMatchObject({
      type_id: "local-hello.panel",
      uri: "local-hello://profile",
    });
    expect(layout.active_tab_uri).toBe("local-hello://profile");

    useWorkspaceTabStore.getState().reset();
  });

  it("根据显式 layout descriptor 生成默认 pinned tab", () => {
    useWorkspaceTabStore.getState().reset();

    useWorkspaceTabStore.getState().initialize(null, null, {
      tabTypes: [{
        typeId: "context",
        label: "上下文",
        allowMultiple: false,
        pinned: true,
        defaultUri: "context://overview",
      }],
      resolveTitle: (_typeId, uri) => uri,
    });

    expect(useWorkspaceTabStore.getState().tabs).toMatchObject([{
      typeId: "context",
      uri: "context://overview",
      title: "上下文",
      pinned: true,
    }]);

    useWorkspaceTabStore.getState().reset();
  });
});
