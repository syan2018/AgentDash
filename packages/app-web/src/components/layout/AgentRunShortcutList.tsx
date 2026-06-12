/**
 * AgentRunShortcutList — 侧栏 AgentRun 快捷列表。
 *
 * 展示当前项目的活跃 AgentRun，以 workspace shell title 为主。
 * 点击跳转到 `/agent-runs/:runId/:agentId`。
 */

import { useEffect, useMemo, useState } from "react";
import { useLocation, useMatch, useNavigate } from "react-router-dom";
import { StatusDot, type StatusDotTone } from "@agentdash/ui";
import type { SessionExecutionStatusValue } from "../../services/session";
import { fetchProjectAgentRuns } from "../../services/lifecycle";
import type { AgentRunWorkspaceListEntry } from "../../types";

/** 基于 delivery 执行状态的视觉映射 */
const EXECUTION_STATUS_TONE: Record<SessionExecutionStatusValue, StatusDotTone> = {
  idle: "muted",
  running: "success",
  cancelling: "warning",
  completed: "info",
  failed: "danger",
  interrupted: "warning",
};

const EXECUTION_STATUS_LABEL: Record<SessionExecutionStatusValue, string> = {
  idle: "就绪",
  running: "执行中",
  cancelling: "取消中",
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

interface AgentRunShortcutEntry {
  runId: string;
  agentId: string;
  workspaceTitle: string;
  executionStatus: SessionExecutionStatusValue;
  updatedAt: string | number;
}

interface LifecycleShortcutListProps {
  projectId: string | null;
}

function normalizeExecutionStatus(status: string): SessionExecutionStatusValue {
  if (
    status === "idle"
    || status === "running"
    || status === "cancelling"
    || status === "completed"
    || status === "failed"
    || status === "interrupted"
  ) {
    return status;
  }
  return "idle";
}

function shortcutEntryFromAgentRun(entry: AgentRunWorkspaceListEntry): AgentRunShortcutEntry {
  return {
    runId: entry.run_ref.run_id,
    agentId: entry.agent_ref.agent_id,
    workspaceTitle: entry.shell.display_title.trim() || "AgentRun 加载中...",
    executionStatus: normalizeExecutionStatus(entry.shell.delivery_status),
    updatedAt: entry.shell.last_activity_at,
  };
}

export function AgentRunShortcutList({ projectId }: LifecycleShortcutListProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const agentRunRouteMatch = useMatch("/agent-runs/:runId/:agentId");
  const [entries, setEntries] = useState<AgentRunWorkspaceListEntry[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!projectId) {
      return;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const view = await fetchProjectAgentRuns(projectId);
        if (!cancelled) {
          setEntries(view.agent_runs);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : "AgentRun 列表加载失败");
      }
    };
    void load();
    const timer = window.setInterval(() => {
      void load();
    }, 30_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [projectId]);

  const agentRunEntries = useMemo(() => {
    if (!projectId) return [];
    return entries
      .map(shortcutEntryFromAgentRun)
      .sort((a, b) => updatedAtTimestamp(b.updatedAt) - updatedAtTimestamp(a.updatedAt));
  }, [entries, projectId]);

  const activeRunId = agentRunRouteMatch?.params.runId ?? null;
  const activeAgentId = agentRunRouteMatch?.params.agentId ?? null;

  return (
    <div className="flex min-h-0 flex-1 flex-col border-b border-border">
      <div className="flex shrink-0 items-center justify-between px-4 pb-1.5 pt-3">
        <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
          AgentRun
        </span>
        {agentRunEntries.length > 0 && (
          <span className="text-[10px] text-muted-foreground/70">
            {agentRunEntries.length}
          </span>
        )}
      </div>

      {!projectId ? (
        <p className="px-4 pb-3 text-xs text-muted-foreground">未选择项目</p>
      ) : agentRunEntries.length === 0 ? (
        <div className="px-4 pb-3">
          <p className="text-xs text-muted-foreground">暂无活跃 AgentRun</p>
          {error && <p className="mt-1 line-clamp-2 text-[11px] text-destructive">{error}</p>}
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-2">
          {agentRunEntries.map((entry) => {
            const isActive = activeRunId === entry.runId && activeAgentId === entry.agentId;
            const path = `/agent-runs/${encodeURIComponent(entry.runId)}/${encodeURIComponent(entry.agentId)}`;
            return (
              <button
                key={`${entry.runId}:${entry.agentId}`}
                type="button"
                onClick={() => {
                  if (location.pathname !== path) {
                    navigate(path);
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
                    {entry.workspaceTitle}
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
