import type { Task } from "../../types";
import { TaskStatusBadge } from "../../components/ui/status-badge";

interface TaskCardProps {
  task: Task;
  onClick?: () => void;
}

export function TaskCard({ task, onClick }: TaskCardProps) {
  const agentLabel = task.agent_binding?.agent_type ?? "未指定";

  return (
    <button
      type="button"
      onClick={onClick}
      className="w-full rounded-[12px] border border-border bg-background p-3.5 text-left transition-all hover:border-primary/25 hover:bg-secondary/35"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium leading-6 text-foreground">{task.title}</p>
          {task.description && <p className="mt-1.5 line-clamp-2 text-xs leading-5 text-muted-foreground">{task.description}</p>}
        </div>
        <TaskStatusBadge status={task.status} />
      </div>
      <div className="mt-3 flex items-center justify-between border-t border-border/70 pt-2.5 text-xs text-muted-foreground">
        <span className="truncate">{agentLabel}</span>
        <span>{new Date(task.updated_at).toLocaleDateString("zh-CN")}</span>
      </div>
    </button>
  );
}
