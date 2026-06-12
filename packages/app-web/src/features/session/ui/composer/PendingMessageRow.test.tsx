import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type {
  PendingMessageView,
  PendingQueueStateView,
} from "../../../../generated/workflow-contracts";
import { PendingMessageList } from "./PendingMessageRow";

const pendingMessage: PendingMessageView = {
  id: "pending-1",
  preview: "继续处理下一步",
  has_images: false,
  created_at: "2026-06-12T00:00:00.000Z",
};

function renderPendingList(options: {
  queue?: PendingQueueStateView;
  canPromote: boolean;
}) {
  return renderToStaticMarkup(
    <PendingMessageList
      messages={[pendingMessage]}
      queue={options.queue}
      canPromote={options.canPromote}
      onPromote={() => {}}
      onDelete={() => {}}
      onResume={() => {}}
    />,
  );
}

describe("PendingMessageList", () => {
  it("shows pending messages outside running mode without exposing promote", () => {
    const markup = renderPendingList({ canPromote: false });

    expect(markup).toContain("继续处理下一步");
    expect(markup).toContain("删除");
    expect(markup).not.toContain("引导");
  });

  it("shows promote only when steer is currently available", () => {
    const markup = renderPendingList({ canPromote: true });

    expect(markup).toContain("引导");
  });

  it("shows paused queue status and resume action", () => {
    const markup = renderPendingList({
      canPromote: false,
      queue: {
        paused: true,
        pause_reason: "turn_interrupted",
        message: "上一轮已中断，pending 队列已暂停。",
        can_resume: true,
      },
    });

    expect(markup).toContain("Pending 队列已暂停");
    expect(markup).toContain("上一轮已中断，pending 队列已暂停。");
    expect(markup).toContain("恢复");
    expect(markup).not.toContain("引导");
  });
});
