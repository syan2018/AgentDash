import { describe, expect, it, vi } from "vitest";

vi.mock("../services/agentRunWorkspaceLayout", () => ({
  loadWorkspaceTabLayout: vi.fn().mockResolvedValue(null),
  saveWorkspaceTabLayout: vi.fn().mockResolvedValue(undefined),
}));

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
  it("首屏展示命令先于面板 effect 时仍保留并激活目标 Tab", () => {
    useWorkspaceTabStore.getState().reset();

    const runtimeLayoutOptions: WorkspaceTabLayoutOptions = {
      tabTypes: [
        {
          typeId: "inspector",
          label: "审计",
          allowMultiple: false,
          pinned: true,
          defaultUri: "inspector://session",
        },
        {
          typeId: "canvas",
          label: "Canvas",
          allowMultiple: true,
          pinned: false,
          defaultUri: "canvas://",
          canCreateUri: (uri) => uri.startsWith("canvas://") && uri !== "canvas://",
        },
      ],
      resolveTitle: (_typeId, uri) => uri,
    };
    const workspaceKey = "agentrun:run-1:agent-1";

    const tabId = useWorkspaceTabStore.getState().openOrActivateInWorkspace(
      workspaceKey,
      "canvas",
      "canvas://cvs-canvas",
      runtimeLayoutOptions,
    );

    // 模拟 WorkspacePanel 首次被动初始化 effect：它必须读取最新 store，
    // 不能用首帧捕获的 null workspaceKey 重置刚打开的 Canvas。
    const mountedStore = useWorkspaceTabStore.getState();
    if (mountedStore.workspaceKey !== workspaceKey) {
      mountedStore.initialize(workspaceKey, null, runtimeLayoutOptions);
    }

    const state = useWorkspaceTabStore.getState();
    expect(tabId).not.toBe("");
    expect(state.workspaceKey).toBe(workspaceKey);
    expect(state.activeTabId).toBe(tabId);
    expect(state.tabs.find((tab) => tab.id === tabId)).toMatchObject({
      typeId: "canvas",
      uri: "canvas://cvs-canvas",
    });

    useWorkspaceTabStore.getState().reset();
  });

  it("恢复 plugin tab 的 type_id 与 uri", () => {
    useWorkspaceTabStore.getState().reset();

    useWorkspaceTabStore.getState().initialize("agentrun:run-1:agent-1", {
      tabs: [{
        type_id: "local-hello.panel",
        uri: "local-hello://profile",
        title: "Hello",
        pinned: false,
      }],
      active_tab_uri: "local-hello://profile",
    }, layoutOptions);

    const layout = useWorkspaceTabStore.getState().exportLayout();
    expect(useWorkspaceTabStore.getState().workspaceKey).toBe("agentrun:run-1:agent-1");
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

    useWorkspaceTabStore.getState().initialize("agentrun:run-1:agent-1", {
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
