import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type {
  AgentDashCompactionStatus,
  AgentDashThreadItem,
} from "../../../generated/backbone-protocol";
import { renderToolCallCard } from "./toolCardRegistry";

describe("context compaction card", () => {
  it.each([
    ["succeeded", "completed", "上下文已压缩"],
    ["failed", "failed", "压缩应用失败"],
    ["lost", "lost", "压缩终态恢复失败"],
    ["cancelled", "cancelled", "上下文压缩已取消"],
  ] as const)(
    "renders %s terminal evidence",
    (status, displayStatus, expectedText) => {
      const item = compactionItem(
        status,
        status === "failed"
          ? "压缩应用失败"
          : status === "lost"
            ? "压缩终态恢复失败"
            : null,
      );

      const card = renderToolCallCard(item, { itemLifecycle: "updated" });

      expect(card.status).toBe(displayStatus);
      expect(renderToStaticMarkup(<>{card.body}</>)).toContain(expectedText);
    },
  );
});

function compactionItem(
  status: AgentDashCompactionStatus,
  error: string | null,
): AgentDashThreadItem {
  return {
    type: "contextCompaction",
    id: "compaction-1",
    mode: "manual",
    status,
    error,
    startedAtMs: 1n,
    completedAtMs: 2n,
    contextRevision: status === "succeeded" ? "context-2" : null,
  };
}
