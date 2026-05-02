/**
 * Task 专属事件卡片
 *
 * 仅渲染 platform event 中以 task_ 开头的事件。
 */

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import { extractPlatformEventType, extractPlatformEventData, extractPlatformEventMessage, isRecord } from "../model/agentdashMeta";
import { isTaskEventUpdate } from "./SessionTaskEventGuard";
import { EventFullCard } from "./EventCards";

export interface AcpTaskEventCardProps {
  event: BackboneEvent;
}

const TASK_EVENT_LABELS: Record<string, string> = {
  task_start_accepted:   "任务已启动",
  task_continue_accepted: "任务继续执行",
  task_cancel_requested: "任务已取消",
  task_start_failed:     "任务启动失败",
};

export function AcpTaskEventCard({ event }: AcpTaskEventCardProps) {
  if (!isTaskEventUpdate(event)) return null;

  const eventType = extractPlatformEventType(event) ?? "task_event";
  const label = TASK_EVENT_LABELS[eventType] ?? eventType;
  const message = extractPlatformEventMessage(event) ?? label;
  const data = extractPlatformEventData(event);

  const fromStatus = isRecord(data) && typeof data.from === "string" ? data.from : null;
  const toStatus = isRecord(data) && typeof data.to === "string" ? data.to : null;

  const isFailure =
    eventType === "task_start_failed" || eventType === "task_cancel_requested";

  const badgeClass = isFailure
    ? "border-destructive/25 bg-destructive/8 text-destructive"
    : "border-success/25 bg-success/8 text-success";

  const detailLines: string[] = [];
  if (fromStatus && toStatus) detailLines.push(`${fromStatus} → ${toStatus}`);
  else if (toStatus) detailLines.push(`状态：${toStatus}`);

  return (
    <EventFullCard
      badgeToken="TASK"
      badgeClass={badgeClass}
      subtitle={label}
      message={message}
      detailLines={detailLines}
    />
  );
}

export default AcpTaskEventCard;
