import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { ComposerSendButton } from "./ComposerSendButton";

describe("ComposerSendButton", () => {
  it("Runtime 执行中且输入为空时展示停止按钮", () => {
    const markup = renderToStaticMarkup(
      <ComposerSendButton
        isRunning
        isSending={false}
        isCancelling={false}
        cancelDisabled={false}
        hasContent={false}
        submitCommand={{
          command_id: "submit",
          kind: "submit_message",
          runtimeCommand: "submit_input",
          enabled: true,
          requires_input: true,
          executor_config_policy: "optional",
        }}
        onSubmit={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(markup).toContain('title="停止"');
    expect(markup).toContain("<rect");
    expect(markup).not.toContain("<path");
  });

  it("Runtime 执行中且已有输入时明确展示 Queue 与 Steer 入口", () => {
    const markup = renderToStaticMarkup(
      <ComposerSendButton
        isRunning
        isSending={false}
        isCancelling={false}
        cancelDisabled={false}
        hasContent
        submitCommand={{
          command_id: "submit",
          kind: "submit_message",
          runtimeCommand: "submit_input",
          enabled: true,
          requires_input: true,
          executor_config_policy: "optional",
        }}
        onSubmit={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(markup).toContain('title="排队（Ctrl/Cmd+Enter 可 Steer）"');
    expect(markup).toContain("<path");
    expect(markup).not.toContain("<rect");
  });
});
