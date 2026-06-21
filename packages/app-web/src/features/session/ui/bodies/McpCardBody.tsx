/**
 * MCP 工具调用 body — 入参 / 出参 / structuredContent / _meta / error 分区
 */

import { useMemo } from "react";
import type { JsonValue } from "../../../../generated/common-contracts";
import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { JsonTree, CopyJsonButton } from "./JsonTree";
import { normalizeMcpOutput } from "./toolOutputContent";
import { ToolOutputContentViewer } from "./ToolOutputContentViewer";
import { CB } from "./cardBodyTokens";

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
    <div className={CB.sectionGap}>
      {hasArgs && (
        <div>
          <div className="mb-1 flex items-center justify-between">
            <p className={CB.sectionTitle}>入参</p>
            <CopyJsonButton data={args} />
          </div>
          <JsonTree data={args} defaultDepth={2} />
        </div>
      )}

      {outputBlocks.length > 0 && (
        <div>
          <p className={`mb-1 ${CB.sectionTitle}`}>出参</p>
          <ToolOutputContentViewer blocks={outputBlocks} />
        </div>
      )}

      {hasStructured && (
        <div>
          <div className="mb-1 flex items-center justify-between">
            <p className={CB.sectionTitle}>structuredContent</p>
            <CopyJsonButton data={result!.structuredContent} />
          </div>
          <JsonTree data={result!.structuredContent} defaultDepth={1} />
        </div>
      )}

      {hasMeta && (
        <div>
          <div className="mb-1 flex items-center justify-between">
            <p className={CB.sectionTitle}>_meta</p>
            <CopyJsonButton data={result!._meta} />
          </div>
          <JsonTree data={result!._meta} defaultDepth={1} />
        </div>
      )}

      {item.error && (
        <div className={CB.errorBlock}>
          {item.error.message}
        </div>
      )}
    </div>
  );
}
