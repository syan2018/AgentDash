/**
 * SessionShortcutList — 侧栏会话快捷列表。
 *
 * 展示当前项目的活跃会话，以 session title 为主。
 * 点击跳转到 `/session/:id`，绝不暴露 lifecycle 技术概念。
 */

import { useEffect, useMemo } from "react";
import { useLocation, useMatch, useNavigate } from "react-router-dom";
import { StatusDot, type StatusDotTone } from "@agentdash/ui";
import { useLifecycleStore } from "../../stores/lifecycleStore";
import type { SessionExecutionStatusValue } from "../../services/session";
import type { LifecycleAgentView } from "../../types";

/** 基于 session 执行状态的视觉映射 */
const EXECUTION_STATUS_TONE: Record<SessionExecutionStatusValue, StatusDotTone> = {
  idle: "muted",
  running: "success",
  completed: "info",
  failed: "danger",
  interrupted: "warning",
};

const EXECUTION_STATUS_LABEL: Record<SessionExecutionStatusValue, string> = {
  idle: "就绪",
  running: "执行中",
  completed: "已完成",
  failed: "失败",
  interrupted: "已中断",
};

function executionStatusTone(status: SessionExecutionStatusValue): StatusDotTone {
  return EXECUTION_STATUS_TONE[status] ?? "muted";
}

function updatedAtTimestamp(value: string | number): number {
  if (typeof value === "number") return value;
  const timestamp = new Date(value).getTime();
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function formatUpdatedAt(value: string | number): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

interface SessionShortcutEntry {
  runtimeSessionId: string;
  sessionTitle: string;
  executionStatus: SessionExecutionStatusValue;
  updatedAt: string | number;
}

interface LifecycleShortcutListProps {
  projectId: string | null;
}

function selectPrimarySessionAgent(runAgents: LifecycleAgentView[]): LifecycleAgentView | null {
  return runAgents.find((agent) => agent.agent_role === "primary" && agent.delivery_runtime_ref)
    ?? runAgents.find((agent) => agent.delivery_runtime_ref)
    ?? runAgents.find((agent) => agent.agent_role === "primary")
    ?? runAgents[0]
    ?? null;
}

export function SessionShortcutList({ projectId }: LifecycleShortcutListProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const sessionRouteMatch = useMatch("/session/:sessionId");
  const lifecycleRuns = useLifecycleStore((s) => s.lifecycleRuns);
  const agents = useLifecycleStore((s) => s.agents);
  const sessionMetas = useLifecycleStore((s) => s.sessionMetas);
  const fetchProjectActiveAgents = useLifecycleStore((s) => s.fetchProjectActiveAgents);
  const error = useLifecycleStore((s) => s.error);

  useEffect(() => {
    if (!projectId) return;
    void fetchProjectActiveAgents(projectId);
    const timer = window.setInterval(() => {
      void fetchProjectActiveAgents(projectId);
    }, 30_000);
    return () => window.clearInterval(timer);
  }, [fetchProjectActiveAgents, projectId]);

  const sessionEntries = useMemo(() => {
    if (!projectId) return [];

    const entries: SessionShortcutEntry[] = [];

    for (const lifecycleRun of lifecycleRuns.values()) {
      if (lifecycleRun.project_id !== projectId) continue;

      const runAgents = Array.from(agents.values()).filter(
        (a) => a.agent_ref.run_id === lifecycleRun.run_ref.run_id,
      );

      const primaryAgent = selectPrimarySessionAgent(runAgents);
      if (!primaryAgent?.delivery_runtime_ref) continue;

      const deliveryRuntimeSessionId =
        primaryAgent.delivery_runtime_ref.runtime_session_id;

      const meta = sessionMetas.get(deliveryRuntimeSessionId);

      entries.push({
        runtimeSessionId: deliveryRuntimeSessionId,
        sessionTitle: meta?.title?.trim() || "会话加载中…",
        executionStatus: (meta?.lastExecutionStatus
          ?? primaryAgent.last_execution_status
          ?? "idle") as SessionExecutionStatusValue,
        updatedAt: meta?.updatedAt ?? primaryAgent.updated_at ?? lifecycleRun.last_activity_at,
      });
    }

    entries.sort((a, b) => updatedAtTimestamp(b.updatedAt) - updatedAtTimestamp(a.updatedAt));
    return entries;
  }, [projectId, lifecycleRuns, agents, sessionMetas]);

  const activeSessionId = sessionRouteMatch?.params.sessionId ?? null;

  return (
    <div className="flex min-h-0 flex-1 flex-col border-b border-border">
      <div className="flex shrink-0 items-center justify-between px-4 pb-1.5 pt-3">
        <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
          会话
        </span>
        {sessionEntries.length > 0 && (
          <span className="text-[10px] text-muted-foreground/70">
            {sessionEntries.length}
          </span>
        )}
      </div>

      {!projectId ? (
        <p className="px-4 pb-3 text-xs text-muted-foreground">未选择项目</p>
      ) : sessionEntries.length === 0 ? (
        <div className="px-4 pb-3">
          <p className="text-xs text-muted-foreground">暂无活跃会话</p>
          {error && <p className="mt-1 line-clamp-2 text-[11px] text-destructive">{error}</p>}
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-2">
          {sessionEntries.map((entry) => {
            const isActive = activeSessionId === entry.runtimeSessionId;
            return (
              <button
                key={entry.runtimeSessionId}
                type="button"
                onClick={() => {
                  if (location.pathname !== `/session/${entry.runtimeSessionId}`) {
                    navigate(`/session/${entry.runtimeSessionId}`);
                  }
                }}
                className={`mb-0.5 flex w-full flex-col gap-0.5 rounded-[8px] px-2.5 py-2 text-left transition-colors ${
                  isActive ? "bg-primary/10" : "hover:bg-secondary/50"
                }`}
              >
                <div className="flex items-center gap-2">
                  <StatusDot
                    tone={executionStatusTone(entry.executionStatus)}
                    size="sm"
                    pulse={entry.executionStatus === "running"}
                    className="shrink-0"
                    title={EXECUTION_STATUS_LABEL[entry.executionStatus] ?? entry.executionStatus}
                  />
                  <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">
                    {entry.sessionTitle}
                  </span>
                  <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                    {formatUpdatedAt(entry.updatedAt)}
                  </span>
                </div>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
