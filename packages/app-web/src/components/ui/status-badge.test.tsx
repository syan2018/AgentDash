import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { TaskStatusBadge } from "./status-badge";
import type { TaskStatus } from "../../types";

describe("TaskStatusBadge", () => {
  it("渲染 Task plan 状态集合", () => {
    const statuses: TaskStatus[] = ["open", "active", "review", "blocked", "done", "dropped"];
    const html = renderToStaticMarkup(
      <>
        {statuses.map((status) => (
          <TaskStatusBadge key={status} status={status} />
        ))}
      </>,
    );

    for (const status of statuses) {
      expect(html).toContain(status);
    }
    expect(html).not.toContain("待验收");
  });
});
