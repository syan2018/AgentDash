/**
 * Task 专属事件卡片
 *
 * 仅渲染 event.type 以 task_ 开头的 session_info_update。
 * 复用 EventFullCard 模板，badge 是唯一染色点。
 */

import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { extractAgentDashMetaFromUpdate, isRecord } from "../model/agentdashMeta";
import { isTaskEventUpdate } from "./AcpTaskEventGuard";
import { EventFullCard } from "./EventCards";

export interface AcpTaskEventCardProps {
  update: SessionUpdate;
}

const TASK_EVENT_LABELS: Record<string, string> = {
  task_start_accepted:   "任务已启动",
  task_continue_accepted: "任务继续执行",
  task_cancel_requested: "任务已取消",
  task_start_failed:     "任务启动失败",
};

export function AcpTaskEventCard({ update }: AcpTaskEventCardProps) {
  if (!isTaskEventUpdate(update)) return null;

  const meta = extractAgentDashMetaFromUpdate(update);
  const eventType = meta?.event?.type ?? "task_event";
  const label = TASK_EVENT_LABELS[eventType] ?? eventType;
  const message = meta?.event?.message ?? label;
  const data = isRecord(meta?.event?.data) ? meta?.event?.data : null;

  const fromStatus = typeof data?.from === "string" ? data.from : null;
  const toStatus = typeof data?.to === "string" ? data.to : null;

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
