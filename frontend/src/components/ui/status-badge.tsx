import type { StoryStatus, TaskStatus } from "../../types";

const storyStatusConfig: Record<StoryStatus, { label: string; className: string }> = {
  draft: { label: "草稿", className: "bg-muted text-muted-foreground" },
  ready: { label: "就绪", className: "bg-info/15 text-info" },
  running: { label: "执行中", className: "bg-primary/15 text-primary" },
  review: { label: "待验收", className: "bg-warning/15 text-warning" },
  completed: { label: "已完成", className: "bg-success/15 text-success" },
  failed: { label: "失败", className: "bg-destructive/15 text-destructive" },
  cancelled: { label: "已取消", className: "bg-muted text-muted-foreground" },
};

const taskStatusConfig: Record<TaskStatus, { label: string; className: string }> = {
  pending: { label: "待执行", className: "bg-muted text-muted-foreground" },
  queued: { label: "排队中", className: "bg-info/15 text-info" },
  running: { label: "执行中", className: "bg-primary/15 text-primary" },
  succeeded: { label: "成功", className: "bg-success/15 text-success" },
  failed: { label: "失败", className: "bg-destructive/15 text-destructive" },
  skipped: { label: "已跳过", className: "bg-muted text-muted-foreground" },
  cancelled: { label: "已取消", className: "bg-muted text-muted-foreground" },
};

interface BadgeProps {
  className?: string;
}

export function StoryStatusBadge({ status, className = "" }: BadgeProps & { status: StoryStatus }) {
  const config = storyStatusConfig[status];
  return (
    <span className={`inline-flex rounded-full px-2.5 py-0.5 text-xs font-medium ${config.className} ${className}`}>
      {status === "running" && <span className="mr-1.5 mt-1 inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-current" />}
      {config.label}
    </span>
  );
}

export function TaskStatusBadge({ status, className = "" }: BadgeProps & { status: TaskStatus }) {
  const config = taskStatusConfig[status];
  return (
    <span className={`inline-flex rounded-full px-2.5 py-0.5 text-xs font-medium ${config.className} ${className}`}>
      {status === "running" && <span className="mr-1.5 mt-1 inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-current" />}
      {config.label}
    </span>
  );
}
