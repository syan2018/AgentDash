/**
 * AgentRunShortcutList — 侧栏 AgentRun 快捷列表。
 *
 * 展示当前项目的活跃 AgentRun，以 workspace shell title 为主。
 * 点击跳转到 `/agent-runs/:runId/:agentId`。
 */

import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { useLocation, useMatch, useNavigate } from "react-router-dom";
import { StatusDot, type StatusDotTone } from "@agentdash/ui";
import type { AgentRunListEntryView } from "../../types";
import { useAgentRunListState } from "../../features/agent/agent-run-list-state-store";
import {
  AGENT_RUN_DELIVERY_STATUS_LABEL,
  agentRunListPresentationStatus,
  type AgentRunDeliveryStatus,
} from "../../features/agent/agent-run-delivery-status";

/** AgentRun list presentation status 的视觉映射。 */
const EXECUTION_STATUS_TONE: Record<AgentRunDeliveryStatus, StatusDotTone> = {
  idle: "muted",
  running: "success",
  suspended: "warning",
  cancelling: "warning",
  completed: "info",
  failed: "danger",
  interrupted: "warning",
  lost: "danger",
};

function executionStatusTone(status: AgentRunDeliveryStatus): StatusDotTone {
  return EXECUTION_STATUS_TONE[status] ?? "muted";
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
  executionStatus: AgentRunDeliveryStatus;
  updatedAt: string | number;
  subagentCount: number;
}

/** 单行估算高度（px），用于自适应可见条数计算。需与下方 row className 的间距匹配。 */
const ROW_HEIGHT = 40;

interface LifecycleShortcutListProps {
  projectId: string | null;
}

function shortcutEntryFromAgentRun(entry: AgentRunListEntryView): AgentRunShortcutEntry {
  return {
    runId: entry.run_ref.run_id,
    agentId: entry.agent_ref.agent_id,
    workspaceTitle: entry.title.trim() || "AgentRun 加载中...",
    executionStatus: agentRunListPresentationStatus(
      entry.runtime?.thread_status,
      entry.runtime?.active_turn_id,
      entry.lifecycle_status,
    ),
    updatedAt: entry.last_activity_at,
    subagentCount: entry.subagent_count ?? 0,
  };
}

export function AgentRunShortcutList({ projectId }: LifecycleShortcutListProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const agentRunRouteMatch = useMatch("/agent-runs/:runId/:agentId");
  const listState = useAgentRunListState(projectId);
  const entries = listState.entries;
  const hasMoreOnServer = Boolean(listState.next_cursor);
  const error = listState.error;
  const listRef = useRef<HTMLDivElement | null>(null);
  const [maxVisible, setMaxVisible] = useState(8);

  const agentRunEntries = useMemo(() => {
    if (!projectId) return [];
    return entries.map(shortcutEntryFromAgentRun);
  }, [entries, projectId]);

  // 自适应高度：按容器实测高度计算可见条数，去掉常驻滚动条。
  useLayoutEffect(() => {
    const node = listRef.current;
    if (!node) return;
    const recompute = () => {
      const height = node.clientHeight;
      setMaxVisible(Math.max(1, Math.floor(height / ROW_HEIGHT)));
    };
    recompute();
    const observer = new ResizeObserver(recompute);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const activeRunId = agentRunRouteMatch?.params.runId ?? null;
  const activeAgentId = agentRunRouteMatch?.params.agentId ?? null;

  // 是否需要「查看全部」入口：本地超出可见容量，或服务端还有更多页。
  const needMoreEntry = agentRunEntries.length > maxVisible || hasMoreOnServer;
  const visibleEntries = needMoreEntry
    ? agentRunEntries.slice(0, Math.max(1, maxVisible - 1))
    : agentRunEntries;
  const hiddenCount = agentRunEntries.length - visibleEntries.length;

  // 外层 section 为「测量包络」：保持 flex-1 撑满（让 footer 留在底部）+ 挂 ref 测可用高度；
  // 内层可见块为内容高（不 flex-grow），border-b 跟随内容——条目少时不再留大片空白边框。
  return (
    <section ref={listRef} className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex flex-col">
        <div className="flex shrink-0 items-center justify-between px-4 pb-1.5 pt-3">
          <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
            AgentRun
          </span>
          {agentRunEntries.length > 0 && (
            <span className="text-[10px] text-muted-foreground/70">
              {agentRunEntries.length}{hasMoreOnServer ? "+" : ""}
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
          <div className="px-3 pb-2">
            {visibleEntries.map((entry) => {
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
                    title={AGENT_RUN_DELIVERY_STATUS_LABEL[entry.executionStatus] ?? entry.executionStatus}
                  />
                  <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">
                    {entry.workspaceTitle}
                  </span>
                  {entry.subagentCount > 0 && (
                    <span
                      className="shrink-0 rounded-[6px] bg-secondary px-1.5 text-[10px] tabular-nums text-muted-foreground"
                      title={`${entry.subagentCount} 个 subagent`}
                    >
                      {entry.subagentCount} sub
                    </span>
                  )}
                  <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                    {formatUpdatedAt(entry.updatedAt)}
                  </span>
                </div>
              </button>
            );
          })}
            {needMoreEntry && (
              <button
                type="button"
                onClick={() => navigate("/dashboard/agent")}
                className="flex w-full items-center justify-center rounded-[8px] px-2.5 py-2 text-left text-[11px] text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
              >
                {hiddenCount > 0 ? `+${hiddenCount} 更多 · 查看全部` : "查看全部"}
              </button>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
