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
  executeAgentRunRuntimeCommand,
  fetchAgentRunRuntimeContextProjection,
  fetchAgentRuntimeView,
  respondAgentRunInteraction,
} from "./agentRunRuntime";
import { agentRuntimeTestFixtures } from "../features/agent-run-runtime/model/agentRuntimeTestFixtures";
import { encodeAgentRuntimeView } from "../generated/agent-runtime-validators";

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

  it("submits context compaction through the typed Product Runtime command", async () => {
    mocks.apiGetMock.mockResolvedValue(
      encodeAgentRuntimeView(agentRuntimeTestFixtures.snapshots.completed),
    );
    mocks.apiPostMock.mockResolvedValue({
      operation_id: "operation-compaction",
      thread_id: "runtime-thread-child",
      status: "accepted",
      evidence: null,
      duplicate: false,
    });

    await executeAgentRunRuntimeCommand(
      { runId: "run/1", agentId: "agent/1" },
      {
        client_command_id: "command-compact",
        command: { kind: "request_compaction" },
      },
    );

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/commands",
      {
        client_command_id: "command-compact",
        command: { kind: "request_compaction" },
      },
    );
  });

  it("loads the canonical Agent Runtime snapshot from the AgentRun target", async () => {
    mocks.apiGetMock.mockResolvedValue(
      encodeAgentRuntimeView(agentRuntimeTestFixtures.snapshots.started),
    );
    await expect(
      fetchAgentRuntimeView({ runId: "run/1", agentId: "agent/1" }),
    ).resolves.toEqual(agentRuntimeTestFixtures.snapshots.started);
    expect(mocks.apiGetMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/view",
    );
  });

  it("loads context projection from the required AgentRun Runtime route", async () => {
    const projection = {
      session_id: "thread-1",
      projection_kind: "agent_runtime_canonical_context",
      projection_version: 7,
      head_event_seq: 7,
      token_estimate: 12,
      message_count: 2,
      segments: [],
      context_usage: {
        categories: [],
        items: [],
        messages: {
          user_message_tokens: 4,
          assistant_message_tokens: 8,
          tool_call_tokens: 0,
          tool_result_tokens: 0,
          attachment_tokens: 0,
        },
        top_tools: [],
        top_attachments: [],
      },
    };
    mocks.apiGetMock.mockResolvedValue(projection);

    await expect(fetchAgentRunRuntimeContextProjection({
      runId: "run/1",
      agentId: "agent/1",
    })).resolves.toEqual(projection);
    expect(mocks.apiGetMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/context/projection",
      { signal: undefined },
    );
  });

  it.each([
    ["JSON number", 9],
    ["leading zero", "09"],
    ["negative", "-1"],
    ["overflow", "18446744073709551616"],
  ])("rejects a non-canonical Runtime u64 encoded as %s", async (_case, revision) => {
    mocks.apiGetMock.mockResolvedValue({
      ...encodeAgentRuntimeView(agentRuntimeTestFixtures.snapshots.started),
      view_revision: revision,
    });

    await expect(
      fetchAgentRuntimeView({ runId: "run/1", agentId: "agent/1" }),
    ).rejects.toThrow("$.view_revision");
  });

  it("rejects a response that is not the canonical Runtime projection", async () => {
    mocks.apiGetMock.mockResolvedValue({
      session_id: "legacy-session",
      events: [],
    });

    await expect(
      fetchAgentRuntimeView({ runId: "run/1", agentId: "agent/1" }),
    ).rejects.toThrow("expected");
  });

  it("responds to a typed Runtime interaction", async () => {
    mocks.apiPostMock.mockResolvedValue({
      operation_id: "operation-1",
      thread_id: "thread-1",
      status: "accepted",
      evidence: null,
      duplicate: false,
    });
    await expect(respondAgentRunInteraction(
      { runId: "run/1", agentId: "agent/1" },
      "interaction/1",
      { kind: "denied", reason: null },
      "command-interaction-1",
    )).resolves.toMatchObject({ status: "accepted" });
    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/runtime/commands",
      {
        client_command_id: "command-interaction-1",
        command: {
          kind: "resolve_interaction",
          interaction_id: "interaction/1",
          response: { kind: "denied", reason: null },
        },
      },
    );
  });
});
