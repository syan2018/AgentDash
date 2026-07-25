import type { TurnSegment } from "../model/useSessionFeed";
import { isAggregatedGroup, isAggregatedThinkingGroup } from "../model/types";
import type { SessionDisplayItem } from "../model/types";

function getItemKey(item: SessionDisplayItem): string {
  if (isAggregatedGroup(item)) return item.groupKey;
  if (isAggregatedThinkingGroup(item)) return item.groupKey;
  return item.id;
}

export function getTurnSectionKey(segment: TurnSegment): string {
  const firstItem = segment.items[0];
  if (firstItem) {
    return `turn-section:${segment.turnId ?? "unscoped"}:${getItemKey(firstItem)}`;
  }
  return `turn-section:${segment.turnId ?? "unscoped"}:status`;
}
