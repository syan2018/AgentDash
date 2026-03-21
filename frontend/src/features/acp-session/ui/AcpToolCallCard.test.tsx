import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { AcpToolCallCard } from "./AcpToolCallCard";

describe("AcpToolCallCard", () => {
  it("在 pending approval 时渲染审批按钮", () => {
    const html = renderToStaticMarkup(
      <AcpToolCallCard
        sessionId="sess-approval-1"
        isPendingApproval
        update={{
          sessionUpdate: "tool_call",
          toolCallId: "tool-call-1",
          title: "执行 shell_exec",
          kind: "execute",
          status: "pending",
          content: [],
          rawInput: { command: "cargo test", cwd: "." },
        }}
      />,
    );

    expect(html).toContain("等待用户审批");
    expect(html).toContain("批准");
    expect(html).toContain("拒绝");
  });

  it("当 rawOutput 标记 approval_state=rejected 时显示拒绝状态", () => {
    const html = renderToStaticMarkup(
      <AcpToolCallCard
        sessionId="sess-approval-2"
        update={{
          sessionUpdate: "tool_call_update",
          toolCallId: "tool-call-2",
          title: "执行 shell_exec",
          kind: "execute",
          status: "failed",
          content: [],
          rawInput: { command: "rm -rf build", cwd: "." },
          rawOutput: {
            approval_state: "rejected",
            reason: "用户拒绝执行",
          },
        }}
      />,
    );

    expect(html).toContain("已拒绝");
  });
});
