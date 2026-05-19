import { StatusDot, type StatusDotSize, type StatusDotTone } from "@agentdash/ui";
import type { ProjectSessionEntry } from "../../types";

type SessionExecutionStatus = ProjectSessionEntry["execution_status"];

const sessionStatusDotTone: Record<SessionExecutionStatus, StatusDotTone> = {
  running: "success",
  idle: "muted",
  completed: "info",
  failed: "danger",
  interrupted: "warning",
};

const sessionStatusDotTitle: Record<SessionExecutionStatus, string> = {
  running: "运行中",
  idle: "空闲",
  completed: "已完成",
  failed: "失败",
  interrupted: "已中断",
};

interface SessionStatusDotProps {
  status: SessionExecutionStatus;
  size?: StatusDotSize;
  className?: string;
  title?: string;
}

export function SessionStatusDot({
  status,
  size = "sm",
  className = "shrink-0",
  title,
}: SessionStatusDotProps) {
  return (
    <StatusDot
      tone={sessionStatusDotTone[status]}
      size={size}
      pulse={status === "running"}
      className={className}
      title={title ?? sessionStatusDotTitle[status]}
    />
  );
}
