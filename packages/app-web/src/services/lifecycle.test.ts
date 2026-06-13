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
  deleteAgentRunMailboxMessage,
  promoteAgentRunMailboxMessage,
  resumeAgentRunMailbox,
  submitAgentRunComposerInput,
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
      active_turn_id: kind === "submit_message" ? undefined : "turn-1",
    },
  };
}

describe("lifecycle message service", () => {
  beforeEach(() => {
    mocks.apiDeleteMock.mockReset();
    mocks.apiDeleteMock.mockResolvedValue({
      command_receipt: { id: "receipt-1", status: "accepted" },
      outcome: "deleted",
    });
    mocks.apiGetMock.mockReset();
    mocks.apiGetMock.mockResolvedValue([]);
    mocks.apiPostMock.mockReset();
    mocks.apiPostMock.mockResolvedValue({
      command_receipt: { id: "receipt-1", status: "accepted" },
      outcome: "queued",
    });
  });

  it("submits composer input through the AgentRun composer endpoint", async () => {
    await submitAgentRunComposerInput("run/1", "agent/1", {
      input: [{ type: "text", text: "follow up", text_elements: [] }],
      client_command_id: "command-composer",
      command: command("submit_message"),
      executor_config: { model_id: "gpt-test" },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/composer-submit",
      {
        input: [{ type: "text", text: "follow up", text_elements: [] }],
        client_command_id: "command-composer",
        command: command("submit_message"),
        executor_config: { model_id: "gpt-test" },
      },
    );
  });

  it("deletes mailbox messages through the AgentRun mailbox endpoint", async () => {
    await deleteAgentRunMailboxMessage("run/1", "agent/1", "message/1", {
      command: command("delete_mailbox_message"),
    });

    expect(mocks.apiDeleteMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/mailbox/messages/message%2F1",
      { command: command("delete_mailbox_message") },
    );
  });

  it("promotes mailbox messages through the AgentRun mailbox endpoint", async () => {
    await promoteAgentRunMailboxMessage("run/1", "agent/1", "message/1", {
      command: command("promote_mailbox_message"),
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/mailbox/messages/message%2F1/promote",
      { command: command("promote_mailbox_message") },
    );
  });

  it("resumes mailbox through the AgentRun mailbox resume endpoint", async () => {
    await resumeAgentRunMailbox("run/1", "agent/1", {
      command: command("resume_mailbox"),
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/mailbox/resume",
      { command: command("resume_mailbox") },
    );
  });
});
