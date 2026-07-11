import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiGetMock: vi.fn(),
  apiPostMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    get: mocks.apiGetMock,
    post: mocks.apiPostMock,
  },
}));

import { compactAgentRunContext, fetchAgentRunRuntimeInspect } from "./agentRunRuntime";

describe("AgentRun runtime service", () => {
  beforeEach(() => {
    mocks.apiGetMock.mockReset();
    mocks.apiPostMock.mockReset();
    mocks.apiPostMock.mockResolvedValue({
      command_receipt: {
        client_command_id: "command-compact",
        status: "accepted",
        duplicate: false,
      },
      outcome: "scheduled_next_turn",
    });
  });

  it("submits context compaction as command-only intent", async () => {
    await compactAgentRunContext("run/1", "agent/1", {
      client_command_id: "command-compact",
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/context/compact",
      {
        client_command_id: "command-compact",
      },
    );
  });

  it("loads the canonical Runtime inspect projection from the AgentRun target", async () => {
    mocks.apiGetMock.mockResolvedValue({ target: {}, binding: null, snapshot: null });
    await fetchAgentRunRuntimeInspect({ runId: "run/1", agentId: "agent/1" });
    expect(mocks.apiGetMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime",
    );
  });
});
