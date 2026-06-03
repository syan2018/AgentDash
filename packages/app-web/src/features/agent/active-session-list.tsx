/**
 * ActiveSessionList — 以后端 ProjectSessionListView 展示项目会话。
 */

import { useEffect, useMemo, useState } from "react";
import { fetchProjectSessionList } from "../../services/lifecycle";
import type { ProjectSessionListEntry } from "../../types";
import type { SessionExecutionStatusValue } from "../../services/session";
import { formatRelativeTime } from "../../lib/format";

const executionStatusLabel: Record<SessionExecutionStatusValue, string> = {
  idle: "就绪",
  running: "执行中",
  completed: "已完成",
  failed: "失败",
  interrupted: "已中断",
};

const executionStatusDotColor: Record<SessionExecutionStatusValue, string> = {
  idle: "bg-gray-400",
  running: "bg-emerald-500 animate-pulse",
  completed: "bg-blue-500",
  failed: "bg-red-500",
  interrupted: "bg-amber-500",
};

type StatusFilterGroup = "all" | "running" | "idle" | "ended";

const STATUS_TAB_OPTIONS: Array<{ value: StatusFilterGroup; label: string }> = [
  { value: "all", label: "全部" },
  { value: "running", label: "执行中" },
  { value: "idle", label: "就绪" },
  { value: "ended", label: "已结束" },
];

function normalizeExecutionStatus(status: string): SessionExecutionStatusValue {
  if (
    status === "idle"
    || status === "running"
    || status === "completed"
    || status === "failed"
    || status === "interrupted"
  ) {
    return status;
  }
  return "idle";
}

function statusGroupOf(status: SessionExecutionStatusValue): Exclude<StatusFilterGroup, "all"> {
  switch (status) {
    case "running": return "running";
    case "idle": return "idle";
    case "completed":
    case "failed":
    case "interrupted":
    default: return "ended";
  }
}

function updatedAtTimestamp(value: string | null | undefined): number | null {
  if (!value) return null;
  const numeric = Number(value);
  if (Number.isFinite(numeric) && numeric > 0) return numeric;
  const timestamp = new Date(value).getTime();
  return Number.isNaN(timestamp) ? null : timestamp;
}

function StatusDot({ status }: { status: SessionExecutionStatusValue }) {
  return (
    <span
      className={`inline-block h-2 w-2 shrink-0 rounded-full ${executionStatusDotColor[status] ?? "bg-gray-400"}`}
      title={executionStatusLabel[status] ?? status}
    />
  );
}

function SessionSearchBar({
  keyword,
  onKeywordChange,
}: {
  keyword: string;
  onKeywordChange: (v: string) => void;
}) {
  return (
    <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border bg-background px-3">
      <div className="relative flex h-7 min-w-0 flex-1 items-center">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
          className="pointer-events-none absolute left-2 text-muted-foreground/70"
        >
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.3-4.3" />
        </svg>
        <input
          type="text"
          value={keyword}
          onChange={(e) => onKeywordChange(e.target.value)}
          placeholder="搜索会话..."
          className="h-7 w-full rounded-md border border-border bg-muted/40 pl-8 pr-7 text-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary focus:bg-background"
          aria-label="搜索"
        />
        {keyword.length > 0 && (
          <button
            type="button"
            onClick={() => onKeywordChange("")}
            className="absolute right-1 flex h-5 w-5 items-center justify-center rounded-[8px] text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground"
            title="清除搜索"
          >
            x
          </button>
        )}
      </div>
    </div>
  );
}

interface SessionRowProps {
  entry: ProjectSessionListEntry;
  isSelected: boolean;
  onSelect: () => void;
}

function SessionRow({ entry, isSelected, onSelect }: SessionRowProps) {
  const executionStatus = normalizeExecutionStatus(entry.delivery_status);
  const updatedAt = updatedAtTimestamp(entry.updated_at);
  const title = entry.title.trim() || "会话加载中...";
  const subjectLabel = entry.subject_label?.trim() || null;

  return (
    <button
      type="button"
      onClick={onSelect}
      className={`flex w-full flex-col gap-0.5 rounded-[8px] px-3 py-2.5 text-left transition-colors ${
        isSelected ? "bg-primary/10" : "hover:bg-muted/40"
      }`}
    >
      <div className="flex items-center gap-2">
        <StatusDot status={executionStatus} />
        <span className={`min-w-0 flex-1 truncate text-xs font-medium ${
          isSelected ? "text-foreground" : "text-foreground/90"
        }`}>
          {title}
        </span>
        {updatedAt && (
          <span className="shrink-0 text-[10px] text-muted-foreground/60">
            {formatRelativeTime(updatedAt)}
          </span>
        )}
      </div>
      <div className="flex items-center gap-1.5 pl-4">
        {subjectLabel && (
          <span className="truncate text-[10px] text-muted-foreground/60">
            {subjectLabel}
          </span>
        )}
        <span className="ml-auto shrink-0 text-[10px] text-muted-foreground/60">
          {executionStatusLabel[executionStatus] ?? executionStatus}
        </span>
      </div>
    </button>
  );
}

interface ActiveSessionListProps {
  projectId: string;
  isLoading: boolean;
  selectedAgentId: string | null;
  onOpenSession: (runtimeSessionId: string, agentId?: string) => void;
}

export function ActiveSessionList({
  projectId,
  isLoading,
  selectedAgentId,
  onOpenSession,
}: ActiveSessionListProps) {
  const [sessions, setSessions] = useState<ProjectSessionListEntry[]>([]);
  const [isFetching, setIsFetching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [keyword, setKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilterGroup>("all");

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      setIsFetching(true);
      try {
        const view = await fetchProjectSessionList(projectId);
        if (!cancelled) {
          setSessions(view.sessions);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "会话列表加载失败");
        }
      } finally {
        if (!cancelled) setIsFetching(false);
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  const filteredEntries = useMemo(() => {
    let list = sessions;

    if (statusFilter !== "all") {
      list = list.filter((entry) =>
        statusGroupOf(normalizeExecutionStatus(entry.delivery_status)) === statusFilter
      );
    }

    const trimmedKeyword = keyword.trim().toLowerCase();
    if (trimmedKeyword) {
      list = list.filter((entry) => {
        const title = entry.title.toLowerCase();
        const subjectLabel = entry.subject_label?.toLowerCase() ?? "";
        return title.includes(trimmedKeyword) || subjectLabel.includes(trimmedKeyword);
      });
    }

    return [...list].sort(
      (a, b) => (updatedAtTimestamp(b.updated_at) ?? 0) - (updatedAtTimestamp(a.updated_at) ?? 0),
    );
  }, [keyword, sessions, statusFilter]);

  if (isLoading || isFetching) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-[8px] border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-5">
        <div className="flex items-center gap-2.5">
          <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            SESSION
          </span>
          <div>
            <p className="text-sm font-semibold tracking-tight text-foreground">活跃会话</p>
            <p className="text-xs text-muted-foreground">
              {sessions.length} 个会话
            </p>
          </div>
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-1 border-b border-border bg-background px-3 py-1.5">
        {STATUS_TAB_OPTIONS.map((tab) => (
          <button
            key={tab.value}
            type="button"
            onClick={() => setStatusFilter(tab.value)}
            className={`rounded-[6px] px-2.5 py-1 text-[11px] font-medium transition-colors ${
              statusFilter === tab.value
                ? "bg-primary/10 text-primary"
                : "text-muted-foreground hover:bg-muted/40 hover:text-foreground"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {sessions.length > 0 && (
        <SessionSearchBar keyword={keyword} onKeywordChange={setKeyword} />
      )}

      <div className="flex-1 overflow-y-auto">
        {filteredEntries.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <p className="text-sm font-medium text-muted-foreground">暂无活跃会话</p>
            <p className="text-xs text-muted-foreground/60">
              {error ?? "从左侧 Agent 面板发起会话后将在此显示"}
            </p>
          </div>
        ) : (
          <div className="space-y-0.5 p-1">
            {filteredEntries.map((entry) => (
              <SessionRow
                key={entry.runtime_session_id}
                entry={entry}
                isSelected={selectedAgentId === entry.agent_ref?.agent_id}
                onSelect={() => onOpenSession(
                  entry.runtime_session_id,
                  entry.agent_ref?.agent_id,
                )}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export { ActiveSessionList as ActiveLifecycleList };
