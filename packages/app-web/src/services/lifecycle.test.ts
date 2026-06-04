import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiPostMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    post: mocks.apiPostMock,
  },
}));

import { sendLifecycleAgentMessageByRuntimeSession } from "./lifecycle";

describe("lifecycle message service", () => {
  beforeEach(() => {
    mocks.apiPostMock.mockReset();
    mocks.apiPostMock.mockResolvedValue({
      runtime_session_id: "runtime-1",
      turn_id: "turn-1",
      run_ref: { run_id: "run-1" },
      agent_ref: { run_id: "run-1", agent_id: "agent-1" },
      frame_ref: { agent_id: "agent-1", frame_id: "frame-1", revision: 1 },
    });
  });

  it("sends user messages through the LifecycleAgent command endpoint", async () => {
    await sendLifecycleAgentMessageByRuntimeSession("runtime/1", {
      input: [{ type: "text", text: "hello", text_elements: [] }],
      executor_config: {
        executor: "PI_AGENT",
        model_id: "gpt-test",
        thinking_level: "low",
      },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/lifecycle-agents/by-runtime-session/runtime%2F1/messages",
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
});
