import { useCallback, useEffect, useMemo, useState } from "react";
import type {
  LifecycleDefinition,
  LifecycleStepDefinition,
  WorkflowRun,
  WorkflowStepState,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import { SessionList } from "../acp-session";
import { STEP_STATUS_LABEL } from "./shared-labels";

const EMPTY_RUNS: WorkflowRun[] = [];
const POLL_INTERVAL = 5000;

// ─── LifecycleNodeCard ───────────────────────────────────

function nodeStatusBadgeClass(status: WorkflowStepState["status"]) {
  switch (status) {
    case "completed":
      return "border-emerald-300/40 bg-emerald-500/10 text-emerald-700";
    case "running":
      return "border-primary/30 bg-primary/10 text-primary animate-pulse";
    case "ready":
      return "border-amber-300/40 bg-amber-500/10 text-amber-700";
    case "failed":
      return "border-destructive/30 bg-destructive/10 text-destructive";
    default:
      return "border-border bg-secondary/30 text-muted-foreground";
  }
}

function nodeTypeLabel(def: LifecycleStepDefinition | null): string {
  if (!def) return "";
  return def.node_type === "agent_node" ? "Agent Node" : "Phase Node";
}

interface LifecycleNodeCardProps {
  stepState: WorkflowStepState;
  stepDef: LifecycleStepDefinition | null;
  isActive: boolean;
}

function LifecycleNodeCard({ stepState, stepDef, isActive }: LifecycleNodeCardProps) {
  const [expanded, setExpanded] = useState(isActive);

  // isActive 变为 true 时自动展开；用户之后仍可手动收起。合法的 prop→state 同步。
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    if (isActive) setExpanded(true);
  }, [isActive]);

  const hasSession = Boolean(stepState.session_id);
  const heading = stepDef?.description?.trim()
    ? `${stepState.step_key} · ${stepDef.description}`
    : stepState.step_key;

  return (
    <div
      className={`rounded-[12px] border transition-colors ${
        isActive
          ? "border-primary/30 bg-primary/[0.02]"
          : "border-border bg-background"
      }`}
    >
      {/* Header */}
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left"
      >
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-sm font-medium text-foreground">{heading}</span>
            <span className={`rounded-full border px-2 py-0.5 text-[11px] ${nodeStatusBadgeClass(stepState.status)}`}>
              {STEP_STATUS_LABEL[stepState.status] ?? stepState.status}
            </span>
            {stepDef && (
              <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                {nodeTypeLabel(stepDef)}
              </span>
            )}
          </div>
          {stepState.summary && (
            <p className="mt-1 truncate text-xs text-muted-foreground">{stepState.summary}</p>
          )}
        </div>
        <span className="shrink-0 text-xs text-muted-foreground">
          {expanded ? "▲" : "▼"}
        </span>
      </button>

      {/* Expanded: session stream */}
      {expanded && (
        <div className="border-t border-border">
          {hasSession ? (
            <div className="h-[28rem] overflow-y-auto">
              <SessionList
                sessionId={stepState.session_id!}
                autoScroll={isActive}
              />
            </div>
          ) : (
            <div className="px-4 py-8 text-center text-sm text-muted-foreground">
              {stepState.status === "pending"
                ? "等待前驱 node 完成..."
                : stepState.status === "ready"
                  ? "准备创建 Agent Session..."
                  : "暂无 Session"}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─── LifecycleProgressBar ────────────────────────────────

function LifecycleProgressBar({ stepStates }: { stepStates: WorkflowStepState[] }) {
  const total = stepStates.length;
  const completed = stepStates.filter((s) => s.status === "completed").length;
  const running = stepStates.filter((s) => s.status === "running").length;
  const pct = total > 0 ? Math.round(((completed + running * 0.5) / total) * 100) : 0;

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>{completed}/{total} nodes completed</span>
        <span>{pct}%</span>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-secondary/40">
        <div
          className="h-full rounded-full bg-primary transition-all duration-300"
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}

// ─── LifecycleSessionView ────────────────────────────────

export interface LifecycleSessionViewProps {
  sessionId: string;
}

export function LifecycleSessionView({ sessionId }: LifecycleSessionViewProps) {
  const lifecycleDefinitions = useWorkflowStore((s) => s.lifecycleDefinitions);
  const runs = useWorkflowStore(
    (s) => s.runsBySessionId[sessionId] ?? EMPTY_RUNS,
  );
  const fetchRunsBySession = useWorkflowStore((s) => s.fetchRunsBySession);
  const fetchLifecycles = useWorkflowStore((s) => s.fetchLifecycles);

  useEffect(() => {
    void fetchLifecycles();
    void fetchRunsBySession(sessionId);
    const interval = setInterval(() => {
      void fetchRunsBySession(sessionId);
    }, POLL_INTERVAL);
    return () => clearInterval(interval);
  }, [fetchLifecycles, fetchRunsBySession, sessionId]);

  const activeRun = useMemo(
    () =>
      runs.find((r) => r.status === "running")
      ?? runs.find((r) => r.status === "ready")
      ?? runs[0]
      ?? null,
    [runs],
  );

  const lifecycle: LifecycleDefinition | null = useMemo(
    () => (activeRun ? lifecycleDefinitions.find((l) => l.id === activeRun.lifecycle_id) ?? null : null),
    [activeRun, lifecycleDefinitions],
  );

  const activeNodeKeys: Set<string> = useMemo(
    () => new Set(activeRun?.active_node_keys ?? []),
    [activeRun],
  );

  const findStepDef = useCallback(
    (key: string): LifecycleStepDefinition | null =>
      lifecycle?.steps.find((s) => s.key === key) ?? null,
    [lifecycle],
  );

  if (!activeRun) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 p-4">
        <div className="text-center">
          <p className="text-sm font-medium text-foreground">Lifecycle 执行</p>
          <p className="mt-1 text-xs text-muted-foreground">
            此 session 尚无关联的 Lifecycle Run。
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-4">
      <LifecycleProgressBar stepStates={activeRun.step_states} />

      <div className="flex flex-wrap items-center gap-2">
        <span className="rounded-full border border-primary/30 bg-primary/10 px-2.5 py-0.5 text-xs text-primary">
          {lifecycle?.name ?? activeRun.lifecycle_id.slice(0, 8)}
        </span>
        {activeRun.active_node_keys && activeRun.active_node_keys.length > 0 && (
          <span className="text-xs text-muted-foreground">
            Active: {activeRun.active_node_keys.join(", ")}
          </span>
        )}
      </div>

      <div className="space-y-3">
        {activeRun.step_states.map((stepState) => (
          <LifecycleNodeCard
            key={stepState.step_key}
            stepState={stepState}
            stepDef={findStepDef(stepState.step_key)}
            isActive={activeNodeKeys.has(stepState.step_key)}
          />
        ))}
      </div>
    </div>
  );
}

export default LifecycleSessionView;
