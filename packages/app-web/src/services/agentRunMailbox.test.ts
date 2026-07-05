import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiDeleteMock: vi.fn(),
  apiGetMock: vi.fn(),
  apiPostMock: vi.fn(),
  apiPutMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    delete: mocks.apiDeleteMock,
    get: mocks.apiGetMock,
    post: mocks.apiPostMock,
    put: mocks.apiPutMock,
  },
}));

import {
  cancelAgentRun,
  deleteAgentRunMailboxMessage,
  forkAgentRun,
  moveAgentRunMailboxMessage,
  promoteAgentRunMailboxMessage,
  resumeAgentRunMailbox,
  submitAgentRunForkInput,
  submitAgentRunComposerInput,
} from "./agentRunMailbox";
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
    mocks.apiPutMock.mockReset();
    mocks.apiPutMock.mockResolvedValue({ ok: true, order_key: 12 });
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
      input: [{ type: "text", text: "branch follow up", text_elements: [] }],
      client_command_id: "command-fork-submit",
      fork_point_ref: { turn_id: "turn-1", entry_index: 3 },
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/fork-submit",
      {
        input: [{ type: "text", text: "branch follow up", text_elements: [] }],
        client_command_id: "command-fork-submit",
        fork_point_ref: { turn_id: "turn-1", entry_index: 3 },
      },
    );
  });

  it("deletes mailbox messages through the AgentRun mailbox endpoint", async () => {
    await deleteAgentRunMailboxMessage("run/1", "agent/1", "message/1", {
      command: command("delete_mailbox_message"),
      client_command_id: "delete-message-1",
    });

    expect(mocks.apiDeleteMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/mailbox/messages/message%2F1",
      {
        command: command("delete_mailbox_message"),
        client_command_id: "delete-message-1",
      },
    );
  });

  it("promotes mailbox messages through the AgentRun mailbox endpoint", async () => {
    await promoteAgentRunMailboxMessage("run/1", "agent/1", "message/1", {
      command: command("promote_mailbox_message"),
      client_command_id: "promote-message-1",
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/mailbox/messages/message%2F1/promote",
      {
        command: command("promote_mailbox_message"),
        client_command_id: "promote-message-1",
      },
    );
  });

  it("resumes mailbox through the AgentRun mailbox resume endpoint", async () => {
    await resumeAgentRunMailbox("run/1", "agent/1", {
      command: command("resume_mailbox"),
      client_command_id: "resume-mailbox-1",
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/mailbox/resume",
      {
        command: command("resume_mailbox"),
        client_command_id: "resume-mailbox-1",
      },
    );
  });

  it("moves mailbox messages with command precondition through the AgentRun mailbox endpoint", async () => {
    await moveAgentRunMailboxMessage("run/1", "agent/1", "message/1", {
      command: command("move_mailbox_message"),
      client_command_id: "move-message-1",
      after_message_id: "message/0",
    });

    expect(mocks.apiPutMock).toHaveBeenCalledWith(
      "/agent-runs/run%2F1/agents/agent%2F1/mailbox/messages/message%2F1/move",
      {
        command: command("move_mailbox_message"),
        client_command_id: "move-message-1",
        after_message_id: "message/0",
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
