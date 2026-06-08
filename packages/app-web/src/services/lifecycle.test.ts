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
  deletePendingMessage,
  enqueuePendingMessage,
  listPendingMessages,
  promotePendingMessage,
  sendAgentRunMessageByRuntimeSession,
  steerAgentRunByRuntimeSession,
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

  it("sends user messages through the session command endpoint", async () => {
    await sendAgentRunMessageByRuntimeSession("runtime/1", {
      input: [{ type: "text", text: "hello", text_elements: [] }],
      executor_config: {
        executor: "PI_AGENT",
        model_id: "gpt-test",
        thinking_level: "low",
      },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/sessions/runtime%2F1/messages",
      {
        input: [{ type: "text", text: "hello", text_elements: [] }],
        executor_config: {
          executor: "PI_AGENT",
          model_id: "gpt-test",
          thinking_level: "low",
        },
      },
    );
  });

  it("sends steering input through the session steering endpoint", async () => {
    await steerAgentRunByRuntimeSession("runtime/1", {
      input: [{ type: "text", text: "adjust course", text_elements: [] }],
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/sessions/runtime%2F1/steering",
      {
        input: [{ type: "text", text: "adjust course", text_elements: [] }],
      },
    );
  });

  it("lists pending messages through the session pending endpoint", async () => {
    await listPendingMessages("runtime/1");

    expect(mocks.apiGetMock).toHaveBeenCalledWith(
      "/sessions/runtime%2F1/pending-messages",
    );
  });

  it("enqueues pending messages through the session pending endpoint", async () => {
    await enqueuePendingMessage("runtime/1", {
      input: [{ type: "text", text: "next", text_elements: [] }],
      executor_config: { model_id: "gpt-test" },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/sessions/runtime%2F1/pending-messages",
      {
        input: [{ type: "text", text: "next", text_elements: [] }],
        executor_config: { model_id: "gpt-test" },
      },
    );
  });

  it("deletes pending messages through the session pending endpoint", async () => {
    await deletePendingMessage("runtime/1", "message/1");

    expect(mocks.apiDeleteMock).toHaveBeenCalledWith(
      "/sessions/runtime%2F1/pending-messages/message%2F1",
    );
  });

  it("promotes pending messages through the session pending endpoint", async () => {
    await promotePendingMessage("runtime/1", "message/1");

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/sessions/runtime%2F1/pending-messages/message%2F1/promote",
      {},
    );
  });
});
