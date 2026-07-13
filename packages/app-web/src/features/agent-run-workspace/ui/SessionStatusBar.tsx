/**
 * AgentRun 状态栏
 *
 * 合并「Task 进度」与「mailbox（pending/steering）」为输入栏上方的单一可折叠栏：
 * - 折叠态：当前待办项（active 优先）+ 整体进度 done/total，外加 pending/暂停徽标。
 * - 展开态：完整 Task 清单（点击打开 TaskDrawer）+ mailbox 分区（保留既有操作）。
 *
 * Task 数据源为 LifecycleRun.tasks（taskPlanStore，同后端 task_read/task_write 写入源），
 * 不引入第二套事实源。无 Task 且无 mailbox 内容时不渲染。
 */

import { useEffect, useMemo, useState } from "react";

import type { MailboxMessageView } from "../../../generated/agent-run-mailbox-contracts";
import type { Task, TaskPlanStatus } from "../../../types";
import { TaskStatusToken } from "../../../components/ui/status-badge";
import { useTaskPlanStore } from "../../../stores/taskPlanStore";
import { TaskDrawer } from "../../task/task-drawer";
import type { AgentRunChatMailboxModel } from "../model/conversationCommandState";
import { MailboxSections } from "./MailboxMessageRow";
import { mailboxHasContent } from "./mailboxContent";

interface SessionStatusBarProps {
  /** Task scope；缺省时只展示 mailbox（兼容非 AgentRun 场景） */
  runId?: string | null;
  agentId?: string | null;

  // mailbox passthrough
  messages: MailboxMessageView[];
  mailbox?: AgentRunChatMailboxModel;
  onPromote: (messageId: string) => void;
  onDelete: (messageId: string) => void;
  onResume?: () => void;
  onRecall?: (messageId: string) => void;
  onMove?: (messageId: string, afterMessageId: string | null) => void;
}

// 折叠态「当前待办」选取优先级
const CURRENT_STATUS_ORDER: TaskPlanStatus[] = ["active", "review", "blocked", "open"];

export function SessionStatusBar(props: SessionStatusBarProps) {
  const { runId, agentId } = props;
  const { taskPlansByRunId, fetchAgentRunTasks } = useTaskPlanStore();
  const [expanded, setExpanded] = useState(false);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);

  useEffect(() => {
    if (!runId || !agentId) return;
    void fetchAgentRunTasks(runId, agentId);
  }, [runId, agentId, fetchAgentRunTasks]);

  const tasks = useMemo<Task[]>(() => {
    if (!runId) return [];
    const plan = taskPlansByRunId[runId];
    return (plan?.tasks ?? []).filter((task) => !task.archived_at);
  }, [runId, taskPlansByRunId]);

  const total = tasks.length;
  const done = tasks.filter((task) => task.status === "done").length;
  const currentTask = useMemo(() => {
    for (const status of CURRENT_STATUS_ORDER) {
      const hit = tasks.find((task) => task.status === status);
      if (hit) return hit;
    }
    return tasks[0] ?? null;
  }, [tasks]);

  const hasMailbox = mailboxHasContent(props.messages, props.mailbox);
  const hasTasks = total > 0;

  const selectedTask = useMemo(
    () => tasks.find((task) => task.id === selectedTaskId) ?? null,
    [selectedTaskId, tasks],
  );

  if (!hasTasks && !hasMailbox) return null;

  const pendingCount = props.messages.length;
  const waitingItems = props.mailbox?.waiting_items ?? [];
  const waitingCount = waitingItems.length;
  const currentWait = waitingItems[0] ?? null;
  const paused = Boolean(props.mailbox?.paused);

  return (
    <div className="shrink-0 pb-2">
      <div className="mx-auto w-full max-w-4xl px-5">
        <div className="relative rounded-[12px] border border-border/60 bg-background shadow-sm">
          {/* 折叠态头部 */}
          <button
            type="button"
            onClick={() => setExpanded((value) => !value)}
            className="flex w-full items-center gap-2 px-3 py-2 text-left"
          >
            <ChevronIcon expanded={expanded} />
            {hasTasks ? (
              <>
                <span className="shrink-0 tabular-nums text-xs font-medium text-foreground">
                  {done}/{total}
                </span>
                {currentTask && (
                  <>
                    <TaskStatusToken status={currentTask.status} className="shrink-0" />
                    <span className="min-w-0 flex-1 truncate text-[13px] text-foreground/80">
                      {currentTask.title}
                    </span>
                  </>
                )}
              </>
            ) : (
              <>
                <span className="min-w-0 flex-1 truncate text-[13px] text-muted-foreground">
                  {currentWait
                    ? `${currentWait.source_label ?? currentWait.kind}: ${currentWait.preview ?? "等待外部事件"}`
                    : "AgentRun 消息"}
                </span>
              </>
            )}
            {paused && (
              <span className="shrink-0 rounded-[6px] bg-warning/10 px-1.5 py-0.5 text-[10px] font-medium text-warning">
                已暂停
              </span>
            )}
            {waitingCount > 0 && (
              <span className="shrink-0 rounded-[6px] bg-info/10 px-1.5 py-0.5 text-[10px] font-medium text-info">
                {waitingCount} 个等待
              </span>
            )}
            {hasMailbox && pendingCount > 0 && (
              <span className="shrink-0 rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {pendingCount} 条消息
              </span>
            )}
          </button>

          {/* 展开态 */}
          {expanded && (
            <div className="border-t border-border/50">
              {hasTasks && (
                <div className="max-h-56 space-y-1 overflow-y-auto p-2">
                  {tasks.map((task) => (
                    <button
                      key={task.id}
                      type="button"
                      onClick={() => setSelectedTaskId(task.id)}
                      className="flex w-full min-w-0 items-center gap-2 rounded-[8px] px-2 py-1.5 text-left transition-colors hover:bg-secondary/50"
                    >
                      <TaskStatusToken status={task.status} className="shrink-0" />
                      <span className="min-w-0 flex-1 truncate text-[13px] text-foreground/90">
                        {task.title}
                      </span>
                      {task.priority && (
                        <span className="shrink-0 rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                          {task.priority}
                        </span>
                      )}
                    </button>
                  ))}
                </div>
              )}
              {hasMailbox && (
                <div className={hasTasks ? "border-t border-border/40 pb-1" : "pb-1"}>
                  <MailboxSections {...props} />
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      <TaskDrawer
        key={selectedTaskId ?? "no-task-selected"}
        task={selectedTask}
        onTaskUpdated={(task) => setSelectedTaskId(task.id)}
        onTaskDeleted={(taskId) => {
          if (selectedTaskId === taskId) setSelectedTaskId(null);
        }}
        onClose={() => setSelectedTaskId(null)}
      />
    </div>
  );
}

function ChevronIcon({ expanded }: { expanded: boolean }) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 12 12"
      fill="none"
      className={`shrink-0 text-muted-foreground/50 transition-transform ${expanded ? "rotate-90" : ""}`}
    >
      <path d="M4.5 2.5L8 6l-3.5 3.5" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

