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
import type { LifecycleAgentView, LifecycleRunView } from "../../types";

const AGENT_STATUS_TONE: Record<string, StatusDotTone> = {
  active: "success",
  running: "success",
  ready: "info",
  completed: "info",
  failed: "danger",
  paused: "warning",
  pending: "muted",
  cancelled: "warning",
};

const STATUS_LABEL: Record<string, string> = {
  active: "就绪",
  running: "运行中",
  completed: "已完成",
  failed: "失败",
  paused: "已暂停",
  pending: "待启动",
  cancelled: "已取消",
};

function agentStatusTone(status: string): StatusDotTone {
  return AGENT_STATUS_TONE[status] ?? "muted";
}

function formatUpdatedAt(value: string): string {
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
  agentStatus: string;
  agentRole: string;
  updatedAt: string;
  runId: string;
}

interface LifecycleShortcutListProps {
  projectId: string | null;
}

export function SessionShortcutList({ projectId }: LifecycleShortcutListProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const sessionRouteMatch = useMatch("/session/:sessionId");
  const runs = useLifecycleStore((s) => s.runs);
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

    for (const run of runs.values()) {
      if (run.project_id !== projectId) continue;

      const runAgents = Array.from(agents.values()).filter(
        (a) => a.agent_ref.run_id === run.run_ref.run_id,
      );

      const primarySessionId = run.runtime_trace_refs[0]?.runtime_session_id;
      if (!primarySessionId) continue;

      const meta = sessionMetas.get(primarySessionId);
      const primaryAgent = runAgents[0];

      entries.push({
        runtimeSessionId: primarySessionId,
        sessionTitle: meta?.title?.trim() || primaryAgent?.agent_role || primaryAgent?.agent_kind || "会话",
        agentStatus: primaryAgent?.status ?? "pending",
        agentRole: primaryAgent?.agent_role || primaryAgent?.agent_kind || "",
        updatedAt: primaryAgent?.updated_at ?? run.last_activity_at,
        runId: run.run_ref.run_id,
      });
    }

    entries.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
    return entries;
  }, [projectId, runs, agents, sessionMetas]);

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
                    tone={agentStatusTone(entry.agentStatus)}
                    size="sm"
                    pulse={entry.agentStatus === "active" || entry.agentStatus === "running"}
                    className="shrink-0"
                    title={STATUS_LABEL[entry.agentStatus] ?? entry.agentStatus}
                  />
                  <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">
                    {entry.sessionTitle}
                  </span>
                  <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                    {formatUpdatedAt(entry.updatedAt)}
                  </span>
                </div>
                {entry.agentRole && entry.sessionTitle !== entry.agentRole && (
                  <p className="ml-3.5 truncate text-[11px] leading-[1.35] text-muted-foreground">
                    {entry.agentRole}
                  </p>
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
