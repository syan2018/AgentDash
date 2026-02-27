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
      className="w-full rounded-lg border border-border bg-card p-3 text-left transition-colors hover:bg-secondary/30"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-foreground">{task.title}</p>
          {task.description && <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{task.description}</p>}
        </div>
        <TaskStatusBadge status={task.status} />
      </div>
      <div className="mt-2 flex items-center justify-between text-xs text-muted-foreground">
        <span>{agentLabel}</span>
        <span>{new Date(task.updated_at).toLocaleDateString("zh-CN")}</span>
      </div>
    </button>
  );
}
