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
  resumeAgentRunPendingQueue,
  sendAgentRunMessage,
  steerAgentRun,
} from "./lifecycle";
import type { AgentRunCommandPreconditionView } from "../generated/workflow-contracts";

function command(kind: AgentRunCommandPreconditionView["command_kind"]): AgentRunCommandPreconditionView {
  return {
    command_id: kind,
    command_kind: kind,
    stale_guard: {
      snapshot_id: `snapshot-${kind}`,
      run_id: "run/1",
      agent_id: "agent/1",
      runtime_session_id: "session-1",
      active_turn_id: kind === "send_next" ? undefined : "turn-1",
    },
  };
}

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
      command: command("send_next"),
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
        command: command("send_next"),
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
      command: command("steer"),
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/steering",
      {
        input: [{ type: "text", text: "adjust course", text_elements: [] }],
        client_command_id: "command-2",
        command: command("steer"),
      },
    );
  });

  it("enqueues pending messages through the AgentRun pending endpoint", async () => {
    await enqueueAgentRunPendingMessage("run/1", "agent/1", {
      input: [{ type: "text", text: "next", text_elements: [] }],
      client_command_id: "command-3",
      command: command("enqueue"),
      executor_config: { model_id: "gpt-test" },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/pending-messages",
      {
        input: [{ type: "text", text: "next", text_elements: [] }],
        client_command_id: "command-3",
        command: command("enqueue"),
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
    await promoteAgentRunPendingMessage("run/1", "agent/1", "message/1", {
      command: command("promote_pending"),
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/pending-messages/message%2F1/promote",
      { command: command("promote_pending") },
    );
  });

  it("resumes pending queues through the AgentRun pending resume endpoint", async () => {
    await resumeAgentRunPendingQueue("run/1", "agent/1", {
      command: command("resume_pending_queue"),
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/pending-messages/resume",
      { command: command("resume_pending_queue") },
    );
  });
});
