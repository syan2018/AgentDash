/**
 * ActiveLifecycleList — 按 lifecycle run / agent 分组展示活跃执行单元
 *
 * 完全替代旧的 session-first 树状列表，以 lifecycle run → agent 为主轴。
 * session 仅作为 agent 下的 runtime trace tab 保留。
 */

import { useEffect, useMemo, useState } from "react";
import { useLifecycleStore } from "../../stores/lifecycleStore";
import type { LifecycleRunView, LifecycleAgentView } from "../../types";

// ─── Status helpers ──────────────────────────────────────

const statusLabel: Record<string, string> = {
  active: "运行中",
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

// ─── AgentRow ────────────────────────────────────────────

interface AgentRowProps {
  agent: LifecycleAgentView;
  isSelected: boolean;
  onSelect: () => void;
}

function AgentRow({ agent, isSelected, onSelect }: AgentRowProps) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={`flex w-full items-center gap-2 rounded-[8px] py-2 pl-8 pr-3 text-left text-xs transition-colors ${
        isSelected ? "bg-primary/10 text-foreground" : "text-muted-foreground hover:bg-muted/40"
      }`}
    >
      <StatusDot status={agent.status} />
      <span className="min-w-0 flex-1 truncate">
        {agent.agent_role || agent.agent_kind}
      </span>
      <span className="shrink-0 text-[10px] text-muted-foreground/60">
        {statusLabel[agent.status] ?? agent.status}
      </span>
    </button>
  );
}

// ─── RunGroup ────────────────────────────────────────────

interface RunGroupProps {
  run: LifecycleRunView;
  agents: LifecycleAgentView[];
  selectedAgentId: string | null;
  onSelectAgent: (runId: string, agentId: string) => void;
}

function RunGroup({ run, agents, selectedAgentId, onSelectAgent }: RunGroupProps) {
  const [collapsed, setCollapsed] = useState(false);

  const subjectLabel = run.subject_associations[0]
    ? `${run.subject_associations[0].subject_ref.kind} · ${run.subject_associations[0].subject_ref.id.slice(0, 8)}`
    : null;
  const runId = run.run_ref.run_id;

  return (
    <div>
      <button
        type="button"
        onClick={() => setCollapsed((v) => !v)}
        className="group flex w-full items-center gap-2 border-b border-border/40 bg-muted/20 px-3 py-2 text-left transition-colors hover:bg-muted/40"
      >
        <span
          className={`inline-block shrink-0 text-[10px] transition-transform ${collapsed ? "" : "rotate-90"}`}
        >
          ▶
        </span>
        <StatusDot status={run.status} />
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">
          Run · {runId.slice(0, 8)}
        </span>
        {subjectLabel && (
          <span className="shrink-0 text-[10px] text-muted-foreground/60">{subjectLabel}</span>
        )}
        <span className="shrink-0 text-[10px] text-muted-foreground/60">
          {agents.length} agent{agents.length !== 1 ? "s" : ""}
        </span>
      </button>
      {!collapsed &&
        agents.map((agent) => (
          <AgentRow
            key={agent.agent_ref.agent_id}
            agent={agent}
            isSelected={selectedAgentId === agent.agent_ref.agent_id}
            onSelect={() => onSelectAgent(runId, agent.agent_ref.agent_id)}
          />
        ))}
    </div>
  );
}

// ─── SearchBar ───────────────────────────────────────────

function LifecycleSearchBar({
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
          placeholder="搜索 run / agent…"
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

// ─── ActiveLifecycleList ─────────────────────────────────

interface ActiveLifecycleListProps {
  projectId: string;
  isLoading: boolean;
  selectedAgentId: string | null;
  onSelectAgent: (runId: string, agentId: string) => void;
}

export function ActiveLifecycleList({
  projectId,
  isLoading,
  selectedAgentId,
  onSelectAgent,
}: ActiveLifecycleListProps) {
  const runs = useLifecycleStore((s) => s.runs);
  const agents = useLifecycleStore((s) => s.agents);
  const fetchProjectActiveAgents = useLifecycleStore((s) => s.fetchProjectActiveAgents);
  const [keyword, setKeyword] = useState("");

  useEffect(() => {
    fetchProjectActiveAgents(projectId);
  }, [projectId, fetchProjectActiveAgents]);

  const runList = useMemo(
    () => Array.from(runs.values()).filter((r) => r.project_id === projectId),
    [runs, projectId],
  );

  const filteredRuns = useMemo(() => {
    if (!keyword.trim()) return runList;
    const lower = keyword.toLowerCase();
    return runList.filter(
      (r) =>
        r.run_ref.run_id.toLowerCase().includes(lower) ||
        r.subject_associations.some(
          (sa) =>
            sa.subject_ref.kind.toLowerCase().includes(lower) ||
            sa.subject_ref.id.toLowerCase().includes(lower),
        ),
    );
  }, [runList, keyword]);

  const agentsByRunId = useMemo(() => {
    const map = new Map<string, LifecycleAgentView[]>();
    for (const agent of agents.values()) {
      const arr = map.get(agent.agent_ref.run_id) ?? [];
      arr.push(agent);
      map.set(agent.agent_ref.run_id, arr);
    }
    return map;
  }, [agents]);

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  const isEmpty = filteredRuns.length === 0;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-5">
        <div className="flex items-center gap-2.5">
          <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            LIFECYCLE
          </span>
          <div>
            <p className="text-sm font-semibold tracking-tight text-foreground">活跃执行</p>
            <p className="text-xs text-muted-foreground">
              {runList.length} 个 run · {agents.size} 个 agent
            </p>
          </div>
        </div>
      </div>

      {runList.length > 0 && (
        <LifecycleSearchBar keyword={keyword} onKeywordChange={setKeyword} />
      )}

      <div className="flex-1 overflow-y-auto">
        {isEmpty ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <p className="text-sm font-medium text-muted-foreground">暂无活跃执行</p>
            <p className="text-xs text-muted-foreground/60">
              lifecycle run 启动后将在此显示
            </p>
          </div>
        ) : (
          filteredRuns.map((run) => (
            <RunGroup
              key={run.run_ref.run_id}
              run={run}
              agents={agentsByRunId.get(run.run_ref.run_id) ?? []}
              selectedAgentId={selectedAgentId}
              onSelectAgent={onSelectAgent}
            />
          ))
        )}
      </div>
    </div>
  );
}
