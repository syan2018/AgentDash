import type { StoryStatus, TaskStatus, StoryPriority, StoryType } from "../../types";

const storyStatusConfig: Record<StoryStatus, { label: string; className: string }> = {
  draft: { label: "草稿", className: "border-border bg-secondary text-muted-foreground" },
  ready: { label: "就绪", className: "border-info/20 bg-info/10 text-info" },
  running: { label: "执行中", className: "border-primary/20 bg-primary/10 text-primary" },
  review: { label: "待验收", className: "border-warning/20 bg-warning/10 text-warning" },
  completed: { label: "已完成", className: "border-success/20 bg-success/10 text-success" },
  failed: { label: "失败", className: "border-destructive/20 bg-destructive/10 text-destructive" },
  cancelled: { label: "已取消", className: "border-border bg-secondary text-muted-foreground" },
};

const storyPriorityConfig: Record<StoryPriority, { label: string; className: string; dotColor: string }> = {
  p0: { label: "P0", className: "border-destructive/20 bg-destructive/10 text-destructive", dotColor: "bg-destructive" },
  p1: { label: "P1", className: "border-warning/20 bg-warning/10 text-warning", dotColor: "bg-warning" },
  p2: { label: "P2", className: "border-primary/20 bg-primary/10 text-primary", dotColor: "bg-primary" },
  p3: { label: "P3", className: "border-border bg-secondary text-muted-foreground", dotColor: "bg-muted-foreground" },
};

const storyTypeConfig: Record<StoryType, { label: string; icon: string; className: string }> = {
  feature: { label: "功能", icon: "FEAT", className: "border-primary/20 bg-primary/10 text-primary" },
  bugfix: { label: "缺陷", icon: "BUG", className: "border-destructive/20 bg-destructive/10 text-destructive" },
  refactor: { label: "重构", icon: "REF", className: "border-warning/20 bg-warning/10 text-warning" },
  docs: { label: "文档", icon: "DOC", className: "border-info/20 bg-info/10 text-info" },
  test: { label: "测试", icon: "TEST", className: "border-success/20 bg-success/10 text-success" },
  other: { label: "其他", icon: "OTHR", className: "border-border bg-secondary text-muted-foreground" },
};

const taskStatusConfig: Record<TaskStatus, { label: string; className: string }> = {
  pending: { label: "待执行", className: "border-border bg-secondary text-muted-foreground" },
  assigned: { label: "已分配", className: "border-info/20 bg-info/10 text-info" },
  running: { label: "执行中", className: "border-primary/20 bg-primary/10 text-primary" },
  awaiting_verification: { label: "待验收", className: "border-warning/20 bg-warning/10 text-warning" },
  completed: { label: "已完成", className: "border-success/20 bg-success/10 text-success" },
  failed: { label: "失败", className: "border-destructive/20 bg-destructive/10 text-destructive" },
};

interface BadgeProps {
  className?: string;
}

export function StoryStatusBadge({ status, className = "" }: BadgeProps & { status: StoryStatus }) {
  const config = storyStatusConfig[status];
  return (
    <span className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium ${config.className} ${className}`}>
      {status === "running" && <span className="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-current" />}
      {config.label}
    </span>
  );
}

export function TaskStatusBadge({ status, className = "" }: BadgeProps & { status: TaskStatus }) {
  const config = taskStatusConfig[status];
  return (
    <span className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium ${config.className} ${className}`}>
      {status === "running" && <span className="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-current" />}
      {config.label}
    </span>
  );
}

export function StoryPriorityBadge({ priority, showLabel = false, className = "" }: BadgeProps & { priority: StoryPriority; showLabel?: boolean }) {
  const config = storyPriorityConfig[priority];
  return (
    <span className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-semibold ${config.className} ${className}`}>
      <span className={`inline-block h-1.5 w-1.5 rounded-full ${config.dotColor}`} />
      {showLabel && config.label}
    </span>
  );
}

export function StoryTypeBadge({ type, showIcon = true, className = "" }: BadgeProps & { type: StoryType; showIcon?: boolean }) {
  const config = storyTypeConfig[type];
  return (
    <span className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-medium ${config.className} ${className}`}>
      {showIcon && <span className="text-[9px] font-semibold tracking-[0.12em]">{config.icon}</span>}
      <span>{config.label}</span>
    </span>
  );
}
