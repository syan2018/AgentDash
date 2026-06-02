/**
 * ActiveSessionList — 以 session title 为主展示的活跃会话列表。
 *
 * 底层数据由 lifecycle run → agent → runtime_session_ref 驱动，
 * 但用户视角是 "会话列表"：标题、状态、agent 角色、subject 归属。
 */

import { useEffect, useMemo, useState } from "react";
import { useLifecycleStore } from "../../stores/lifecycleStore";
import { findStoryById, useStoryStore } from "../../stores/storyStore";
import type { LifecycleRunView, LifecycleAgentView } from "../../types";
import { formatRelativeTime } from "../../lib/format";
import { groupSessionsBySubject, type SessionEntry, type SessionGroup } from "./lifecycle-grouping";

// ─── Status helpers ──────────────────────────────────────

const statusLabel: Record<string, string> = {
  active: "就绪",
  completed: "已完成",
  failed: "失败",
  paused: "已暂停",
  pending: "待启动",
};

const statusDotColor: Record<string, string> = {
  active: "bg-emerald-500",
  completed: "bg-blue-500",
  failed: "bg-red-500",
  paused: "bg-amber-500",
  pending: "bg-gray-400",
};

function StatusDot({ status }: { status: string }) {
  return (
    <span
      className={`inline-block h-2 w-2 shrink-0 rounded-full ${statusDotColor[status] ?? "bg-gray-400"}`}
      title={statusLabel[status] ?? status}
    />
  );
}

// ─── SessionRow：两行式会话行 ────────────────────────────

interface SessionRowProps {
  run: LifecycleRunView;
  agent: LifecycleAgentView;
  sessionTitle: string | null;
  isSelected: boolean;
  onSelect: () => void;
  /** sub-agent 行缩进 */
  indent?: boolean;
  /** 同 run 下的 sub-agent 数量 */
  subAgentCount?: number;
  /** sub-agent 是否展开 */
  subAgentsExpanded?: boolean;
  onToggleSubAgents?: () => void;
}

/**
 * 从 storyStore 中解析 subject 的可读标签。
 * 只显示能找到标题的 subject，绝不 fallback 到 GUID。
 */
function resolveSubjectDisplayLabel(run: LifecycleRunView): string | null {
  if (run.subject_associations.length === 0) return null;
  const sa = run.subject_associations[0];
  const { kind, id } = sa.subject_ref;
  const state = useStoryStore.getState();

  if (kind === "story") {
    const story = findStoryById(state.storiesByProjectId, id);
    return story ? `Story · ${story.title}` : null;
  }
  if (kind === "task") {
    for (const tasks of Object.values(state.tasksByStoryId)) {
      const task = tasks.find((t) => t.id === id);
      if (task) return `任务 · ${task.title}`;
    }
    return null;
  }
  return null;
}

function SessionRow({
  run,
  agent,
  sessionTitle,
  isSelected,
  onSelect,
  indent,
  subAgentCount = 0,
  subAgentsExpanded = false,
  onToggleSubAgents,
}: SessionRowProps) {
  const subjectLabel = resolveSubjectDisplayLabel(run);

  const title = sessionTitle?.trim() || agent.agent_role || agent.agent_kind || "会话";
  const updatedAt = agent.updated_at ? new Date(agent.updated_at).getTime() : null;

  return (
    <button
      type="button"
      onClick={onSelect}
      className={`flex w-full flex-col gap-0.5 rounded-[8px] py-2.5 text-left transition-colors ${
        indent ? "pl-7 pr-3" : "px-3"
      } ${isSelected ? "bg-primary/10" : "hover:bg-muted/40"}`}
    >
      <div className="flex items-center gap-2">
        <StatusDot status={agent.status} />
        <span className={`min-w-0 flex-1 truncate text-xs font-medium ${
          isSelected ? "text-foreground" : "text-foreground/90"
        }`}>
          {title}
        </span>
        {subAgentCount > 0 && (
          <span
            role="button"
            tabIndex={0}
            onClick={(e) => { e.stopPropagation(); onToggleSubAgents?.(); }}
            onKeyDown={(e) => { if (e.key === "Enter") { e.stopPropagation(); onToggleSubAgents?.(); } }}
            className="flex h-5 items-center gap-0.5 rounded-[6px] border border-border bg-secondary/60 px-1.5 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground"
            title={subAgentsExpanded ? "收起子 Agent" : "展开子 Agent"}
          >
            +{subAgentCount}
          </span>
        )}
        {updatedAt && (
          <span className="shrink-0 text-[10px] text-muted-foreground/60">
            {formatRelativeTime(updatedAt)}
          </span>
        )}
      </div>
      <div className="flex items-center gap-1.5 pl-4">
        {indent && (
          <span className="text-[10px] text-muted-foreground/50">子 Agent</span>
        )}
        {(sessionTitle || indent) && (agent.agent_role || agent.agent_kind) && (
          <span className="truncate text-[10px] text-muted-foreground">
            {agent.agent_role || agent.agent_kind}
          </span>
        )}
        {subjectLabel && !indent && (
          <>
            {sessionTitle && (agent.agent_role || agent.agent_kind) && (
              <span className="text-[10px] text-muted-foreground/40">·</span>
            )}
            <span className="truncate text-[10px] text-muted-foreground/60">
              {subjectLabel}
            </span>
          </>
        )}
        <span className="ml-auto shrink-0 text-[10px] text-muted-foreground/60">
          {statusLabel[agent.status] ?? agent.status}
        </span>
      </div>
    </button>
  );
}

// ─── SearchBar ───────────────────────────────────────────

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
          placeholder="搜索会话…"
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
            ×
          </button>
        )}
      </div>
    </div>
  );
}

// ─── SubjectGroupHeader ─────────────────────────────────

const groupKindLabel: Record<string, string> = {
  story: "Story",
  task: "Task",
  project: "项目",
};

function SubjectGroupHeader({
  group,
  collapsed,
  onToggle,
}: {
  group: SessionGroup;
  collapsed: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="group flex w-full items-center gap-2 border-b border-border/40 bg-muted/20 px-3 py-2 text-left transition-colors hover:bg-muted/40"
    >
      <span
        className={`inline-block shrink-0 text-[10px] transition-transform ${collapsed ? "" : "rotate-90"}`}
      >
        ▶
      </span>
      <span className="rounded-[4px] bg-secondary px-1.5 py-0.5 text-[10px] font-medium uppercase text-muted-foreground">
        {groupKindLabel[group.kind] ?? group.kind}
      </span>
      <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">
        {group.label}
      </span>
      <span className="shrink-0 text-[10px] text-muted-foreground/60">
        {group.entries.length} 个会话
      </span>
    </button>
  );
}

// ─── ActiveSessionList ──────────────────────────────────

/** 将 lifecycle agent status 归并到用户可见的筛选 tab */
type StatusFilterGroup = "all" | "running" | "idle" | "ended";

function statusGroupOf(status: string): Exclude<StatusFilterGroup, "all"> {
  switch (status) {
    case "running": return "running";
    case "active":
    case "paused":
    case "pending": return "idle";
    case "completed":
    case "failed":
    default: return "ended";
  }
}

const STATUS_TAB_OPTIONS: Array<{ value: StatusFilterGroup; label: string }> = [
  { value: "all", label: "全部" },
  { value: "running", label: "执行中" },
  { value: "idle", label: "就绪" },
  { value: "ended", label: "已结束" },
];

interface ActiveSessionListProps {
  projectId: string;
  isLoading: boolean;
  selectedAgentId: string | null;
  onSelectAgent: (runId: string, agentId: string) => void;
}

export function ActiveSessionList({
  projectId,
  isLoading,
  selectedAgentId,
  onSelectAgent,
}: ActiveSessionListProps) {
  const runs = useLifecycleStore((s) => s.runs);
  const agents = useLifecycleStore((s) => s.agents);
  const sessionMetas = useLifecycleStore((s) => s.sessionMetas);
  const fetchProjectActiveAgents = useLifecycleStore((s) => s.fetchProjectActiveAgents);
  const [keyword, setKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilterGroup>("all");

  useEffect(() => {
    fetchProjectActiveAgents(projectId);
  }, [projectId, fetchProjectActiveAgents]);

  /** 按 run 聚合 agent：主 agent + sub-agents */
  interface RunEntry {
    run: LifecycleRunView;
    primaryAgent: LifecycleAgentView;
    subAgents: LifecycleAgentView[];
    sessionTitle: string | null;
    primarySessionId: string | null;
  }

  const runEntries = useMemo(() => {
    const entries: RunEntry[] = [];

    for (const run of runs.values()) {
      if (run.project_id !== projectId) continue;
      const runAgents = Array.from(agents.values()).filter(
        (a) => a.agent_ref.run_id === run.run_ref.run_id,
      );
      if (runAgents.length === 0) continue;

      const primarySessionId = run.runtime_trace_refs[0]?.runtime_session_id ?? null;
      const meta = primarySessionId ? sessionMetas.get(primarySessionId) ?? null : null;

      const [primary, ...subs] = runAgents;
      entries.push({
        run,
        primaryAgent: primary,
        subAgents: subs,
        sessionTitle: meta?.title ?? null,
        primarySessionId,
      });
    }

    return entries;
  }, [runs, agents, sessionMetas, projectId]);

  /** 展开为 SessionEntry 供分组/筛选使用 */
  const sessionEntries: SessionEntry[] = useMemo(() => {
    return runEntries.map((re) => ({
      run: re.run,
      agent: re.primaryAgent,
      sessionTitle: re.sessionTitle,
      primarySessionId: re.primarySessionId,
    }));
  }, [runEntries]);

  /** run_id → RunEntry 索引，用于渲染时取 sub-agents */
  const runEntryMap = useMemo(() => {
    const map = new Map<string, RunEntry>();
    for (const re of runEntries) {
      map.set(re.run.run_ref.run_id, re);
    }
    return map;
  }, [runEntries]);

  const [expandedSubAgents, setExpandedSubAgents] = useState<Set<string>>(new Set());
  const toggleSubAgents = (runId: string) => {
    setExpandedSubAgents((prev) => {
      const next = new Set(prev);
      if (next.has(runId)) next.delete(runId);
      else next.add(runId);
      return next;
    });
  };

  const filteredEntries = useMemo(() => {
    let list = sessionEntries;

    if (statusFilter !== "all") {
      list = list.filter((e) => statusGroupOf(e.agent.status) === statusFilter);
    }

    if (keyword.trim()) {
      const lower = keyword.toLowerCase();
      list = list.filter((e) => {
        const title = e.sessionTitle?.toLowerCase() ?? "";
        const role = (e.agent.agent_role || e.agent.agent_kind).toLowerCase();
        const subjectDisplayLabel = resolveSubjectDisplayLabel(e.run)?.toLowerCase() ?? "";
        return title.includes(lower) || role.includes(lower) || subjectDisplayLabel.includes(lower);
      });
    }

    return list;
  }, [sessionEntries, statusFilter, keyword]);

  const groups = useMemo(
    () => groupSessionsBySubject(filteredEntries),
    [filteredEntries],
  );
  const hasGroups = groups.length > 1 || (groups.length === 1 && groups[0].kind !== "project");

  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());
  const toggleGroup = (subjectId: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(subjectId)) next.delete(subjectId);
      else next.add(subjectId);
      return next;
    });
  };

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
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
              {sessionEntries.length} 个会话
            </p>
          </div>
        </div>
      </div>

      {/* 状态 tab */}
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

      {sessionEntries.length > 0 && (
        <SessionSearchBar keyword={keyword} onKeywordChange={setKeyword} />
      )}

      <div className="flex-1 overflow-y-auto">
        {filteredEntries.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <p className="text-sm font-medium text-muted-foreground">暂无活跃会话</p>
            <p className="text-xs text-muted-foreground/60">
              从左侧 Agent 面板发起会话后将在此显示
            </p>
          </div>
        ) : hasGroups ? (
          groups.map((group) => (
            <div key={group.subjectId}>
              <SubjectGroupHeader
                group={group}
                collapsed={collapsedGroups.has(group.subjectId)}
                onToggle={() => toggleGroup(group.subjectId)}
              />
              {!collapsedGroups.has(group.subjectId) && (
                <div className="space-y-0.5 p-1">
                  {group.entries.map((entry) => {
                    const re = runEntryMap.get(entry.run.run_ref.run_id);
                    const subs = re?.subAgents ?? [];
                    const runId = entry.run.run_ref.run_id;
                    const expanded = expandedSubAgents.has(runId);
                    return (
                      <div key={entry.agent.agent_ref.agent_id}>
                        <SessionRow
                          run={entry.run}
                          agent={entry.agent}
                          sessionTitle={entry.sessionTitle}
                          isSelected={selectedAgentId === entry.agent.agent_ref.agent_id}
                          onSelect={() => onSelectAgent(runId, entry.agent.agent_ref.agent_id)}
                          subAgentCount={subs.length}
                          subAgentsExpanded={expanded}
                          onToggleSubAgents={() => toggleSubAgents(runId)}
                        />
                        {expanded && subs.map((sub) => (
                          <SessionRow
                            key={sub.agent_ref.agent_id}
                            run={entry.run}
                            agent={sub}
                            sessionTitle={null}
                            isSelected={selectedAgentId === sub.agent_ref.agent_id}
                            onSelect={() => onSelectAgent(runId, sub.agent_ref.agent_id)}
                            indent
                          />
                        ))}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          ))
        ) : (
          <div className="space-y-0.5 p-1">
            {filteredEntries.map((entry) => {
              const re = runEntryMap.get(entry.run.run_ref.run_id);
              const subs = re?.subAgents ?? [];
              const runId = entry.run.run_ref.run_id;
              const expanded = expandedSubAgents.has(runId);
              return (
                <div key={entry.agent.agent_ref.agent_id}>
                  <SessionRow
                    run={entry.run}
                    agent={entry.agent}
                    sessionTitle={entry.sessionTitle}
                    isSelected={selectedAgentId === entry.agent.agent_ref.agent_id}
                    onSelect={() => onSelectAgent(runId, entry.agent.agent_ref.agent_id)}
                    subAgentCount={subs.length}
                    subAgentsExpanded={expanded}
                    onToggleSubAgents={() => toggleSubAgents(runId)}
                  />
                  {expanded && subs.map((sub) => (
                    <SessionRow
                      key={sub.agent_ref.agent_id}
                      run={entry.run}
                      agent={sub}
                      sessionTitle={null}
                      isSelected={selectedAgentId === sub.agent_ref.agent_id}
                      onSelect={() => onSelectAgent(runId, sub.agent_ref.agent_id)}
                      indent
                    />
                  ))}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

export { ActiveSessionList as ActiveLifecycleList };
