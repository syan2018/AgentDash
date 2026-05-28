/**
 * MCP 工具调用 body — 入参/出参/错误分区
 */

import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { GenericJsonBody } from "./GenericJsonBody";

type McpItem = Extract<ThreadItem, { type: "mcpToolCall" }>;

export function McpCardBody({ item }: { item: McpItem }) {
  return (
    <div className="space-y-3">
      <GenericJsonBody
        arguments={item.arguments}
        contentItems={item.result?.content}
      />
      {item.error && (
        <div className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-2.5 py-2 text-xs text-destructive">
          {item.error.message}
        </div>
      )}
    </div>
  );
}
