import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { TaskToolCardBody } from "./TaskToolCardBody";

describe("TaskToolCardBody", () => {
  it("renders the runtime task view instead of the generic JSON fallback", () => {
    const view = {
      mode: "list",
      tasks: [{
        id: "task-1",
        title: "修复工具卡展示",
        status: "active",
      }],
    };
    const item: Extract<ThreadItem, { type: "dynamicToolCall" }> = {
      type: "dynamicToolCall",
      id: "task-read-1",
      tool: "task_read",
      namespace: null,
      arguments: { mode: "list" },
      status: "completed",
      contentItems: [{
        type: "inputText",
        text: `Task view 已读取\n${JSON.stringify(view)}`,
      }],
      success: true,
      durationMs: null,
    };

    const html = renderToStaticMarkup(<TaskToolCardBody item={item} />);

    expect(html).toContain("修复工具卡展示");
    expect(html).toContain("active");
    expect(html).not.toContain("入参");
    expect(html).not.toContain("出参");
  });
});
