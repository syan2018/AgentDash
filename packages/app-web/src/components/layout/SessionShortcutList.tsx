import { useEffect, useMemo } from "react";
import { useLocation, useMatch, useNavigate } from "react-router-dom";
import { StatusDot, type StatusDotTone } from "@agentdash/ui";
import { useLifecycleStore } from "../../stores/lifecycleStore";
import type { LifecycleAgentView, LifecycleRunView } from "../../types";

const RUN_STATUS_TONE: Record<string, StatusDotTone> = {
  draft: "muted",
  ready: "info",
  running: "success",
  blocked: "warning",
  completed: "info",
  failed: "danger",
  cancelled: "warning",
};

const AGENT_STATUS_TONE: Record<string, StatusDotTone> = {
  active: "success",
  running: "success",
  ready: "info",
  completed: "info",
  failed: "danger",
  cancelled: "warning",
};

function statusTone(status: string, kind: "run" | "agent"): StatusDotTone {
  const tones = kind === "run" ? RUN_STATUS_TONE : AGENT_STATUS_TONE;
  return tones[status] ?? "muted";
}

function runSubjectLabel(run: LifecycleRunView): string | null {
  const subject = run.subject_associations[0]?.subject_ref;
  if (!subject) return null;
  return `${subject.kind} · ${subject.id.slice(0, 8)}`;
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

interface LifecycleShortcutListProps {
  projectId: string | null;
}

export function SessionShortcutList({ projectId }: LifecycleShortcutListProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const agentRouteMatch = useMatch("/agent/:agentId");
  const runRouteMatch = useMatch("/run/:runId");
  const sessionRouteMatch = useMatch("/session/:sessionId");
  const runs = useLifecycleStore((s) => s.runs);
  const agents = useLifecycleStore((s) => s.agents);
  const fetchSubjectExecution = useLifecycleStore((s) => s.fetchSubjectExecution);
  const error = useLifecycleStore((s) => s.error);

  useEffect(() => {
    if (!projectId) return;
    void fetchSubjectExecution("project", projectId);
    const timer = window.setInterval(() => {
      void fetchSubjectExecution("project", projectId);
    }, 30_000);
    return () => window.clearInterval(timer);
  }, [fetchSubjectExecution, projectId]);

  const projectRuns = useMemo(() => {
    if (!projectId) return [];
    return Array.from(runs.values())
      .filter((run) => run.project_id === projectId)
      .sort((left, right) => right.last_activity_at.localeCompare(left.last_activity_at));
  }, [projectId, runs]);

  const agentsByRunId = useMemo(() => {
    const map = new Map<string, LifecycleAgentView[]>();
    for (const agent of agents.values()) {
      if (projectId && agent.project_id !== projectId) continue;
      const items = map.get(agent.agent_ref.run_id) ?? [];
      items.push(agent);
      map.set(agent.agent_ref.run_id, items);
    }
    for (const items of map.values()) {
      items.sort((left, right) => right.updated_at.localeCompare(left.updated_at));
    }
    return map;
  }, [agents, projectId]);

  const activeAgentId = agentRouteMatch?.params.agentId ?? null;
  const activeRunId = runRouteMatch?.params.runId ?? null;
  const activeTraceId = sessionRouteMatch?.params.sessionId ?? null;

  return (
    <div className="flex min-h-0 flex-1 flex-col border-b border-border">
      <div className="flex shrink-0 items-center justify-between px-4 pb-1.5 pt-3">
        <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
          Lifecycle
        </span>
        {projectRuns.length > 0 && (
          <span className="text-[10px] text-muted-foreground/70">
            {projectRuns.length} run
          </span>
        )}
      </div>

      {!projectId ? (
        <p className="px-4 pb-3 text-xs text-muted-foreground">未选择项目</p>
      ) : projectRuns.length === 0 ? (
        <div className="px-4 pb-3">
          <p className="text-xs text-muted-foreground">暂无 lifecycle 执行</p>
          {error && <p className="mt-1 line-clamp-2 text-[11px] text-destructive">{error}</p>}
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-2">
          {projectRuns.map((run) => {
            const runId = run.run_ref.run_id;
            const runAgents = agentsByRunId.get(runId) ?? run.agents;
            const traceActive = activeTraceId
              ? run.runtime_trace_refs.some((ref) => ref.runtime_session_id === activeTraceId)
              : false;
            const runActive = activeRunId === runId || traceActive;
            const subject = runSubjectLabel(run);

            return (
              <div key={runId} className="mb-1">
                <button
                  type="button"
                  onClick={() => {
                    if (location.pathname !== `/run/${runId}`) navigate(`/run/${runId}`);
                  }}
                  className={`flex w-full flex-col gap-1 rounded-[8px] px-2.5 py-2 text-left transition-colors ${
                    runActive ? "bg-primary/10" : "hover:bg-secondary/50"
                  }`}
                  title={subject ? `Run ${runId} · ${subject}` : `Run ${runId}`}
                >
                  <div className="flex items-center gap-2">
                    <StatusDot
                      tone={statusTone(run.status, "run")}
                      size="sm"
                      pulse={run.status === "running"}
                      className="shrink-0"
                      title={run.status}
                    />
                    <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">
                      Run · {runId.slice(0, 8)}
                    </span>
                    <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                      {formatUpdatedAt(run.last_activity_at)}
                    </span>
                  </div>
                  {subject && (
                    <p className="ml-3.5 truncate text-[11px] leading-[1.35] text-muted-foreground">
                      {subject}
                    </p>
                  )}
                </button>

                {runAgents.map((agent) => {
                  const agentId = agent.agent_ref.agent_id;
                  const isActive = activeAgentId === agentId;
                  return (
                    <button
                      key={agentId}
                      type="button"
                      onClick={() => {
                        if (location.pathname === `/agent/${agentId}`) return;
                        navigate(`/agent/${agentId}`, {
                          state: {
                            run_id: runId,
                            frame_id: agent.current_frame_id ?? null,
                          },
                        });
                      }}
                      className={`ml-3 mt-0.5 flex w-[calc(100%-0.75rem)] items-center gap-2 rounded-[8px] px-2.5 py-1.5 text-left transition-colors ${
                        isActive ? "bg-primary/10 text-foreground" : "text-muted-foreground hover:bg-secondary/50"
                      }`}
                      title={agentId}
                    >
                      <StatusDot
                        tone={statusTone(agent.status, "agent")}
                        size="sm"
                        pulse={agent.status === "active" || agent.status === "running"}
                        className="shrink-0"
                        title={agent.status}
                      />
                      <span className="min-w-0 flex-1 truncate text-xs">
                        {agent.agent_role || agent.agent_kind}
                      </span>
                      <span className="shrink-0 font-mono text-[10px] text-muted-foreground/70">
                        {agentId.slice(0, 6)}
                      </span>
                    </button>
                  );
                })}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
