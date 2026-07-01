import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { MailboxMessageView } from "../../../generated/agent-run-mailbox-contracts";
import type {
  SessionChatCommandModel,
  SessionChatMailboxModel,
} from "../../session/ui/SessionChatViewTypes";
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
  mailbox?: Partial<SessionChatMailboxModel>;
  promoteCommand?: SessionChatCommandModel;
  deleteCommand?: SessionChatCommandModel;
  onRecall?: (messageId: string) => void;
}) {
  const messages = options.messages ?? [mailboxMessage];
  const mailbox: SessionChatMailboxModel = {
    messages,
    paused: false,
    user_attention: false,
    hide_system_steer_messages: false,
    can_resume: false,
    ...options.mailbox,
    promoteAction: options.promoteCommand ?? options.mailbox?.promoteAction,
    deleteAction: options.deleteCommand ?? options.mailbox?.deleteAction,
  };
  return renderToStaticMarkup(
    <MailboxMessageList
      messages={messages}
      mailbox={mailbox}
      onPromote={() => {}}
      onDelete={() => {}}
      onResume={() => {}}
      onRecall={options.onRecall}
    />,
  );
}

const deleteCommand: SessionChatCommandModel = {
  kind: "delete_mailbox_message",
  command_id: "cmd-delete",
  enabled: true,
  requires_input: false,
  executor_config_policy: "forbidden",
};

const promoteCommand: SessionChatCommandModel = {
  kind: "promote_mailbox_message",
  command_id: "cmd-promote",
  enabled: true,
  requires_input: false,
  executor_config_policy: "forbidden",
};

describe("MailboxMessageList", () => {
  it("renders message preview and delete action, hides internal state", () => {
    const markup = renderMailboxList({ deleteCommand });

    expect(markup).toContain("继续处理下一步");
    expect(markup).toContain("用户输入");
    expect(markup).toContain("排队");
    expect(markup).toContain("删除");
    // 不应暴露后端状态机概念
    expect(markup).not.toContain("排队中");
    expect(markup).not.toContain("Run 边界");
    expect(markup).not.toContain("启动或继续");
    expect(markup).not.toContain("Loop 边界");
    expect(markup).not.toContain("Stop continuation");
  });

  it("renders Routine source identity with queued status", () => {
    const markup = renderMailboxList({
      messages: [
        {
          ...mailboxMessage,
          id: "routine-message-1",
          origin: "system",
          source: {
            namespace: "routine",
            kind: "trigger",
            source_ref: "routine-execution-1",
            correlation_ref: "routine-1",
            actor: "routine",
            display_label_key: "mailbox.source.routine.trigger",
          },
          preview: "Routine 后续触发",
          status: "queued",
          can_reorder: false,
          can_recall: false,
        },
      ],
      deleteCommand,
    });

    expect(markup).toContain("Routine 触发");
    expect(markup).toContain("Routine 后续触发");
    expect(markup).toContain("排队");
    expect(markup).not.toContain("mailbox.source.routine.trigger");
  });

  it("renders Companion source identities with paused and blocked status", () => {
    const markup = renderMailboxList({
      messages: [
        {
          ...mailboxMessage,
          id: "companion-dispatch-1",
          origin: "companion",
          source: {
            namespace: "companion",
            kind: "dispatch",
            source_ref: "dispatch-1",
            correlation_ref: "gate-1",
            actor: "agent",
            route: "sub",
            display_label_key: "mailbox.source.companion.dispatch",
          },
          preview: "派发给协作 Agent",
          status: "paused",
          can_promote: false,
          can_reorder: false,
          can_recall: false,
        },
        {
          ...mailboxMessage,
          id: "companion-human-response-1",
          origin: "companion",
          source: {
            namespace: "companion",
            kind: "human_response",
            source_ref: "gate-2",
            correlation_ref: "request-2",
            actor: "human",
            route: "human",
            display_label_key: "mailbox.source.companion.human_response",
          },
          preview: "用户已回应",
          status: "blocked",
          can_promote: false,
          can_reorder: false,
          can_recall: false,
        },
      ],
      deleteCommand,
    });

    expect(markup).toContain("Companion 派发");
    expect(markup).toContain("用户回应");
    expect(markup).toContain("暂停");
    expect(markup).toContain("阻塞");
    expect(markup).not.toContain("mailbox.source.companion.dispatch");
    expect(markup).not.toContain("mailbox.source.companion.human_response");
  });

  it("shows promote button only when command enabled and message can_promote", () => {
    const markup = renderMailboxList({
      deleteCommand,
      promoteCommand,
    });

    expect(markup).toContain("注入当前轮");
  });

  it("hides promote when message cannot be promoted", () => {
    const markup = renderMailboxList({
      messages: [{ ...mailboxMessage, can_promote: false }],
      deleteCommand,
      promoteCommand,
    });

    expect(markup).not.toContain("注入当前轮");
  });

  it("does not render when no messages and no user attention", () => {
    const markup = renderMailboxList({
      messages: [],
      mailbox: {
        paused: false,
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
        user_attention: true,
        can_resume: true,
        messages: [],
        resumeAction: {
          kind: "resume_mailbox",
          command_id: "cmd-resume",
          enabled: true,
          unavailable_reason: "上一轮已中断。",
          requires_input: false,
          executor_config_policy: "forbidden",
        },
      },
    });

    expect(markup).toContain("消息投递已暂停");
    expect(markup).toContain("恢复");
    // 不应直接输出后端技术信息
    expect(markup).not.toContain("后端暂停消息");
    expect(markup).not.toContain("Mailbox");
  });

  it("shows full failure detail for blocked messages", () => {
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
    expect(markup).toContain("阻塞");
    // 完整错误需要可见，便于用户判断下一步处理。
    expect(markup).not.toContain("已阻塞");
    expect(markup).not.toContain("Loop 边界");
    expect(markup).not.toContain("Stop continuation");
    expect(markup).not.toContain("2 次尝试");
    expect(markup).toContain("delivery_result_unknown");
  });

  it("shows retry row action for failed messages when recall handler exists", () => {
    const markup = renderMailboxList({
      messages: [
        {
          ...mailboxMessage,
          status: "failed",
          last_error: "backend executor unavailable",
          can_promote: false,
          can_recall: false,
        },
      ],
      deleteCommand,
      onRecall: () => {},
    });

    expect(markup).toContain("失败");
    expect(markup).toContain("backend executor unavailable");
    expect(markup).toContain("aria-label=\"重试\"");
    expect(markup).not.toContain("编辑");
  });

  it("shows image indicator in preview", () => {
    const markup = renderMailboxList({
      messages: [{ ...mailboxMessage, has_images: true }],
      deleteCommand,
    });

    expect(markup).toContain("[图]");
  });
});
