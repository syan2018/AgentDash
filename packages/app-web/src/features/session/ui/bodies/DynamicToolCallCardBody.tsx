/**
 * DynamicToolCall body — 按 tool 名分流到专用 renderer，未知 tool 走 GenericJsonBody。
 *
 * read → ReadCardBody（行号 + 折叠预览）
 * 其他 → GenericJsonBody（入参/出参双分区 JSON 树）
 */

import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { GenericJsonBody } from "./GenericJsonBody";
import { ReadCardBody } from "./ReadCardBody";

type DynamicItem = Extract<ThreadItem, { type: "dynamicToolCall" }>;

export function DynamicToolCallCardBody({ item }: { item: DynamicItem }) {
  const tool = item.tool.toLowerCase();

  if (tool === "read") {
    return <ReadCardBody item={item} />;
  }

  return (
    <GenericJsonBody
      arguments={item.arguments}
      contentItems={item.contentItems}
    />
  );
}
