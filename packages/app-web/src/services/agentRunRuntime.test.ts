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
import type { AgentRunCommandPreconditionView } from "../generated/agent-run-mailbox-contracts";

function command(kind: AgentRunCommandPreconditionView["command_kind"]): AgentRunCommandPreconditionView {
  return {
    command_id: kind,
    command_kind: kind,
    stale_guard: {
      snapshot_id: `snapshot-${kind}`,
      run_id: "run/1",
      agent_id: "agent/1",
      active_turn_id: "turn-1",
    },
  };
}

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
      command: command("compact_context"),
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/context/compact",
      {
        client_command_id: "command-compact",
        command: command("compact_context"),
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
