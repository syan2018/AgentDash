/**
 * 通用 JSON 入参/出参双分区展示
 *
 * 兜底 body：对任何未注册专用 renderer 的工具调用，
 * 入参以 JSON 树展示，出参优先走 ToolOutputContentViewer。
 */

import { useMemo } from "react";
import type { DynamicToolCallOutputContentItem } from "../../../../generated/backbone-protocol";
import { JsonTree, CopyJsonButton } from "./JsonTree";
import { normalizeDynamicOutput, type ToolOutputBlock } from "./toolOutputContent";
import { ToolOutputContentViewer } from "./ToolOutputContentViewer";
import { CB } from "./cardBodyTokens";

export interface GenericJsonBodyProps {
  arguments?: unknown;
  contentItems?: unknown;
}

export function GenericJsonBody({ arguments: args, contentItems }: GenericJsonBodyProps) {
  const hasArgs = args != null && !(typeof args === "object" && args !== null && Object.keys(args).length === 0);
  const hasOutput = contentItems != null;

  const outputBlocks = useMemo((): ToolOutputBlock[] => {
    if (!hasOutput) return [];
    if (isDynamicContentItems(contentItems)) {
      return normalizeDynamicOutput(contentItems);
    }
    return [{ kind: "json", value: contentItems }];
  }, [contentItems, hasOutput]);

  if (!hasArgs && outputBlocks.length === 0) return null;

  return (
    <div className={CB.sectionGap}>
      {hasArgs && (
        <Section label="入参" data={args}>
          <JsonTree data={args} defaultDepth={2} />
        </Section>
      )}
      {outputBlocks.length > 0 && (
        <div>
          <p className={`mb-1 ${CB.sectionTitle}`}>出参</p>
          <ToolOutputContentViewer blocks={outputBlocks} />
        </div>
      )}
    </div>
  );
}

function Section({
  label,
  data,
  children,
}: {
  label: string;
  data: unknown;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="mb-1 flex items-center justify-between">
        <p className={CB.sectionTitle}>{label}</p>
        <CopyJsonButton data={data} />
      </div>
      {children}
    </div>
  );
}

function isDynamicContentItems(v: unknown): v is DynamicToolCallOutputContentItem[] {
  if (!Array.isArray(v)) return false;
  if (v.length === 0) return false;
  const first = v[0] as Record<string, unknown>;
  return first != null && typeof first === "object" && (first.type === "inputText" || first.type === "inputImage");
}
