import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiDeleteMock: vi.fn(),
  apiGetMock: vi.fn(),
  apiPostMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    delete: mocks.apiDeleteMock,
    get: mocks.apiGetMock,
    post: mocks.apiPostMock,
  },
}));

import {
  deleteAgentRunPendingMessage,
  enqueueAgentRunPendingMessage,
  promoteAgentRunPendingMessage,
  sendAgentRunMessage,
  steerAgentRun,
} from "./lifecycle";

describe("lifecycle message service", () => {
  beforeEach(() => {
    mocks.apiDeleteMock.mockReset();
    mocks.apiDeleteMock.mockResolvedValue(undefined);
    mocks.apiGetMock.mockReset();
    mocks.apiGetMock.mockResolvedValue([]);
    mocks.apiPostMock.mockReset();
    mocks.apiPostMock.mockResolvedValue({
      runtime_session_id: "runtime-1",
      turn_id: "turn-1",
      run_ref: { run_id: "run-1" },
      agent_ref: { run_id: "run-1", agent_id: "agent-1" },
      frame_ref: { agent_id: "agent-1", frame_id: "frame-1", revision: 1 },
    });
  });

  it("sends user messages through the AgentRun command endpoint", async () => {
    await sendAgentRunMessage("run/1", "agent/1", {
      input: [{ type: "text", text: "hello", text_elements: [] }],
      client_command_id: "command-1",
      executor_config: {
        executor: "PI_AGENT",
        model_id: "gpt-test",
        thinking_level: "low",
      },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/messages",
      {
        input: [{ type: "text", text: "hello", text_elements: [] }],
        client_command_id: "command-1",
        executor_config: {
          executor: "PI_AGENT",
          model_id: "gpt-test",
          thinking_level: "low",
        },
      },
    );
  });

  it("sends steering input through the AgentRun steering endpoint", async () => {
    await steerAgentRun("run/1", "agent/1", {
      input: [{ type: "text", text: "adjust course", text_elements: [] }],
      client_command_id: "command-2",
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/steering",
      {
        input: [{ type: "text", text: "adjust course", text_elements: [] }],
        client_command_id: "command-2",
      },
    );
  });

  it("enqueues pending messages through the AgentRun pending endpoint", async () => {
    await enqueueAgentRunPendingMessage("run/1", "agent/1", {
      input: [{ type: "text", text: "next", text_elements: [] }],
      client_command_id: "command-3",
      executor_config: { model_id: "gpt-test" },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/pending-messages",
      {
        input: [{ type: "text", text: "next", text_elements: [] }],
        client_command_id: "command-3",
        executor_config: { model_id: "gpt-test" },
      },
    );
  });

  it("deletes pending messages through the AgentRun pending endpoint", async () => {
    await deleteAgentRunPendingMessage("run/1", "agent/1", "message/1");

    expect(mocks.apiDeleteMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/pending-messages/message%2F1",
    );
  });

  it("promotes pending messages through the AgentRun pending endpoint", async () => {
    await promoteAgentRunPendingMessage("run/1", "agent/1", "message/1");

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/pending-messages/message%2F1/promote",
      {},
    );
  });
});
