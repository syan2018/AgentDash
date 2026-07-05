/**
 * ActiveAgentRunList — 以后端 AgentRunWorkspaceListView 展示项目 AgentRun。
 *
 * 主 Run 行内联其直接子 Agent（一跳，含真实 shell 状态），`+N` 药丸就地展开；
 * 按 subject 分组聚合；列表 keyset 游标分页，「加载更多」按需续拉。
 */

import { CardMenu, ConfirmDialog, type CardMenuItem } from "@agentdash/ui";
import { useCallback, useMemo, useState } from "react";
import { useMatch, useNavigate } from "react-router-dom";
import type { AgentRunListChild, AgentRunWorkspaceListEntry } from "../../types";
import type { SessionExecutionStatusValue } from "../../services/session";
import { deleteAgentRun } from "../../services/agentRun";
import { formatRelativeTime } from "../../lib/format";
import { agentSourceLabel } from "../../lib/agent-source";
import {
  useAgentRunListState,
  useAgentRunListStateStore,
} from "./agent-run-list-state-store";
import {
  groupAgentRunsBySubject,
  groupKindLabel,
  hasMeaningfulGroups,
  type AgentRunGroup,
} from "./agent-run-grouping";

/** 首屏 / 每批分页大小（与后端默认一致）。 */
const PAGE_SIZE = 30;

const executionStatusLabel: Record<SessionExecutionStatusValue, string> = {
  idle: "就绪",
  running: "执行中",
  cancelling: "取消中",
  completed: "已完成",
  failed: "失败",
  interrupted: "已中断",
  lost: "已丢失",
};

const executionStatusDotColor: Record<SessionExecutionStatusValue, string> = {
  idle: "bg-gray-400",
  running: "bg-emerald-500 animate-pulse",
  cancelling: "bg-amber-500 animate-pulse",
  completed: "bg-blue-500",
  failed: "bg-red-500",
  interrupted: "bg-amber-500",
  lost: "bg-red-500",
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
    || status === "lost"
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
    case "lost":
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

/** 本项目所有 subAgent 从属关系统一称 Companion。 */
const COMPANION_LABEL = "Companion";

/**
 * Companion 折叠开关：第二行右侧、灰色低调胶囊。文案固定为「Companion · 数量」，
 * 展开/收起仅由 chevron 旋转区分（展开时转 90°）。
 */
function SubAgentToggle({
  count,
  expanded,
  onToggle,
}: {
  count: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  const a11yLabel = `${expanded ? "收起" : "展开"} ${count} 个 ${COMPANION_LABEL}`;
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onToggle();
      }}
      className="flex shrink-0 items-center gap-1 rounded-[8px] border border-border bg-muted/50 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      title={a11yLabel}
      aria-expanded={expanded}
      aria-label={a11yLabel}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="11"
        height="11"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.4"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
        className={`transition-transform ${expanded ? "rotate-90" : ""}`}
      >
        <path d="m9 18 6-6-6-6" />
      </svg>
      <span className="leading-none">{COMPANION_LABEL} · {count}</span>
    </button>
  );
}

/**
 * Agent 身份标识：展示绑定的 Project Agent 显示名（后端已解析 preset.display_name || name）。
 * 主行与各级子行共用。来源（source）以单独的 {@link SourceTag} 展示。
 */
function AgentIdentityMeta({ label }: { label?: string | null }) {
  const text = label?.trim() || null;
  if (!text) return null;
  return (
    <span className="min-w-0 truncate text-[10px] text-muted-foreground" title={text}>
      {text}
    </span>
  );
}

/**
 * Agent 来源标签：把后端标准化 `source` 枚举 slug 映射为人类可读短标签
 * （project_agent → Project 等）；unknown / 空值不渲染。主行与各级子行共用。
 */
function SourceTag({ source }: { source?: string | null }) {
  const label = agentSourceLabel(source);
  if (!label) return null;
  return (
    <span className="shrink-0 rounded-[4px] bg-secondary px-1 py-0.5 text-[9px] font-medium uppercase tracking-wide text-muted-foreground">
      {label}
    </span>
  );
}

interface AgentRunChildRowProps {
  child: AgentRunListChild;
  /** 缩进层级，顶层子 Agent 为 1，逐层递增。 */
  depth: number;
  selectedAgentId: string | null;
  onOpenAgentRun: (runId: string, agentId: string) => void;
}

function AgentRunChildRow({ child, depth, selectedAgentId, onOpenAgentRun }: AgentRunChildRowProps) {
  const [expanded, setExpanded] = useState(false);
  const status = normalizeExecutionStatus(child.shell.delivery_status);
  const updatedAt = updatedAtTimestamp(child.shell.last_activity_at);
  const title = child.shell.display_title.trim() || child.project_agent_label?.trim() || "Companion";
  const isSelected = selectedAgentId === child.agent_ref.agent_id;
  const nested = child.children ?? [];

  return (
    <div>
      <button
        type="button"
        onClick={() => onOpenAgentRun(child.run_ref.run_id, child.agent_ref.agent_id)}
        style={{ paddingLeft: `${12 + depth * 16}px` }}
        className={`flex w-full flex-col gap-0.5 rounded-[8px] py-2.5 pr-3 text-left transition-colors ${
          isSelected ? "bg-primary/10" : "hover:bg-muted/40"
        }`}
      >
        <div className="flex items-center gap-2">
          <span className="shrink-0 text-[11px] text-muted-foreground/70">↳</span>
          <StatusDot status={status} />
          <span className={`min-w-0 flex-1 truncate text-xs ${
            isSelected ? "font-medium text-foreground" : "text-foreground/80"
          }`}>
            {title}
          </span>
          {updatedAt && (
            <span className="shrink-0 text-[10px] text-muted-foreground/60">
              {formatRelativeTime(updatedAt)}
            </span>
          )}
        </div>
        <div className="flex items-center gap-1.5 pl-5">
          <SourceTag source={child.source} />
          <AgentIdentityMeta label={child.project_agent_label} />
          <span className="min-w-0 flex-1" />
          {nested.length > 0 && (
            <SubAgentToggle
              count={nested.length}
              expanded={expanded}
              onToggle={() => setExpanded((v) => !v)}
            />
          )}
          <span className="shrink-0 text-[10px] text-muted-foreground/60">
            {executionStatusLabel[status] ?? status}
          </span>
        </div>
      </button>

      {expanded && nested.map((grandChild) => (
        <AgentRunChildRow
          key={`${grandChild.run_ref.run_id}:${grandChild.agent_ref.agent_id}`}
          child={grandChild}
          depth={depth + 1}
          selectedAgentId={selectedAgentId}
          onOpenAgentRun={onOpenAgentRun}
        />
      ))}
    </div>
  );
}

interface AgentRunRowProps {
  entry: AgentRunWorkspaceListEntry;
  selectedAgentId: string | null;
  onOpenAgentRun: (runId: string, agentId: string) => void;
  onRequestDelete: (entry: AgentRunWorkspaceListEntry) => void;
}

function AgentRunRow({
  entry,
  selectedAgentId,
  onOpenAgentRun,
  onRequestDelete,
}: AgentRunRowProps) {
  const [expanded, setExpanded] = useState(false);
  const status = normalizeExecutionStatus(entry.shell.delivery_status);
  const updatedAt = updatedAtTimestamp(entry.shell.last_activity_at);
  const title = entry.shell.display_title.trim() || "AgentRun 加载中...";
  const children = entry.children ?? [];
  const isSelected = selectedAgentId === entry.agent_ref.agent_id;
  const menuItems = useMemo<CardMenuItem[]>(() => [
    {
      key: "delete",
      label: "删除 AgentRun",
      danger: true,
      onSelect: () => onRequestDelete(entry),
    },
  ], [entry, onRequestDelete]);
  const openRun = () => onOpenAgentRun(entry.run_ref.run_id, entry.agent_ref.agent_id);

  return (
    <div>
      <div
        role="button"
        tabIndex={0}
        onClick={openRun}
        onKeyDown={(event) => {
          if (event.target !== event.currentTarget) return;
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            openRun();
          }
        }}
        className={`flex w-full flex-col gap-0.5 rounded-[8px] px-3 py-2.5 text-left transition-colors ${
          isSelected ? "bg-primary/10" : "hover:bg-muted/40"
        }`}
      >
        <div className="flex items-center gap-2">
          <StatusDot status={status} />
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
          <CardMenu items={menuItems} />
        </div>
        <div className="flex items-center gap-1.5 pl-4">
          <SourceTag source={entry.source} />
          <AgentIdentityMeta label={entry.project_agent_label} />
          <span className="min-w-0 flex-1" />
          {children.length > 0 && (
            <SubAgentToggle
              count={children.length}
              expanded={expanded}
              onToggle={() => setExpanded((v) => !v)}
            />
          )}
          <span className="shrink-0 text-[10px] text-muted-foreground/60">
            {executionStatusLabel[status] ?? status}
          </span>
        </div>
      </div>

      {expanded && children.map((child) => (
        <AgentRunChildRow
          key={`${child.run_ref.run_id}:${child.agent_ref.agent_id}`}
          child={child}
          depth={1}
          selectedAgentId={selectedAgentId}
          onOpenAgentRun={onOpenAgentRun}
        />
      ))}
    </div>
  );
}

function SubjectGroupHeader({
  group,
  collapsed,
  onToggle,
}: {
  group: AgentRunGroup;
  collapsed: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="group flex w-full items-center gap-2 border-b border-border/40 bg-muted/20 px-3 py-2 text-left transition-colors hover:bg-muted/40"
    >
      <span className={`inline-block shrink-0 text-[10px] transition-transform ${collapsed ? "" : "rotate-90"}`}>
        ▶
      </span>
      {group.kind !== "ungrouped" && (
        <span className="rounded-[4px] bg-secondary px-1.5 py-0.5 text-[10px] font-medium uppercase text-muted-foreground">
          {groupKindLabel(group.kind)}
        </span>
      )}
      <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">
        {group.label}
      </span>
      <span className="shrink-0 text-[10px] text-muted-foreground/60">
        {group.entries.length} 个 AgentRun
      </span>
    </button>
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
  const listState = useAgentRunListState(projectId, PAGE_SIZE);
  const loadMoreProjectAgentRuns = useAgentRunListStateStore((state) => state.loadMore);
  const refreshProjectAgentRuns = useAgentRunListStateStore((state) => state.refreshProject);
  const navigate = useNavigate();
  const agentRunRouteMatch = useMatch("/agent-runs/:runId/:agentId");
  const agentRuns = listState.entries;
  const nextCursor = listState.next_cursor;
  const isFetching = listState.status === "loading";
  const isLoadingMore = listState.is_loading_more;
  const error = listState.error;
  const [keyword, setKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilterGroup>("all");
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());
  const [deleteTarget, setDeleteTarget] = useState<{
    runId: string;
    title: string;
  } | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  const loadMore = useCallback(async () => {
    if (!nextCursor || isLoadingMore) return;
    await loadMoreProjectAgentRuns(projectId, PAGE_SIZE);
  }, [isLoadingMore, loadMoreProjectAgentRuns, nextCursor, projectId]);

  const toggleGroup = (key: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const requestDelete = useCallback((entry: AgentRunWorkspaceListEntry) => {
    const title = entry.shell.display_title.trim() || "AgentRun 加载中...";
    setDeleteTarget({ runId: entry.run_ref.run_id, title });
    setDeleteError(null);
  }, []);

  const closeDeleteDialog = useCallback(() => {
    if (isDeleting) return;
    setDeleteTarget(null);
  }, [isDeleting]);

  const confirmDelete = useCallback(async () => {
    if (!deleteTarget || isDeleting) return;
    setIsDeleting(true);
    setDeleteError(null);
    try {
      await deleteAgentRun(projectId, deleteTarget.runId);
      await refreshProjectAgentRuns(projectId, "agent_run_deleted");
      if (agentRunRouteMatch?.params.runId === deleteTarget.runId) {
        navigate("/dashboard/agent");
      }
      setDeleteTarget(null);
    } catch (err) {
      setDeleteError(err instanceof Error ? err.message : "删除 AgentRun 失败");
    } finally {
      setIsDeleting(false);
    }
  }, [
    agentRunRouteMatch?.params.runId,
    deleteTarget,
    isDeleting,
    navigate,
    projectId,
    refreshProjectAgentRuns,
  ]);

  // 状态 tab + 关键词在**已加载窗口**内过滤（见任务 PRD：服务端过滤为后续）。
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
        const childMatch = (entry.children ?? []).some((child) =>
          child.shell.display_title.toLowerCase().includes(trimmedKeyword)
        );
        return title.includes(trimmedKeyword) || subjectLabel.includes(trimmedKeyword) || childMatch;
      });
    }

    return list;
  }, [agentRuns, keyword, statusFilter]);

  const groups = useMemo(() => groupAgentRunsBySubject(filteredEntries), [filteredEntries]);
  const showGroups = hasMeaningfulGroups(groups);

  if (isLoading || isFetching) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-[8px] border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  const renderRow = (entry: AgentRunWorkspaceListEntry) => (
    <AgentRunRow
      key={`${entry.run_ref.run_id}:${entry.agent_ref.agent_id}`}
      entry={entry}
      selectedAgentId={selectedAgentId}
      onOpenAgentRun={onOpenAgentRun}
      onRequestDelete={requestDelete}
    />
  );

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
              {agentRuns.length}{nextCursor ? "+" : ""} 个 AgentRun
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
        {deleteError && (
          <p className="border-b border-border bg-destructive/10 px-3 py-2 text-[11px] text-destructive">
            {deleteError}
          </p>
        )}
        {filteredEntries.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <p className="text-sm font-medium text-muted-foreground">暂无活跃 AgentRun</p>
            <p className="text-xs text-muted-foreground/60">
              {error ?? "从左侧 Agent 面板发起 AgentRun 后将在此显示"}
            </p>
          </div>
        ) : (
          <>
            {showGroups ? (
              groups.map((group) => (
                <div key={group.key}>
                  <SubjectGroupHeader
                    group={group}
                    collapsed={collapsedGroups.has(group.key)}
                    onToggle={() => toggleGroup(group.key)}
                  />
                  {!collapsedGroups.has(group.key) && (
                    <div className="space-y-0.5 p-1">
                      {group.entries.map(renderRow)}
                    </div>
                  )}
                </div>
              ))
            ) : (
              <div className="space-y-0.5 p-1">
                {filteredEntries.map(renderRow)}
              </div>
            )}

            {nextCursor && (
              <div className="p-2">
                <button
                  type="button"
                  onClick={() => void loadMore()}
                  disabled={isLoadingMore}
                  className="flex w-full items-center justify-center rounded-[8px] border border-border px-3 py-2 text-xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground disabled:opacity-60"
                >
                  {isLoadingMore ? "加载中…" : "加载更多"}
                </button>
              </div>
            )}
            {error && agentRuns.length > 0 && (
              <p className="px-3 pb-3 text-[11px] text-destructive">{error}</p>
            )}
          </>
        )}
      </div>
      <ConfirmDialog
        open={deleteTarget !== null}
        title="删除 AgentRun"
        description={`将删除「${deleteTarget?.title ?? ""}」及其关联 runtime trace facts。正在运行或取消中的 AgentRun 会被后端拒绝删除。`}
        confirmLabel="删除"
        tone="danger"
        onClose={closeDeleteDialog}
        onConfirm={() => void confirmDelete()}
        isConfirming={isDeleting}
      />
    </div>
  );
}

export { ActiveAgentRunList as ActiveLifecycleList };
