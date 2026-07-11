import { beforeEach, describe, expect, it, vi } from "vitest";

const apiPostMock = vi.hoisted(() => vi.fn());

vi.mock("../api/client", () => ({ api: { post: apiPostMock } }));

import { cancelAgentRun, submitAgentRunComposerInput } from "./agentRunMailbox";

describe("canonical AgentRun Runtime commands", () => {
  beforeEach(() => apiPostMock.mockReset());

  it("submits input through the Runtime-backed composer endpoint", async () => {
    await submitAgentRunComposerInput("run/1", "agent/1", {
      input: [{ type: "text", text: "follow up", text_elements: [] }],
      client_command_id: "command-composer",
      executor_config: { model_id: "gpt-test" },
    });

    expect(apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/composer-submit",
      {
        input: [{ type: "text", text: "follow up", text_elements: [] }],
        client_command_id: "command-composer",
        executor_config: { model_id: "gpt-test" },
      },
    );
  });

  it("interrupts through the canonical Runtime endpoint", async () => {
    await cancelAgentRun("run/1", "agent/1", {
      client_command_id: "cancel-agent-run-1",
    });

    expect(apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/cancel",
      {
        client_command_id: "cancel-agent-run-1",
      },
    );
  });
});
