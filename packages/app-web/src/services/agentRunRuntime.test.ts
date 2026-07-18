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
  fetchManagedRuntimeChangePage,
  fetchManagedRuntimeSnapshot,
  respondAgentRunInteraction,
} from "./agentRunRuntime";
import { managedRuntimeTestFixtures } from "../features/agent-run-runtime/model/managedRuntimeTestFixtures";

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

  it("loads the canonical Managed Runtime snapshot from the AgentRun target", async () => {
    mocks.apiGetMock.mockResolvedValue(
      managedRuntimeTestFixtures.snapshots.started,
    );
    await expect(
      fetchManagedRuntimeSnapshot({ runId: "run/1", agentId: "agent/1" }),
    ).resolves.toBe(managedRuntimeTestFixtures.snapshots.started);
    expect(mocks.apiGetMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/snapshot",
    );
  });

  it("loads canonical committed changes after the durable cursor", async () => {
    mocks.apiGetMock.mockResolvedValue(managedRuntimeTestFixtures.changePage);
    await expect(
      fetchManagedRuntimeChangePage(
        { runId: "run/1", agentId: "agent/1" },
        8,
      ),
    ).resolves.toBe(managedRuntimeTestFixtures.changePage);
    expect(mocks.apiGetMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/changes?limit=256&after=8",
    );
  });

  it("rejects a response that is not the canonical Runtime projection", async () => {
    mocks.apiGetMock.mockResolvedValue({
      session_id: "legacy-session",
      events: [],
    });

    await expect(
      fetchManagedRuntimeSnapshot({ runId: "run/1", agentId: "agent/1" }),
    ).rejects.toThrow("canonical contract");
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
