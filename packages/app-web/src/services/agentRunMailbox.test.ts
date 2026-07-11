import { beforeEach, describe, expect, it, vi } from "vitest";

const apiPostMock = vi.hoisted(() => vi.fn());

vi.mock("../api/client", () => ({ api: { post: apiPostMock } }));

import { cancelAgentRun, submitAgentRunComposerInput } from "./agentRunMailbox";
import type { AgentRunCommandPreconditionView } from "../generated/agent-run-mailbox-contracts";

function command(kind: AgentRunCommandPreconditionView["command_kind"]): AgentRunCommandPreconditionView {
  return {
    command_id: kind,
    command_kind: kind,
    stale_guard: {
      snapshot_id: `snapshot-${kind}`,
      run_id: "run/1",
      agent_id: "agent/1",
      active_turn_id: kind === "submit_message" ? undefined : "turn-1",
    },
  };
}

describe("canonical AgentRun Runtime commands", () => {
  beforeEach(() => apiPostMock.mockReset());

  it("submits input through the Runtime-backed composer endpoint", async () => {
    await submitAgentRunComposerInput("run/1", "agent/1", {
      input: [{ type: "text", text: "follow up", text_elements: [] }],
      client_command_id: "command-composer",
      command: command("submit_message"),
      executor_config: { model_id: "gpt-test" },
    });

    expect(apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/composer-submit",
      {
        input: [{ type: "text", text: "follow up", text_elements: [] }],
        client_command_id: "command-composer",
        command: command("submit_message"),
        executor_config: { model_id: "gpt-test" },
      },
    );
  });

  it("interrupts through the canonical Runtime endpoint", async () => {
    await cancelAgentRun("run/1", "agent/1", {
      command: command("cancel"),
      client_command_id: "cancel-agent-run-1",
    });

    expect(apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/cancel",
      {
        command: command("cancel"),
        client_command_id: "cancel-agent-run-1",
      },
    );
  });
});
