/**
 * Task 专属事件卡片
 *
 * 仅渲染 event.type 以 task_ 开头的 session_info_update。
 */

import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { extractAgentDashMetaFromUpdate } from "../model/agentdashMeta";

export interface AcpTaskEventCardProps {
  update: SessionUpdate;
}

const TASK_EVENT_LABELS: Record<string, string> = {
  task_start_accepted: "任务已启动",
  task_continue_accepted: "任务继续执行",
  task_cancel_requested: "任务已取消",
  task_start_failed: "任务启动失败",
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

export function isTaskEventUpdate(update: SessionUpdate): boolean {
  if (update.sessionUpdate !== "session_info_update") return false;
  const eventType = extractAgentDashMetaFromUpdate(update)?.event?.type;
  return typeof eventType === "string" && eventType.startsWith("task_");
}

export function AcpTaskEventCard({ update }: AcpTaskEventCardProps) {
  if (!isTaskEventUpdate(update)) return null;

  const meta = extractAgentDashMetaFromUpdate(update);
  const eventType = meta?.event?.type ?? "task_event";
  const label = TASK_EVENT_LABELS[eventType] ?? eventType;
  const message = meta?.event?.message ?? label;
  const data = isRecord(meta?.event?.data) ? meta?.event?.data : null;

  const fromStatus = typeof data?.from === "string" ? data.from : null;
  const toStatus = typeof data?.to === "string" ? data.to : null;
  const taskId = typeof data?.task_id === "string" ? data.task_id : null;

  return (
    <div className="rounded-md border border-emerald-500/30 bg-emerald-500/10 px-3 py-2">
      <div className="flex items-center gap-2 text-xs">
        <span>📌</span>
        <span className="font-medium text-emerald-700">{label}</span>
        {toStatus && <span className="rounded bg-emerald-500/15 px-1.5 py-0.5 text-[10px] text-emerald-700">{toStatus}</span>}
      </div>
      <p className="mt-1 text-xs text-foreground/80">{message}</p>
      {(fromStatus || taskId) && (
        <p className="mt-1 text-[10px] text-muted-foreground font-mono">
          {taskId ? `task=${taskId}` : ""}
          {taskId && fromStatus ? " · " : ""}
          {fromStatus && toStatus ? `${fromStatus} -> ${toStatus}` : ""}
        </p>
      )}
    </div>
  );
}

export default AcpTaskEventCard;
