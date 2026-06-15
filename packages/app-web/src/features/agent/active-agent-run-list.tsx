/**
 * ActiveAgentRunList — 以后端 AgentRunWorkspaceListView 展示项目 AgentRun。
 */

import { useEffect, useMemo, useState } from "react";
import { fetchAgentRunWorkspace, fetchProjectAgentRuns } from "../../services/lifecycle";
import type { AgentRunLineageRef, AgentRunWorkspaceListEntry } from "../../types";
import type { SessionExecutionStatusValue } from "../../services/session";
import { formatRelativeTime } from "../../lib/format";

/** UI 递归展开的最大深度兜底（lineage 支持任意深度且无环检测）。 */
const MAX_TREE_DEPTH = 16;

/** 顶层行的稳定空祖先集合，避免每次渲染新建。 */
const EMPTY_ANCESTORS: ReadonlySet<string> = new Set<string>();

const executionStatusLabel: Record<SessionExecutionStatusValue, string> = {
  idle: "就绪",
  running: "执行中",
  cancelling: "取消中",
  completed: "已完成",
  failed: "失败",
  interrupted: "已中断",
};

const executionStatusDotColor: Record<SessionExecutionStatusValue, string> = {
  idle: "bg-gray-400",
  running: "bg-emerald-500 animate-pulse",
  cancelling: "bg-amber-500 animate-pulse",
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
    || status === "cancelling"
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
    case "cancelling": return "running";
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

function AgentRunSearchBar({
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
          placeholder="搜索 AgentRun..."
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

function ExpandChevron({ expanded }: { expanded: boolean }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`shrink-0 text-muted-foreground/70 transition-transform ${expanded ? "rotate-90" : ""}`}
      aria-hidden
    >
      <path d="m9 18 6-6-6-6" />
    </svg>
  );
}

interface AgentRunTreeRowProps {
  runId: string;
  agentId: string;
  title: string;
  /** delivery 执行状态；子节点（lineage ref）暂无状态时为 null。 */
  executionStatus: SessionExecutionStatusValue | null;
  updatedAt: number | null;
  /** 副信息：top 行为 subject label，子行为 agent_kind / relation。 */
  metaLabel: string | null;
  /** 是否可能存在子节点（top: subagent_count>0；子行: 乐观允许下钻）。 */
  canExpand: boolean;
  depth: number;
  /** 祖先 agentId 路径，用于防环。 */
  ancestors: ReadonlySet<string>;
  selectedAgentId: string | null;
  onOpenAgentRun: (runId: string, agentId: string) => void;
}

function AgentRunTreeRow({
  runId,
  agentId,
  title,
  executionStatus,
  updatedAt,
  metaLabel,
  canExpand,
  depth,
  ancestors,
  selectedAgentId,
  onOpenAgentRun,
}: AgentRunTreeRowProps) {
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<AgentRunLineageRef[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isSelected = selectedAgentId === agentId;
  // 防环 + 深度上限：祖先链已含本节点或超深时不再允许展开。
  const expandable = canExpand && depth < MAX_TREE_DEPTH && !ancestors.has(agentId);

  const toggleExpand = async () => {
    const next = !expanded;
    setExpanded(next);
    if (next && children === null && !loading) {
      setLoading(true);
      try {
        const view = await fetchAgentRunWorkspace(runId, agentId);
        setChildren(view.children ?? []);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : "子 Agent 加载失败");
        setChildren([]);
      } finally {
        setLoading(false);
      }
    }
  };

  const childAncestors = useMemo(() => {
    const next = new Set(ancestors);
    next.add(agentId);
    return next;
  }, [ancestors, agentId]);

  return (
    <div>
      <div
        className={`flex w-full items-center gap-1 rounded-[8px] transition-colors ${
          isSelected ? "bg-primary/10" : "hover:bg-muted/40"
        }`}
        style={{ paddingLeft: `${depth * 16}px` }}
      >
        <button
          type="button"
          onClick={() => void toggleExpand()}
          disabled={!expandable}
          aria-label={expanded ? "收起 subagent" : "展开 subagent"}
          className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-[6px] ${
            expandable ? "hover:bg-muted/60" : "invisible"
          }`}
        >
          <ExpandChevron expanded={expanded} />
        </button>
        <button
          type="button"
          onClick={() => onOpenAgentRun(runId, agentId)}
          className="flex min-w-0 flex-1 flex-col gap-0.5 py-2 pr-3 text-left"
        >
          <div className="flex items-center gap-2">
            <StatusDot status={executionStatus ?? "idle"} />
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
          {(metaLabel || executionStatus) && (
            <div className="flex items-center gap-1.5">
              {metaLabel && (
                <span className="truncate text-[10px] text-muted-foreground/60">
                  {metaLabel}
                </span>
              )}
              {executionStatus && (
                <span className="ml-auto shrink-0 text-[10px] text-muted-foreground/60">
                  {executionStatusLabel[executionStatus] ?? executionStatus}
                </span>
              )}
            </div>
          )}
        </button>
      </div>

      {expanded && (
        <div>
          {loading && (
            <p className="py-1 text-[10px] text-muted-foreground/60" style={{ paddingLeft: `${(depth + 1) * 16 + 28}px` }}>
              加载中…
            </p>
          )}
          {error && (
            <p className="py-1 text-[10px] text-destructive" style={{ paddingLeft: `${(depth + 1) * 16 + 28}px` }}>
              {error}
            </p>
          )}
          {children && children.length === 0 && !loading && !error && (
            <p className="py-1 text-[10px] text-muted-foreground/50" style={{ paddingLeft: `${(depth + 1) * 16 + 28}px` }}>
              无子 Agent
            </p>
          )}
          {children?.map((child) => (
            <AgentRunTreeRow
              key={`${child.run_id}:${child.agent_id}`}
              runId={child.run_id}
              agentId={child.agent_id}
              title={child.display_title.trim() || child.agent_kind || "Subagent"}
              executionStatus={null}
              updatedAt={null}
              metaLabel={[child.agent_kind, child.relation_kind].filter(Boolean).join(" · ") || null}
              canExpand
              depth={depth + 1}
              ancestors={childAncestors}
              selectedAgentId={selectedAgentId}
              onOpenAgentRun={onOpenAgentRun}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface ActiveAgentRunListProps {
  projectId: string;
  isLoading: boolean;
  selectedAgentId: string | null;
  onOpenAgentRun: (runId: string, agentId: string) => void;
}

export function ActiveAgentRunList({
  projectId,
  isLoading,
  selectedAgentId,
  onOpenAgentRun,
}: ActiveAgentRunListProps) {
  const [agentRuns, setAgentRuns] = useState<AgentRunWorkspaceListEntry[]>([]);
  const [isFetching, setIsFetching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [keyword, setKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilterGroup>("all");

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      setIsFetching(true);
      try {
        const view = await fetchProjectAgentRuns(projectId);
        if (!cancelled) {
          setAgentRuns(view.agent_runs);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "AgentRun 列表加载失败");
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
    let list = agentRuns;

    if (statusFilter !== "all") {
      list = list.filter((entry) =>
        statusGroupOf(normalizeExecutionStatus(entry.shell.delivery_status)) === statusFilter
      );
    }

    const trimmedKeyword = keyword.trim().toLowerCase();
    if (trimmedKeyword) {
      list = list.filter((entry) => {
        const title = entry.shell.display_title.toLowerCase();
        const subjectLabel = entry.subject_label?.toLowerCase() ?? "";
        return title.includes(trimmedKeyword) || subjectLabel.includes(trimmedKeyword);
      });
    }

    return [...list].sort(
      (a, b) =>
        (updatedAtTimestamp(b.shell.last_activity_at) ?? 0)
        - (updatedAtTimestamp(a.shell.last_activity_at) ?? 0),
    );
  }, [agentRuns, keyword, statusFilter]);

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
            AGENT RUN
          </span>
          <div>
            <p className="text-sm font-semibold tracking-tight text-foreground">活跃 AgentRun</p>
            <p className="text-xs text-muted-foreground">
              {agentRuns.length} 个 AgentRun
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

      {agentRuns.length > 0 && (
        <AgentRunSearchBar keyword={keyword} onKeywordChange={setKeyword} />
      )}

      <div className="flex-1 overflow-y-auto">
        {filteredEntries.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <p className="text-sm font-medium text-muted-foreground">暂无活跃 AgentRun</p>
            <p className="text-xs text-muted-foreground/60">
              {error ?? "从左侧 Agent 面板发起 AgentRun 后将在此显示"}
            </p>
          </div>
        ) : (
          <div className="space-y-0.5 p-1">
            {filteredEntries.map((entry) => (
              <AgentRunTreeRow
                key={`${entry.run_ref.run_id}:${entry.agent_ref.agent_id}`}
                runId={entry.run_ref.run_id}
                agentId={entry.agent_ref.agent_id}
                title={entry.shell.display_title.trim() || "AgentRun 加载中..."}
                executionStatus={normalizeExecutionStatus(entry.shell.delivery_status)}
                updatedAt={updatedAtTimestamp(entry.shell.last_activity_at)}
                metaLabel={entry.subject_label?.trim() || null}
                canExpand={(entry.subagent_count ?? 0) > 0}
                depth={0}
                ancestors={EMPTY_ANCESTORS}
                selectedAgentId={selectedAgentId}
                onOpenAgentRun={onOpenAgentRun}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export { ActiveAgentRunList as ActiveLifecycleList };
