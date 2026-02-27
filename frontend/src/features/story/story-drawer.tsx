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
  const ctx = story.context;
  const hasContent = ctx.prd_doc || ctx.spec_refs.length > 0 || ctx.resource_list.length > 0;

  return (
    <div className="space-y-4 p-6">
      <h4 className="text-sm font-medium text-foreground">上下文</h4>

      {!hasContent ? (
        <p className="rounded-md border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">
          暂无上下文条目
        </p>
      ) : (
        <>
          {ctx.prd_doc && (
            <div className="rounded-md border border-border bg-card p-3">
              <p className="mb-1 text-xs font-medium text-muted-foreground">PRD 文档</p>
              <pre className="whitespace-pre-wrap text-sm text-foreground">{ctx.prd_doc}</pre>
            </div>
          )}

          {ctx.spec_refs.length > 0 && (
            <div className="rounded-md border border-border bg-card p-3">
              <p className="mb-2 text-xs font-medium text-muted-foreground">规格引用</p>
              <ul className="space-y-1">
                {ctx.spec_refs.map((ref, i) => (
                  <li key={i} className="text-sm text-foreground">
                    <span className="mr-2 text-muted-foreground">·</span>{ref}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {ctx.resource_list.length > 0 && (
            <div className="rounded-md border border-border bg-card p-3">
              <p className="mb-2 text-xs font-medium text-muted-foreground">资源列表</p>
              {ctx.resource_list.map((res, i) => (
                <div key={i} className="mb-1 flex items-center gap-2">
                  <span className="rounded bg-secondary px-2 py-0.5 text-[10px] uppercase text-muted-foreground">
                    {res.resource_type}
                  </span>
                  <span className="text-sm text-foreground">{res.name}</span>
                  <span className="text-xs text-muted-foreground">{res.uri}</span>
                </div>
              ))}
            </div>
          )}
        </>
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
        (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
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
