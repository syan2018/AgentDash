import type { StoryStatus, TaskStatus, StoryPriority, StoryType } from "../../types";

const storyStatusConfig: Record<StoryStatus, { label: string; className: string }> = {
  draft: { label: "草稿", className: "bg-secondary text-muted-foreground border border-muted" },
  ready: { label: "就绪", className: "bg-info/15 text-info" },
  running: { label: "执行中", className: "bg-primary/15 text-primary" },
  review: { label: "待验收", className: "bg-warning/15 text-warning" },
  completed: { label: "已完成", className: "bg-success/15 text-success" },
  failed: { label: "失败", className: "bg-destructive/15 text-destructive" },
  cancelled: { label: "已取消", className: "bg-muted text-muted-foreground" },
};

const storyPriorityConfig: Record<StoryPriority, { label: string; className: string; dotColor: string }> = {
  p0: { label: "P0", className: "bg-destructive/15 text-destructive border border-destructive/30", dotColor: "bg-destructive" },
  p1: { label: "P1", className: "bg-warning/15 text-warning border border-warning/30", dotColor: "bg-warning" },
  p2: { label: "P2", className: "bg-info/15 text-info border border-info/30", dotColor: "bg-info" },
  p3: { label: "P3", className: "bg-secondary text-muted-foreground border border-muted", dotColor: "bg-muted-foreground" },
};

const storyTypeConfig: Record<StoryType, { label: string; icon: string; className: string }> = {
  feature: { label: "功能", icon: "✨", className: "bg-primary/15 text-primary" },
  bugfix: { label: "缺陷", icon: "🐛", className: "bg-destructive/15 text-destructive" },
  refactor: { label: "重构", icon: "♻️", className: "bg-warning/15 text-warning" },
  docs: { label: "文档", icon: "📝", className: "bg-info/15 text-info" },
  test: { label: "测试", icon: "🧪", className: "bg-success/15 text-success" },
  other: { label: "其他", icon: "📦", className: "bg-secondary text-muted-foreground" },
};

const taskStatusConfig: Record<TaskStatus, { label: string; className: string }> = {
  pending: { label: "待执行", className: "bg-secondary text-muted-foreground border border-muted" },
  assigned: { label: "已分配", className: "bg-info/15 text-info" },
  running: { label: "执行中", className: "bg-primary/15 text-primary" },
  awaiting_verification: { label: "待验收", className: "bg-warning/15 text-warning" },
  completed: { label: "已完成", className: "bg-success/15 text-success" },
  failed: { label: "失败", className: "bg-destructive/15 text-destructive" },
};

interface BadgeProps {
  className?: string;
}

export function StoryStatusBadge({ status, className = "" }: BadgeProps & { status: StoryStatus }) {
  const config = storyStatusConfig[status];
  return (
    <span className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ${config.className} ${className}`}>
      {status === "running" && <span className="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-current" />}
      {config.label}
    </span>
  );
}

export function TaskStatusBadge({ status, className = "" }: BadgeProps & { status: TaskStatus }) {
  const config = taskStatusConfig[status];
  return (
    <span className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ${config.className} ${className}`}>
      {status === "running" && <span className="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-current" />}
      {config.label}
    </span>
  );
}

export function StoryPriorityBadge({ priority, showLabel = false, className = "" }: BadgeProps & { priority: StoryPriority; showLabel?: boolean }) {
  const config = storyPriorityConfig[priority];
  return (
    <span className={`inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] font-semibold ${config.className} ${className}`}>
      <span className={`inline-block h-1.5 w-1.5 rounded-full ${config.dotColor}`} />
      {showLabel && config.label}
    </span>
  );
}

export function StoryTypeBadge({ type, showIcon = true, className = "" }: BadgeProps & { type: StoryType; showIcon?: boolean }) {
  const config = storyTypeConfig[type];
  return (
    <span className={`inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] font-medium ${config.className} ${className}`}>
      {showIcon && <span className="text-[10px]">{config.icon}</span>}
      <span>{config.label}</span>
    </span>
  );
}

// 获取 Story 类型的配置信息
export function getStoryTypeInfo(type: StoryType) {
  return storyTypeConfig[type];
}

// 获取 Story 优先级的配置信息
export function getStoryPriorityInfo(priority: StoryPriority) {
  return storyPriorityConfig[priority];
}
