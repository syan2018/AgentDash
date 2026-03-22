/**
 * Task 专属事件卡片
 *
 * 仅渲染 event.type 以 task_ 开头的 session_info_update。
 */

import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { extractAgentDashMetaFromUpdate, isRecord } from "../model/agentdashMeta";
import { isTaskEventUpdate } from "./AcpTaskEventGuard";

export interface AcpTaskEventCardProps {
  update: SessionUpdate;
}

const TASK_EVENT_LABELS: Record<string, string> = {
  task_start_accepted: "任务已启动",
  task_continue_accepted: "任务继续执行",
  task_cancel_requested: "任务已取消",
  task_start_failed: "任务启动失败",
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

  const isFailure = eventType === "task_start_failed" || eventType === "task_cancel_requested";
  const borderColor = isFailure ? "border-destructive/30" : "border-success/30";
  const statusColor = isFailure ? "text-destructive" : "text-success";

  return (
    <div className={`rounded-[12px] border ${borderColor} bg-background px-3 py-2.5`}>
      <div className="flex items-center gap-2.5 text-xs">
        <span className={`inline-flex rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground`}>
          TASK
        </span>
        <span className={`font-medium ${statusColor}`}>{label}</span>
        {toStatus && (
          <span className={`rounded-[6px] border ${borderColor} bg-secondary/60 px-1.5 py-0.5 text-[10px] ${statusColor}`}>
            {toStatus}
          </span>
        )}
      </div>
      <p className="mt-1.5 text-xs text-foreground/80">{message}</p>
      {fromStatus && toStatus && (
        <p className="mt-1.5 text-[10px] text-muted-foreground">
          {fromStatus} → {toStatus}
        </p>
      )}
    </div>
  );
}

export default AcpTaskEventCard;
