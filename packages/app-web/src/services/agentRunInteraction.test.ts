import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiPostMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    post: mocks.apiPostMock,
  },
}));

import {
  forkAgentRun,
  submitAgentRunForkInput,
} from "./agentRunInteraction";

describe("lifecycle message service", () => {
  beforeEach(() => {
    mocks.apiPostMock.mockReset();
    mocks.apiPostMock.mockResolvedValue({
      command_receipt: { id: "receipt-1", status: "accepted" },
      outcome: "launched",
    });
  });

  it("forks AgentRun from a stable runtime message ref", async () => {
    await forkAgentRun("run/1", "agent/1", {
      client_command_id: "command-fork",
      fork_point_ref: { turn_id: "turn-1", entry_index: 3 },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/fork",
      {
        client_command_id: "command-fork",
        fork_point_ref: { turn_id: "turn-1", entry_index: 3 },
      },
    );
  });

  it("submits fork input through the AgentRun fork-submit endpoint", async () => {
    await submitAgentRunForkInput("run/1", "agent/1", {
      input: [{ kind: "text", text: "branch follow up" }],
      client_command_id: "command-fork-submit",
      fork_point_ref: { turn_id: "turn-1", entry_index: 3 },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/fork-submit",
      {
        input: [{ kind: "text", text: "branch follow up" }],
        client_command_id: "command-fork-submit",
        fork_point_ref: { turn_id: "turn-1", entry_index: 3 },
      },
    );
  });

});
