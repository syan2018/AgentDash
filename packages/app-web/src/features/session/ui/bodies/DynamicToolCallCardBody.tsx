/**
 * DynamicToolCall body — 入参/出参双分区，复用 GenericJsonBody
 */

import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { GenericJsonBody } from "./GenericJsonBody";

type DynamicItem = Extract<ThreadItem, { type: "dynamicToolCall" }>;

export function DynamicToolCallCardBody({ item }: { item: DynamicItem }) {
  return (
    <GenericJsonBody
      arguments={item.arguments}
      contentItems={item.contentItems}
    />
  );
}
