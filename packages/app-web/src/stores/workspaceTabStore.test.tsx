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

  it("恢复和清理 layout 时丢弃当前 runtime 不可打开的动态 tab", () => {
    useWorkspaceTabStore.getState().reset();

    const canvasType: WorkspaceTabLayoutOptions["tabTypes"][number] = {
      typeId: "canvas",
      label: "Canvas",
      allowMultiple: true,
      pinned: false,
      defaultUri: "canvas://",
      canCreateUri: (uri) => uri === "canvas://active",
    };
    const runtimeLayoutOptions: WorkspaceTabLayoutOptions = {
      tabTypes: [canvasType],
      resolveTitle: (_typeId, uri) => uri,
    };

    useWorkspaceTabStore.getState().initialize("session-1", {
      tabs: [
        {
          type_id: "canvas",
          uri: "canvas://active",
          title: "Active",
          pinned: false,
        },
        {
          type_id: "canvas",
          uri: "canvas://inactive",
          title: "Inactive",
          pinned: false,
        },
      ],
      active_tab_uri: "canvas://inactive",
    }, runtimeLayoutOptions);

    expect(useWorkspaceTabStore.getState().tabs.map((tab) => tab.uri)).toEqual([
      "canvas://active",
    ]);

    useWorkspaceTabStore.getState().pruneInvalidTabs({
      ...runtimeLayoutOptions,
      tabTypes: [{
        ...canvasType,
        canCreateUri: () => false,
      }],
    });

    expect(useWorkspaceTabStore.getState().tabs).toEqual([]);
    expect(useWorkspaceTabStore.getState().activeTabId).toBeNull();

    useWorkspaceTabStore.getState().reset();
  });
});
