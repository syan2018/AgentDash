import type { Artifact, Task } from "../../types";
import { TaskStatusBadge } from "../../components/ui/status-badge";

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
        <pre className="whitespace-pre-wrap text-xs text-foreground">
          {artifact.blocks.map((b) => ("text" in b ? b.text : JSON.stringify(b))).join("\n")}
        </pre>
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

export function TaskDrawer({ task, onClose }: TaskDrawerProps) {
  if (!task) return null;

  const agentLabel = task.agent_binding?.agent_type ?? "未指定 Agent";

  return (
    <>
      <div className="fixed inset-0 z-30 bg-foreground/15 backdrop-blur-[1px]" onClick={onClose} />
      <aside className="fixed inset-y-0 right-0 z-40 flex w-full max-w-[52rem] flex-col border-l border-border bg-background shadow-xl">
        <header className="flex items-center justify-between border-b border-border px-6 py-4">
          <div className="min-w-0">
            <div className="mb-1 flex items-center gap-2">
              <TaskStatusBadge status={task.status} />
              <span className="text-xs text-muted-foreground">{agentLabel}</span>
            </div>
            <h3 className="truncate text-base font-semibold text-foreground">{task.title}</h3>
            {task.description && <p className="mt-1 text-sm text-muted-foreground">{task.description}</p>}
          </div>
          <button type="button" onClick={onClose} className="rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-secondary">
            关闭
          </button>
        </header>

        <div className="flex-1 overflow-y-auto p-6">
          <h4 className="mb-3 text-sm font-medium text-foreground">任务详情</h4>

          <div className="mb-6 grid grid-cols-2 gap-3">
            <div className="rounded-md border border-border bg-card p-3">
              <p className="text-xs text-muted-foreground">工作空间 ID</p>
              <p className="mt-1 truncate text-sm font-mono text-foreground">{task.workspace_id ?? "未绑定"}</p>
            </div>
            <div className="rounded-md border border-border bg-card p-3">
              <p className="text-xs text-muted-foreground">Agent 预设</p>
              <p className="mt-1 text-sm text-foreground">{task.agent_binding?.preset_name ?? "无"}</p>
            </div>
          </div>

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
        </div>
      </aside>
    </>
  );
}
