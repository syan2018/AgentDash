import type { Task } from "../../types";
import { TaskCard } from "./task-card";

interface TaskListProps {
  tasks: Task[];
  onTaskClick: (task: Task) => void;
}

export function TaskList({ tasks, onTaskClick }: TaskListProps) {
  if (tasks.length === 0) {
    return (
      <div className="rounded-[12px] border border-dashed border-border bg-secondary/25 py-10 text-center text-sm text-muted-foreground">
        当前 Story 暂无 Task
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {tasks.map((task) => (
        <TaskCard key={task.id} task={task} onClick={() => onTaskClick(task)} />
      ))}
    </div>
  );
}
