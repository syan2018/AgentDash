/**
 * task_read / task_write 工具卡片 body
 *
 * 后端 `result_with_view` 把 Task view JSON 写进工具结果 content（`"{summary}\n{json}"`），
 * 这里从 contentItems 的 text block 解析出该 JSON 并定制渲染：
 * - task_write：展示本次变更清单（changes[]，含 status 迁移）。
 * - task_read：按 mode 展示 overview 进度 / list 列表。
 * 解析失败时回退通用 JSON body，不阻塞渲染。
 */

import { useMemo } from "react";
import type { ThreadItem } from "../../../../generated/backbone-protocol";
import type { TaskStatus } from "../../../../types";
import { TaskStatusToken } from "../../../../components/ui/status-badge";
import { GenericJsonBody } from "./GenericJsonBody";
import { normalizeDynamicOutput } from "./toolOutputContent";
import { CB } from "./cardBodyTokens";

type DynamicItem = Extract<ThreadItem, { type: "dynamicToolCall" }>;

const CHANGE_KIND_LABEL: Record<string, string> = {
  created: "新建",
  updated: "更新",
  status_changed: "状态推进",
  reordered: "排序",
  dropped: "归档",
  context_refs_replaced: "更新引用",
};

interface TaskChange {
  task_id: string;
  title: string;
  change_kind: string;
  status_from?: TaskStatus | null;
  status_to?: TaskStatus | null;
}

interface CompactTask {
  id: string;
  title: string;
  status: TaskStatus;
  priority?: string | null;
  assigned_agent_id?: string | null;
}

interface TaskView {
  mode?: string;
  counts?: Record<string, number>;
  total?: number;
  done?: number;
  current_items?: CompactTask[];
  tasks?: CompactTask[];
  changes?: TaskChange[];
}

export function TaskToolCardBody({ item }: { item: DynamicItem }) {
  const view = useMemo(() => parseTaskView(item.contentItems), [item.contentItems]);

  if (!view) {
    return <GenericJsonBody arguments={item.arguments} contentItems={item.contentItems} />;
  }

  const tool = item.tool.toLowerCase();

  if (tool === "task_write") {
    return <WriteChanges changes={view.changes ?? []} item={item} />;
  }

  // task_read（及其它）：overview 优先，其次列表
  if (view.mode === "overview") {
    return <Overview view={view} />;
  }
  const tasks = view.tasks ?? view.current_items ?? [];
  if (tasks.length > 0) {
    return <TaskRows tasks={tasks} />;
  }
  return <GenericJsonBody arguments={item.arguments} contentItems={item.contentItems} />;
}

function WriteChanges({ changes, item }: { changes: TaskChange[]; item: DynamicItem }) {
  if (changes.length === 0) {
    return <GenericJsonBody arguments={item.arguments} contentItems={item.contentItems} />;
  }
  return (
    <div className={CB.itemGap}>
      {changes.map((change, index) => (
        <div
          key={`${change.task_id}-${index}`}
          className={`flex min-w-0 items-center gap-2 ${CB.inlineEntryButton}`}
        >
          <span className={CB.kindBadge}>
            {CHANGE_KIND_LABEL[change.change_kind] ?? change.change_kind}
          </span>
          <span className="min-w-0 flex-1 truncate text-xs text-foreground/80">{change.title}</span>
          {change.change_kind === "status_changed" && change.status_to && (
            <span className={`flex shrink-0 items-center gap-1 ${CB.meta}`}>
              {change.status_from && <TaskStatusToken status={change.status_from} />}
              <span aria-hidden>→</span>
              <TaskStatusToken status={change.status_to} />
            </span>
          )}
        </div>
      ))}
    </div>
  );
}

function Overview({ view }: { view: TaskView }) {
  const total = view.total ?? 0;
  const done = view.done ?? view.counts?.done ?? 0;
  const active = view.current_items ?? [];
  const countEntries = view.counts ? Object.entries(view.counts).filter(([, c]) => c > 0) : [];

  if (total === 0 && active.length === 0) {
    return <p className={CB.meta}>暂无任务</p>;
  }

  return (
    <div className={CB.sectionGap}>
      <div className={`flex items-center gap-2 ${CB.meta}`}>
        <span className="text-foreground/70">
          {done}/{total}
        </span>
        {countEntries.map(([status, count]) => (
          <span key={status} className={CB.kindBadge}>
            {status} {count}
          </span>
        ))}
      </div>
      {active.length > 0 && <TaskRows tasks={active} />}
    </div>
  );
}

function TaskRows({ tasks }: { tasks: CompactTask[] }) {
  return (
    <div className={CB.itemGap}>
      {tasks.map((task) => (
        <div
          key={task.id}
          className={`flex min-w-0 items-center gap-2 ${CB.inlineEntryButton}`}
        >
          <TaskStatusToken status={task.status} className="shrink-0" />
          <span className="min-w-0 flex-1 truncate text-xs text-foreground/80">{task.title}</span>
          {task.priority && (
            <span className={CB.kindBadge}>
              {task.priority}
            </span>
          )}
        </div>
      ))}
    </div>
  );
}

/** 从工具结果 contentItems 解析出 Task view JSON。content 形如 `"{summary}\n{json}"`。 */
function parseTaskView(contentItems: unknown): TaskView | null {
  if (!Array.isArray(contentItems)) return null;
  const blocks = normalizeDynamicOutput(contentItems as never);
  const text = blocks
    .filter((block) => block.kind === "text")
    .map((block) => (block.kind === "text" ? block.text : ""))
    .join("\n");
  if (!text) return null;
  // content 首行是人类可读 summary，JSON 从第一个 `{` 开始。
  const start = text.indexOf("{");
  if (start < 0) return null;
  try {
    const parsed = JSON.parse(text.slice(start)) as unknown;
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed as TaskView;
    }
    return null;
  } catch {
    return null;
  }
}
