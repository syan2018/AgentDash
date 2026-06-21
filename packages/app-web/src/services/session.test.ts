import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiGetMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    get: mocks.apiGetMock,
  },
}));

vi.mock("../api/settings", () => ({
  settingsApi: {},
}));

import { fetchSessionExecutionState } from "./session";

describe("session service", () => {
  beforeEach(() => {
    mocks.apiGetMock.mockReset();
  });

  it("normalizes cancelling execution state", async () => {
    mocks.apiGetMock.mockResolvedValue({
      session_id: "session-1",
      status: "cancelling",
      turn_id: "turn-1",
      message: "取消中",
    });

    await expect(fetchSessionExecutionState("session-1")).resolves.toEqual({
      session_id: "session-1",
      status: "cancelling",
      turn_id: "turn-1",
      message: "取消中",
    });
    expect(mocks.apiGetMock).toHaveBeenCalledWith("/sessions/session-1/state");
  });

  it("rejects unknown route-local execution status", async () => {
    mocks.apiGetMock.mockResolvedValue({
      session_id: "session-1",
      status: "ready",
      turn_id: null,
      message: null,
    });

    await expect(fetchSessionExecutionState("session-1")).rejects.toThrow(
      "未知的会话执行状态: ready",
    );
  });
});
