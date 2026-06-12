import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type {
  ConversationCommandView,
  ConversationPendingSnapshotView,
  PendingMessageView,
} from "../../../../generated/workflow-contracts";
import { PendingMessageList } from "./PendingMessageRow";

const pendingMessage: PendingMessageView = {
  id: "pending-1",
  preview: "继续处理下一步",
  has_images: false,
  created_at: "2026-06-12T00:00:00.000Z",
};

function renderPendingList(options: {
  messages?: PendingMessageView[];
  pending?: ConversationPendingSnapshotView;
  promoteCommand?: ConversationCommandView;
}) {
  return renderToStaticMarkup(
    <PendingMessageList
      messages={options.messages ?? [pendingMessage]}
      pending={options.pending}
      promoteCommand={options.promoteCommand}
      onPromote={() => {}}
      onDelete={() => {}}
      onResume={() => {}}
    />,
  );
}

describe("PendingMessageList", () => {
  it("shows pending messages outside running mode without exposing promote", () => {
    const markup = renderPendingList({});

    expect(markup).toContain("继续处理下一步");
    expect(markup).toContain("删除");
    expect(markup).not.toContain("引导");
  });

  it("shows promote only when snapshot exposes pending row command", () => {
    const markup = renderPendingList({
      promoteCommand: {
        kind: "promote_pending",
        command_id: "cmd-promote",
        enabled: true,
        requires_input: false,
        executor_config_policy: "forbidden",
        placement: ["pending_row"],
        stale_guard: {
          run_id: "run-1",
          agent_id: "agent-1",
          runtime_session_id: "session-1",
          active_turn_id: "turn-1",
        },
      },
    });

    expect(markup).toContain("引导");
  });

  it("does not render paused empty queue without user attention", () => {
    const markup = renderPendingList({
      messages: [],
      pending: {
        paused: true,
        visible_message_count: 0,
        user_attention: false,
      },
    });

    expect(markup).toBe("");
  });

  it("shows paused queue status and resume action from snapshot", () => {
    const markup = renderPendingList({
      messages: [],
      pending: {
        paused: true,
        visible_message_count: 0,
        user_attention: true,
        resume_command: {
          kind: "resume_pending_queue",
          command_id: "cmd-resume",
          enabled: true,
          unavailable_reason: "上一轮已中断，pending 队列已暂停。",
          requires_input: false,
          executor_config_policy: "forbidden",
          placement: ["pending_banner"],
          stale_guard: {
            run_id: "run-1",
            agent_id: "agent-1",
            runtime_session_id: "session-1",
          },
        },
      },
    });

    expect(markup).toContain("Pending 队列已暂停");
    expect(markup).toContain("上一轮已中断，pending 队列已暂停。");
    expect(markup).toContain("恢复");
    expect(markup).not.toContain("引导");
  });
});
