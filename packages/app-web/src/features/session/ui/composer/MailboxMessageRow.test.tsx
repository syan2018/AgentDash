import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type {
  ConversationCommandView,
  ConversationMailboxSnapshotView,
  MailboxMessageView,
} from "../../../../generated/workflow-contracts";
import { MailboxMessageList } from "./MailboxMessageRow";

const mailboxMessage: MailboxMessageView = {
  id: "mailbox-1",
  origin: "user",
  source: "composer",
  delivery: { kind: "launch_or_continue_turn" },
  barrier: "agent_run_turn_boundary",
  drain_mode: "one",
  status: "queued",
  preview: "继续处理下一步",
  has_images: false,
  attempt_count: 0,
  created_at: "2026-06-12T00:00:00.000Z",
  updated_at: "2026-06-12T00:00:00.000Z",
  can_promote: true,
  can_delete: true,
};

function renderMailboxList(options: {
  messages?: MailboxMessageView[];
  mailbox?: ConversationMailboxSnapshotView;
  promoteCommand?: ConversationCommandView;
  deleteCommand?: ConversationCommandView;
}) {
  return renderToStaticMarkup(
    <MailboxMessageList
      messages={options.messages ?? [mailboxMessage]}
      mailbox={options.mailbox}
      promoteCommand={options.promoteCommand}
      deleteCommand={options.deleteCommand}
      onPromote={() => {}}
      onDelete={() => {}}
      onResume={() => {}}
    />,
  );
}

const deleteCommand: ConversationCommandView = {
  kind: "delete_mailbox_message",
  command_id: "cmd-delete",
  enabled: true,
  requires_input: false,
  executor_config_policy: "forbidden",
  placement: ["mailbox_row"],
  stale_guard: {
    snapshot_id: "snapshot-delete",
    run_id: "run-1",
    agent_id: "agent-1",
    runtime_session_id: "session-1",
  },
};

describe("MailboxMessageList", () => {
  it("shows mailbox messages outside running mode without exposing promote by default", () => {
    const markup = renderMailboxList({ deleteCommand });

    expect(markup).toContain("继续处理下一步");
    expect(markup).toContain("删除");
    expect(markup).not.toContain("引导");
  });

  it("shows promote only when snapshot exposes mailbox row command", () => {
    const markup = renderMailboxList({
      deleteCommand,
      promoteCommand: {
        kind: "promote_mailbox_message",
        command_id: "cmd-promote",
        enabled: true,
        requires_input: false,
        executor_config_policy: "forbidden",
        placement: ["mailbox_row"],
        stale_guard: {
          snapshot_id: "snapshot-promote",
          run_id: "run-1",
          agent_id: "agent-1",
          runtime_session_id: "session-1",
          active_turn_id: "turn-1",
        },
      },
    });

    expect(markup).toContain("引导");
  });

  it("does not render paused empty mailbox without user attention", () => {
    const markup = renderMailboxList({
      messages: [],
      mailbox: {
        paused: true,
        visible_message_count: 0,
        user_attention: false,
      },
    });

    expect(markup).toBe("");
  });

  it("shows paused mailbox status and resume action from snapshot", () => {
    const markup = renderMailboxList({
      messages: [],
      mailbox: {
        paused: true,
        visible_message_count: 0,
        user_attention: true,
        resume_command: {
          kind: "resume_mailbox",
          command_id: "cmd-resume",
          enabled: true,
          unavailable_reason: "上一轮已中断，Mailbox 已暂停。",
          requires_input: false,
          executor_config_policy: "forbidden",
          placement: ["mailbox_banner"],
          stale_guard: {
            snapshot_id: "snapshot-resume",
            run_id: "run-1",
            agent_id: "agent-1",
            runtime_session_id: "session-1",
          },
        },
      },
    });

    expect(markup).toContain("Mailbox 已暂停");
    expect(markup).toContain("上一轮已中断，Mailbox 已暂停。");
    expect(markup).toContain("恢复");
    expect(markup).not.toContain("引导");
  });
});
