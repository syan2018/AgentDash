import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type {
  ConversationCommandView,
  ConversationMailboxSnapshotView,
} from "../../../generated/workflow-contracts";
import type {
  MailboxStateView,
  MailboxMessageView,
} from "../../../generated/agent-run-mailbox-contracts";
import { MailboxMessageList } from "./MailboxMessageRow";

const mailboxMessage: MailboxMessageView = {
  id: "mailbox-1",
  origin: "user",
  source: {
    namespace: "core",
    kind: "composer",
    actor: "user",
    display_label_key: "mailbox.source.core.composer",
  },
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
  can_reorder: true,
  can_recall: true,
};

function renderMailboxList(options: {
  messages?: MailboxMessageView[];
  mailbox?: ConversationMailboxSnapshotView;
  mailboxState?: MailboxStateView;
  promoteCommand?: ConversationCommandView;
  deleteCommand?: ConversationCommandView;
}) {
  return renderToStaticMarkup(
    <MailboxMessageList
      messages={options.messages ?? [mailboxMessage]}
      mailbox={options.mailbox}
      mailboxState={options.mailboxState}
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
  it("renders message preview and delete action, hides internal state", () => {
    const markup = renderMailboxList({ deleteCommand });

    expect(markup).toContain("继续处理下一步");
    expect(markup).toContain("删除");
    // 不应暴露后端状态机概念
    expect(markup).not.toContain("排队中");
    expect(markup).not.toContain("Run 边界");
    expect(markup).not.toContain("启动或继续");
    expect(markup).not.toContain("Loop 边界");
    expect(markup).not.toContain("Stop continuation");
  });

  it("shows promote button only when command enabled and message can_promote", () => {
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

    expect(markup).toContain("注入当前轮");
  });

  it("hides promote when message cannot be promoted", () => {
    const markup = renderMailboxList({
      messages: [{ ...mailboxMessage, can_promote: false }],
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

    expect(markup).not.toContain("注入当前轮");
  });

  it("does not render when no messages and no user attention", () => {
    const markup = renderMailboxList({
      messages: [],
      mailbox: {
        paused: true,
        visible_message_count: 0,
        user_attention: false,
        messages: [],
      },
    });

    expect(markup).toBe("");
  });

  it("shows pause banner with resume action", () => {
    const markup = renderMailboxList({
      messages: [],
      mailbox: {
        paused: true,
        visible_message_count: 0,
        user_attention: true,
        messages: [],
        resume_command: {
          kind: "resume_mailbox",
          command_id: "cmd-resume",
          enabled: true,
          unavailable_reason: "上一轮已中断。",
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
      mailboxState: {
        paused: true,
        pause_reason: "turn_failed",
        message: "后端暂停消息",
        can_resume: true,
        hide_system_steer_messages: false,
      },
    });

    expect(markup).toContain("消息投递已暂停");
    expect(markup).toContain("恢复");
    // 不应直接输出后端技术信息
    expect(markup).not.toContain("后端暂停消息");
    expect(markup).not.toContain("Mailbox");
  });

  it("shows failure hint for blocked messages without exposing internal error", () => {
    const markup = renderMailboxList({
      messages: [
        {
          ...mailboxMessage,
          status: "blocked",
          barrier: "agent_loop_turn_boundary",
          delivery: { kind: "steer_active_turn", stop_effect: "continue_on_stop" },
          attempt_count: 2,
          last_error: "delivery_result_unknown",
          can_promote: false,
          can_reorder: false,
          can_recall: false,
        },
      ],
      deleteCommand,
    });

    expect(markup).toContain("继续处理下一步");
    expect(markup).toContain("失败");
    // 不应暴露后端状态机概念
    expect(markup).not.toContain("已阻塞");
    expect(markup).not.toContain("Loop 边界");
    expect(markup).not.toContain("Stop continuation");
    expect(markup).not.toContain("2 次尝试");
    expect(markup).not.toContain("delivery_result_unknown");
  });

  it("shows image indicator in preview", () => {
    const markup = renderMailboxList({
      messages: [{ ...mailboxMessage, has_images: true }],
      deleteCommand,
    });

    expect(markup).toContain("[图]");
  });
});
