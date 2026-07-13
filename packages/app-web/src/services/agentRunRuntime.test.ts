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

import {
  compactAgentRunContext,
  fetchAgentRunRuntimeContext,
  fetchAgentRunRuntimeInspect,
  respondAgentRunInteraction,
} from "./agentRunRuntime";

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
      command: {
        command_id: "snapshot:compact",
        command_kind: "compact_context",
        stale_guard: { snapshot_id: "snapshot", run_id: "run/1", agent_id: "agent/1" },
      },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/context/compact",
      {
        client_command_id: "command-compact",
        command: {
          command_id: "snapshot:compact",
          command_kind: "compact_context",
          stale_guard: { snapshot_id: "snapshot", run_id: "run/1", agent_id: "agent/1" },
        },
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

  it("loads the canonical Runtime context without a legacy projection fallback", async () => {
    mocks.apiGetMock.mockResolvedValue({ thread_id: "thread-1", head: null, checkpoint: null, blocks: [], fidelity: "opaque" });
    await fetchAgentRunRuntimeContext({ runId: "run/1", agentId: "agent/1" });
    expect(mocks.apiGetMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/context",
    );
  });

  it("responds to a typed Runtime interaction", async () => {
    await respondAgentRunInteraction(
      { runId: "run/1", agentId: "agent/1" },
      "interaction/1",
      { kind: "denied", reason: null },
    );
    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/interactions/interaction%2F1/respond",
      { kind: "denied", reason: null },
    );
  });
});
