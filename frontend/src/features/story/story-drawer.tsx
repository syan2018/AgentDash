import { useMemo, useState } from "react";
import type { Story, Task } from "../../types";
import { StoryStatusBadge } from "../../components/ui/status-badge";
import { TaskList } from "../task/task-list";

interface StoryDrawerProps {
  story: Story | null;
  tasks: Task[];
  onClose: () => void;
  onOpenTask: (task: Task) => void;
}

type DrawerTab = "context" | "tasks" | "review";

function ContextPanel({ story }: { story: Story }) {
  const items = story.context.items ?? [];
  return (
    <div className="space-y-3 p-6">
      <h4 className="text-sm font-medium text-foreground">上下文</h4>
      {items.length === 0 ? (
        <p className="rounded-md border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">
          暂无上下文条目
        </p>
      ) : (
        items.map((item) => (
          <div key={item.id} className="rounded-md border border-border bg-card p-3">
            <div className="mb-1 flex items-center gap-2">
              <span className="rounded bg-secondary px-2 py-0.5 text-[10px] uppercase text-muted-foreground">
                {item.sourceKind}
              </span>
              <p className="text-sm font-medium text-foreground">{item.displayName ?? item.reference}</p>
            </div>
            <p className="text-sm text-muted-foreground">{item.summary ?? item.reason}</p>
          </div>
        ))
      )}
    </div>
  );
}

function ReviewPanel({ story, tasks }: { story: Story; tasks: Task[] }) {
  const successCount = tasks.filter((task) => task.status === "succeeded").length;
  const failedCount = tasks.filter((task) => task.status === "failed").length;
  return (
    <div className="space-y-4 p-6">
      <h4 className="text-sm font-medium text-foreground">验收</h4>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">Story 状态</p>
          <p className="mt-1 text-sm font-medium text-foreground">{story.status}</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">成功任务数</p>
          <p className="mt-1 text-sm font-medium text-success">{successCount}</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">失败任务数</p>
          <p className="mt-1 text-sm font-medium text-destructive">{failedCount}</p>
        </div>
      </div>
      <p className="text-sm text-muted-foreground">{story.description || "暂无 Story 描述"}</p>
    </div>
  );
}

export function StoryDrawer({ story, tasks, onClose, onOpenTask }: StoryDrawerProps) {
  const [activeTab, setActiveTab] = useState<DrawerTab>("context");

  const sortedTasks = useMemo(
    () =>
      [...tasks].sort(
        (a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime(),
      ),
    [tasks],
  );

  if (!story) return null;

  return (
    <>
      <div className="fixed inset-0 z-20 bg-foreground/10 backdrop-blur-[1px]" onClick={onClose} />
      <aside className="fixed inset-y-0 right-0 z-30 flex w-full max-w-[80rem] flex-col border-l border-border bg-background shadow-xl">
        <header className="flex items-center justify-between border-b border-border px-6 py-4">
          <div className="min-w-0">
            <div className="mb-1">
              <StoryStatusBadge status={story.status} />
            </div>
            <h3 className="truncate text-lg font-semibold text-foreground">{story.title}</h3>
          </div>
          <button type="button" onClick={onClose} className="rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-secondary">
            关闭
          </button>
        </header>

        <div className="flex border-b border-border bg-card">
          {[
            { key: "context", label: "上下文" },
            { key: "tasks", label: "任务列表" },
            { key: "review", label: "验收" },
          ].map((tab) => (
            <button
              key={tab.key}
              type="button"
              onClick={() => setActiveTab(tab.key as DrawerTab)}
              className={`px-5 py-3 text-sm ${
                activeTab === tab.key
                  ? "border-b-2 border-primary text-primary"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {tab.label}
            </button>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto">
          {activeTab === "context" && <ContextPanel story={story} />}
          {activeTab === "tasks" && (
            <div className="p-6">
              <TaskList tasks={sortedTasks} onTaskClick={onOpenTask} />
            </div>
          )}
          {activeTab === "review" && <ReviewPanel story={story} tasks={sortedTasks} />}
        </div>
      </aside>
    </>
  );
}
