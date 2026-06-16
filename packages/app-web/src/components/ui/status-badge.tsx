import type { StoryStatus, TaskStatus, StoryPriority, StoryType } from "../../types";

const storyStatusConfig: Record<StoryStatus, { label: string; className: string; progress: number }> = {
  created: { label: "created", className: "text-muted-foreground", progress: 0 },
  context_ready: { label: "context_ready", className: "text-info", progress: 0.2 },
  executing: { label: "executing", className: "text-primary", progress: 0.55 },
  decomposed: { label: "decomposed", className: "text-warning", progress: 0.75 },
  completed: { label: "completed", className: "text-success", progress: 1 },
  failed: { label: "failed", className: "text-destructive", progress: 0 },
  cancelled: { label: "cancelled", className: "text-muted-foreground", progress: 0 },
};

const storyPriorityConfig: Record<StoryPriority, { label: string; className: string; tooltip: string }> = {
  p0: { label: "P0", className: "border-destructive/20 bg-destructive/10 text-destructive", tooltip: "P0 · 紧急" },
  p1: { label: "P1", className: "border-warning/20 bg-warning/10 text-warning", tooltip: "P1 · 高" },
  p2: { label: "P2", className: "border-primary/20 bg-primary/10 text-primary", tooltip: "P2 · 中" },
  p3: { label: "P3", className: "border-border bg-secondary text-muted-foreground", tooltip: "P3 · 低" },
};

const storyTypeConfig: Record<StoryType, { label: string; icon: string; className: string; tooltip: string }> = {
  feature: { label: "feature", icon: "FEAT", className: "border-primary/20 bg-primary/10 text-primary", tooltip: "FEAT · 功能" },
  bugfix: { label: "bugfix", icon: "BUG", className: "border-destructive/20 bg-destructive/10 text-destructive", tooltip: "BUG · 缺陷修复" },
  refactor: { label: "refactor", icon: "REF", className: "border-warning/20 bg-warning/10 text-warning", tooltip: "REF · 重构" },
  docs: { label: "docs", icon: "DOC", className: "border-info/20 bg-info/10 text-info", tooltip: "DOC · 文档" },
  test: { label: "test", icon: "TEST", className: "border-success/20 bg-success/10 text-success", tooltip: "TEST · 测试" },
  other: { label: "other", icon: "OTHR", className: "border-border bg-secondary text-muted-foreground", tooltip: "OTHR · 其他" },
};

const taskStatusConfig: Record<TaskStatus, { label: string; className: string }> = {
  open: { label: "open", className: "border-border bg-secondary text-muted-foreground" },
  active: { label: "active", className: "border-primary/20 bg-primary/10 text-primary" },
  review: { label: "review", className: "border-warning/20 bg-warning/10 text-warning" },
  blocked: { label: "blocked", className: "border-destructive/20 bg-destructive/10 text-destructive" },
  done: { label: "done", className: "border-success/20 bg-success/10 text-success" },
  dropped: { label: "dropped", className: "border-border bg-secondary text-muted-foreground" },
};

interface BadgeProps {
  className?: string;
}

function storyStatusPath(progress: number): string {
  const center = 7;
  const radius = 3.5;
  const angle = 2 * Math.PI * progress;
  const endX = center + radius * Math.sin(angle);
  const endY = center - radius * Math.cos(angle);
  const largeArc = progress > 0.5 ? 1 : 0;
  return `M${center},${center} L${center},${center - radius} A${radius},${radius} 0 ${largeArc},1 ${endX},${endY} Z`;
}

export function StoryStatusIcon({ status, className = "h-3.5 w-3.5" }: BadgeProps & { status: StoryStatus }) {
  const config = storyStatusConfig[status];

  return (
    <svg viewBox="0 0 14 14" fill="none" className={`${className} shrink-0 ${config.className}`} aria-hidden="true">
      <circle cx="7" cy="7" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" />
      {config.progress === 1 ? (
        <>
          <circle cx="7" cy="7" r="6" fill="currentColor" />
          <path d="M10.8 4.6 6.1 9.3 3.6 6.8" fill="none" stroke="white" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.5" />
        </>
      ) : config.progress > 0 ? (
        <path d={storyStatusPath(config.progress)} fill="currentColor" />
      ) : null}
      {status === "failed" && <path d="M4.8 4.8 9.2 9.2 M9.2 4.8 4.8 9.2" stroke="currentColor" strokeLinecap="round" strokeWidth="1.4" />}
      {status === "cancelled" && <path d="M4.5 9.5 9.5 4.5" stroke="currentColor" strokeLinecap="round" strokeWidth="1.5" />}
    </svg>
  );
}

export function StoryStatusToken({ status, count, className = "" }: BadgeProps & { status: StoryStatus; count?: number }) {
  const config = storyStatusConfig[status];

  return (
    <span className={`inline-flex min-w-0 items-center gap-1.5 text-xs font-semibold ${className}`}>
      <StoryStatusIcon status={status} className="h-3 w-3" />
      <span className={`truncate font-mono text-[11px] ${config.className}`}>{config.label}</span>
      {typeof count === "number" && (
        <span className="font-mono text-[11px] font-medium text-muted-foreground">{count}</span>
      )}
    </span>
  );
}

export function StoryStatusBadge({ status, className = "" }: BadgeProps & { status: StoryStatus }) {
  const config = storyStatusConfig[status];
  return (
    <span className={`inline-flex items-center gap-1.5 rounded-[6px] bg-secondary px-1.5 py-0.5 text-[11px] font-medium ${config.className} ${className}`}>
      <StoryStatusIcon status={status} className="h-3 w-3" />
      {config.label}
    </span>
  );
}

export function TaskStatusBadge({ status, className = "" }: BadgeProps & { status: TaskStatus }) {
  const config = taskStatusConfig[status];
  return (
    <span className={`inline-flex items-center gap-1.5 rounded-[8px] border px-2.5 py-1 text-xs font-medium ${config.className} ${className}`}>
      {config.label}
    </span>
  );
}

export function StoryPriorityBadge({ priority, showLabel = true, className = "" }: BadgeProps & { priority: StoryPriority; showLabel?: boolean }) {
  const config = storyPriorityConfig[priority];
  return (
    <span title={config.tooltip} className={`inline-flex h-5 items-center rounded-[6px] border px-1.5 font-mono text-[10px] font-semibold ${config.className} ${className}`}>
      {showLabel ? config.label : null}
    </span>
  );
}

export function StoryPriorityToken(props: BadgeProps & { priority: StoryPriority }) {
  return <StoryPriorityBadge {...props} showLabel />;
}

export function StoryTypeBadge({ type, showIcon = true, className = "" }: BadgeProps & { type: StoryType; showIcon?: boolean }) {
  const config = storyTypeConfig[type];
  return (
    <span title={config.tooltip} className={`inline-flex h-5 items-center rounded-[6px] border px-1.5 font-mono text-[10px] font-semibold ${config.className} ${className}`}>
      {showIcon ? config.icon : config.label}
    </span>
  );
}

export function StoryTypeToken(props: BadgeProps & { type: StoryType }) {
  return <StoryTypeBadge {...props} />;
}
