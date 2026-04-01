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

  it("当结构化输入尚未闭合时展示草稿输入而不是空对象", () => {
    const html = renderToStaticMarkup(
      <AcpToolCallCard
        update={{
          sessionUpdate: "tool_call_update",
          toolCallId: "tool-call-3",
          title: "执行 fs_write",
          kind: "edit",
          status: "pending",
          content: [],
          rawInput: {},
          _meta: {
            agentdash: {
              v: 1,
              event: {
                type: "tool_call_draft",
                data: {
                  toolCallId: "tool-call-3",
                  toolName: "fs_write",
                  phase: "delta",
                  draftInput: "{\"path\":\"notes.txt\",\"content\":\"hello",
                  isParseable: false,
                },
              },
            },
          },
        } as any}
      />,
    );

    expect(html).toContain("草稿输入");
    expect(html).toContain("{&quot;path&quot;:&quot;notes.txt&quot;,&quot;content&quot;:&quot;hello");
    expect(html).not.toContain("<pre class=\"agentdash-chat-code-block\">{}</pre>");
  });
});
