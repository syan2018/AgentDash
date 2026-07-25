import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiPostMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    post: mocks.apiPostMock,
  },
}));

import {
  cancelAgentRun,
  forkAgentRun,
  submitAgentRunForkInput,
  submitAgentRunComposerInput,
} from "./agentRunInteraction";
import type { AgentRunCommandPreconditionView } from "../generated/agent-run-interaction-contracts";

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

describe("lifecycle message service", () => {
  beforeEach(() => {
    mocks.apiPostMock.mockReset();
    mocks.apiPostMock.mockResolvedValue({
      command_receipt: { id: "receipt-1", status: "accepted" },
      outcome: "launched",
    });
  });

  it("submits composer input through the AgentRun composer endpoint", async () => {
    await submitAgentRunComposerInput("run/1", "agent/1", {
      input: [{ kind: "text", text: "follow up" }],
      client_command_id: "command-composer",
      command: command("submit_message"),
      executor_config: { model_id: "gpt-test" },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/composer-submit",
      {
        input: [{ kind: "text", text: "follow up" }],
        client_command_id: "command-composer",
        command: command("submit_message"),
        executor_config: { model_id: "gpt-test" },
      },
    );
  });

  it("forks AgentRun from a stable runtime message ref", async () => {
    await forkAgentRun("run/1", "agent/1", {
      client_command_id: "command-fork",
      fork_point_ref: { turn_id: "turn-1", entry_index: 3 },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/fork",
      {
        client_command_id: "command-fork",
        fork_point_ref: { turn_id: "turn-1", entry_index: 3 },
      },
    );
  });

  it("submits fork input through the AgentRun fork-submit endpoint", async () => {
    await submitAgentRunForkInput("run/1", "agent/1", {
      input: [{ kind: "text", text: "branch follow up" }],
      client_command_id: "command-fork-submit",
      fork_point_ref: { turn_id: "turn-1", entry_index: 3 },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/fork-submit",
      {
        input: [{ kind: "text", text: "branch follow up" }],
        client_command_id: "command-fork-submit",
        fork_point_ref: { turn_id: "turn-1", entry_index: 3 },
      },
    );
  });

  it("cancels AgentRun with request-level client command id", async () => {
    await cancelAgentRun("run/1", "agent/1", {
      command: command("cancel"),
      client_command_id: "cancel-agent-run-1",
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/cancel",
      {
        command: command("cancel"),
        client_command_id: "cancel-agent-run-1",
      },
    );
  });
});
