import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  settingsListMock: vi.fn(),
  settingsUpdateMock: vi.fn(),
}));

vi.mock("../api/settings", () => ({
  settingsApi: {
    list: mocks.settingsListMock,
    update: mocks.settingsUpdateMock,
  },
}));

import {
  loadWorkspaceTabLayout,
  saveWorkspaceTabLayout,
} from "./agentRunWorkspaceLayout";

describe("agentRunWorkspaceLayout service", () => {
  beforeEach(() => {
    mocks.settingsListMock.mockReset();
    mocks.settingsUpdateMock.mockReset();
  });

  it("persists workspace tab layout with AgentRun workspace setting key", async () => {
    mocks.settingsUpdateMock.mockResolvedValue(undefined);

    await saveWorkspaceTabLayout("agentrun:run-1:agent-1", {
      tabs: [{
        type_id: "canvas",
        uri: "canvas://cvs-1",
        title: "Canvas",
        pinned: false,
      }],
      active_tab_uri: "canvas://cvs-1",
    });

    expect(mocks.settingsUpdateMock).toHaveBeenCalledWith(
      { scope: "user" },
      [{
        key: "ui.agentrun_workspace_tab_layout.agentrun:run-1:agent-1",
        value: {
          tabs: [{
            type_id: "canvas",
            uri: "canvas://cvs-1",
            title: "Canvas",
            pinned: false,
          }],
          active_tab_uri: "canvas://cvs-1",
        },
      }],
    );
  });

  it("loads workspace tab layout from AgentRun workspace setting key", async () => {
    mocks.settingsListMock.mockResolvedValue([{
      key: "ui.agentrun_workspace_tab_layout.agentrun:run-1:agent-1",
      value: {
        tabs: [{
          type_id: "context",
          uri: "context://overview",
          title: "上下文",
          pinned: true,
        }],
        active_tab_uri: "context://overview",
      },
    }]);

    await expect(loadWorkspaceTabLayout("agentrun:run-1:agent-1")).resolves.toEqual({
      tabs: [{
        type_id: "context",
        uri: "context://overview",
        title: "上下文",
        pinned: true,
      }],
      active_tab_uri: "context://overview",
    });
    expect(mocks.settingsListMock).toHaveBeenCalledWith({
      scope: "user",
      category: "ui.agentrun_workspace_tab_layout.agentrun:run-1:agent-1",
    });
  });
});
