/**
 * MCP 工具调用 body — 入参 / 出参 / structuredContent / _meta / error 分区
 */

import { useMemo } from "react";
import type { ThreadItem, JsonValue } from "../../../../generated/backbone-protocol";
import { JsonTree, CopyJsonButton } from "./JsonTree";
import { normalizeMcpOutput } from "./toolOutputContent";
import { ToolOutputContentViewer } from "./ToolOutputContentViewer";

type McpItem = Extract<ThreadItem, { type: "mcpToolCall" }>;

export function McpCardBody({ item }: { item: McpItem }) {
  const args = item.arguments as Record<string, unknown> | null;
  const hasArgs = args != null && Object.keys(args).length > 0;
  const result = item.result;

  const outputBlocks = useMemo(
    () => normalizeMcpOutput(result?.content as JsonValue[] | null | undefined),
    [result?.content],
  );

  const hasStructured = result?.structuredContent != null;
  const hasMeta = result?._meta != null;

  return (
    <div className="space-y-3">
      {hasArgs && (
        <div>
          <div className="mb-1 flex items-center justify-between">
            <p className="text-xs font-medium text-muted-foreground/60">入参</p>
            <CopyJsonButton data={args} />
          </div>
          <JsonTree data={args} defaultDepth={2} />
        </div>
      )}

      {outputBlocks.length > 0 && (
        <div>
          <p className="mb-1 text-xs font-medium text-muted-foreground/60">出参</p>
          <ToolOutputContentViewer blocks={outputBlocks} />
        </div>
      )}

      {hasStructured && (
        <div>
          <div className="mb-1 flex items-center justify-between">
            <p className="text-xs font-medium text-muted-foreground/60">structuredContent</p>
            <CopyJsonButton data={result!.structuredContent} />
          </div>
          <JsonTree data={result!.structuredContent} defaultDepth={1} />
        </div>
      )}

      {hasMeta && (
        <div>
          <div className="mb-1 flex items-center justify-between">
            <p className="text-xs font-medium text-muted-foreground/60">_meta</p>
            <CopyJsonButton data={result!._meta} />
          </div>
          <JsonTree data={result!._meta} defaultDepth={1} />
        </div>
      )}

      {item.error && (
        <div className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-2.5 py-2 text-xs text-destructive">
          {item.error.message}
        </div>
      )}
    </div>
  );
}
