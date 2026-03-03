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
  p0: { label: "P0", className: "bg-red-50 text-red-600 border border-red-100", dotColor: "bg-red-500" },
  p1: { label: "P1", className: "bg-orange-50 text-orange-600 border border-orange-100", dotColor: "bg-orange-500" },
  p2: { label: "P2", className: "bg-blue-50 text-blue-600 border border-blue-100", dotColor: "bg-blue-500" },
  p3: { label: "P3", className: "bg-gray-50 text-gray-500 border border-gray-200", dotColor: "bg-gray-400" },
};

const storyTypeConfig: Record<StoryType, { label: string; icon: string; className: string }> = {
  feature: { label: "功能", icon: "✨", className: "bg-violet-50 text-violet-600" },
  bugfix: { label: "缺陷", icon: "🐛", className: "bg-rose-50 text-rose-600" },
  refactor: { label: "重构", icon: "♻️", className: "bg-amber-50 text-amber-600" },
  docs: { label: "文档", icon: "📝", className: "bg-sky-50 text-sky-600" },
  test: { label: "测试", icon: "🧪", className: "bg-emerald-50 text-emerald-600" },
  other: { label: "其他", icon: "📦", className: "bg-slate-50 text-slate-500" },
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
