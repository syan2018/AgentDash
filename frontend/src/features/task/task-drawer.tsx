import type { Artifact, SessionUpdate, Task } from "../../types";
import { TaskStatusBadge } from "../../components/ui/status-badge";
import { ContentBlockList } from "../../components/acp/content-block";
import { ToolCallView } from "../../components/acp/tool-call";
import { PlanView } from "../../components/acp/plan";
import { ConfirmationRequestCard } from "../../components/acp/confirmation-request";

interface TaskDrawerProps {
  task: Task | null;
  onClose: () => void;
}

function ArtifactBlock({ artifact }: { artifact: Artifact }) {
  if (artifact.type === "text") {
    return (
      <div className="rounded-md border border-border bg-card p-3">
        <p className="mb-2 text-xs font-medium text-muted-foreground">{artifact.title ?? "文本产物"}</p>
        <pre className="whitespace-pre-wrap text-xs leading-relaxed text-foreground">{artifact.content}</pre>
      </div>
    );
  }

  if (artifact.type === "content_block") {
    return (
      <div className="rounded-md border border-border bg-card p-3">
        <p className="mb-2 text-xs font-medium text-muted-foreground">{artifact.title ?? "内容块产物"}</p>
        <ContentBlockList blocks={artifact.blocks} />
      </div>
    );
  }

  return (
    <div className="rounded-md border border-border bg-card p-3">
      <p className="mb-2 text-xs font-medium text-muted-foreground">{artifact.title ?? "JSON 产物"}</p>
      <pre className="overflow-auto text-xs leading-relaxed text-foreground">{JSON.stringify(artifact.data, null, 2)}</pre>
    </div>
  );
}

function SessionUpdateBlock({ update }: { update: SessionUpdate }) {
  if (update.type === "content") {
    return (
      <div className="rounded-md border border-border bg-card p-3">
        <p className="mb-2 text-xs text-muted-foreground">内容输出</p>
        <ContentBlockList blocks={update.blocks} />
      </div>
    );
  }
  if (update.type === "tool_call") {
    return <ToolCallView toolCall={update.toolCall} />;
  }
  if (update.type === "plan") {
    return <PlanView entries={update.entries} />;
  }
  return <ConfirmationRequestCard request={update.request} />;
}

export function TaskDrawer({ task, onClose }: TaskDrawerProps) {
  if (!task) return null;

  return (
    <>
      <div className="fixed inset-0 z-30 bg-foreground/15 backdrop-blur-[1px]" onClick={onClose} />
      <aside className="fixed inset-y-0 right-0 z-40 flex w-full max-w-[52rem] flex-col border-l border-border bg-background shadow-xl">
        <header className="flex items-center justify-between border-b border-border px-6 py-4">
          <div className="min-w-0">
            <div className="mb-1 flex items-center gap-2">
              <TaskStatusBadge status={task.status} />
              <span className="text-xs text-muted-foreground">{task.agentType}</span>
            </div>
            <h3 className="truncate text-base font-semibold text-foreground">{task.title}</h3>
            {task.description && <p className="mt-1 text-sm text-muted-foreground">{task.description}</p>}
          </div>
          <button type="button" onClick={onClose} className="rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-secondary">
            关闭
          </button>
        </header>

        <div className="grid flex-1 grid-cols-1 gap-0 overflow-hidden md:grid-cols-2">
          <section className="overflow-y-auto border-b border-border p-4 md:border-b-0 md:border-r">
            <h4 className="mb-3 text-sm font-medium text-foreground">执行日志</h4>
            {task.executionTrace.length === 0 ? (
              <p className="rounded-md border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">
                暂无执行日志
              </p>
            ) : (
              <div className="space-y-2">
                {task.executionTrace.map((update, index) => (
                  <SessionUpdateBlock key={index} update={update} />
                ))}
              </div>
            )}
          </section>

          <section className="overflow-y-auto p-4">
            <h4 className="mb-3 text-sm font-medium text-foreground">执行产物</h4>
            {task.artifacts.length === 0 ? (
              <p className="rounded-md border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">
                暂无执行产物
              </p>
            ) : (
              <div className="space-y-2">
                {task.artifacts.map((artifact, index) => (
                  <ArtifactBlock key={index} artifact={artifact} />
                ))}
              </div>
            )}
          </section>
        </div>
      </aside>
    </>
  );
}
