import { useCallback, useEffect, useMemo, useState } from "react";
import type {
  ActivityAttemptState,
  ActivityDefinition,
  WorkflowGraph,
  WorkflowRun,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import { SessionList } from "../session";
import { ATTEMPT_STATUS_LABEL } from "./shared-labels";
import { submitHumanDecision } from "../../services/workflow";

const EMPTY_RUNS: WorkflowRun[] = [];
const POLL_INTERVAL = 5000;

// ─── 状态映射 ───────────────────────────────────────────

function attemptStatusBadgeClass(status: ActivityAttemptState["status"]) {
  switch (status) {
    case "completed":
      return "border-success/40 bg-success/10 text-success";
    case "running":
      return "border-primary/30 bg-primary/10 text-primary animate-pulse";
    case "claiming":
      return "border-primary/20 bg-primary/5 text-primary";
    case "ready":
      return "border-warning/40 bg-warning/10 text-warning";
    case "failed":
      return "border-destructive/30 bg-destructive/10 text-destructive";
    case "cancelled":
      return "border-border bg-secondary/30 text-muted-foreground";
    default:
      return "border-border bg-secondary/30 text-muted-foreground";
  }
}

function nodeTypeLabel(def: ActivityDefinition | null): string {
  if (!def) return "";
  if (def.executor.kind === "agent") return `Agent · ${def.executor.session_policy}`;
  if (def.executor.kind === "human") return "Human Approval";
  return `Function · ${def.executor.type}`;
}

function ActivityAttemptCard({
  run,
  attempt,
  activityDef,
  isActive,
  onDecisionSubmitted,
}: {
  run: WorkflowRun;
  attempt: ActivityAttemptState;
  activityDef: ActivityDefinition | null;
  isActive: boolean;
  onDecisionSubmitted: (run: WorkflowRun) => void;
}) {
  const [feedback, setFeedback] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const executorRun = attempt.executor_run;
  const sessionId = executorRun?.kind === "agent_session" ? executorRun.session_id : null;
  const decisionPort =
    activityDef?.completion_policy.kind === "human_decision"
      ? activityDef.completion_policy.decision_port
      : "decision";

  const submitDecision = async (decision: "approved" | "rejected") => {
    setIsSubmitting(true);
    try {
      const updated = await submitHumanDecision({
        run_id: run.id,
        activity_key: attempt.activity_key,
        attempt: attempt.attempt,
        decision_port: decisionPort,
        decision,
        summary: feedback,
      });
      onDecisionSubmitted(updated);
      setFeedback("");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className={`rounded-[12px] border ${isActive ? "border-primary/30 bg-primary/[0.02]" : "border-border bg-background"}`}>
      <div className="flex items-center justify-between gap-3 px-4 py-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-sm font-medium text-foreground">
              {attempt.activity_key} #{attempt.attempt}
            </span>
            <span className={`rounded-[8px] border px-2 py-0.5 text-[11px] ${attemptStatusBadgeClass(attempt.status)}`}>
              {ATTEMPT_STATUS_LABEL[attempt.status] ?? attempt.status}
            </span>
            {activityDef && (
              <span className="rounded-[8px] border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                {nodeTypeLabel(activityDef)}
              </span>
            )}
          </div>
          {attempt.summary && (
            <p className="mt-1 text-xs text-muted-foreground">{attempt.summary}</p>
          )}
        </div>
      </div>

      {sessionId && (
        <div className="border-t border-border">
          <div className="h-[28rem] overflow-y-auto">
            <SessionList sessionId={sessionId} autoScroll={isActive} />
          </div>
        </div>
      )}

      {executorRun?.kind === "human_decision" && attempt.status === "running" && (
        <div className="space-y-2 border-t border-border px-4 py-3">
          <textarea
            value={feedback}
            onChange={(event) => setFeedback(event.target.value)}
            rows={3}
            className="agentdash-form-textarea"
            placeholder="审批意见或退回说明"
          />
          <div className="flex justify-end gap-2">
            <button
              type="button"
              disabled={isSubmitting}
              onClick={() => void submitDecision("rejected")}
              className="agentdash-button-secondary text-xs"
            >
              退回
            </button>
            <button
              type="button"
              disabled={isSubmitting}
              onClick={() => void submitDecision("approved")}
              className="agentdash-button-primary text-xs"
            >
              通过
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function ActivityArtifactPanel({ run }: { run: WorkflowRun }) {
  const state = run.activity_state;
  if (!state || (state.outputs.length === 0 && state.inputs.length === 0)) return null;

  const latestOutputs = latestOutputArtifacts(state.outputs);

  return (
    <section className="rounded-[12px] border border-border bg-background">
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <div>
          <p className="text-sm font-medium text-foreground">Artifacts</p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {state.outputs.length} outputs · {state.inputs.length} inputs
          </p>
        </div>
        <span className="rounded-[6px] border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
          latest / history
        </span>
      </div>

      <div className="grid gap-3 p-3 lg:grid-cols-2">
        <div className="space-y-2">
          <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            Latest Outputs
          </p>
          {latestOutputs.length === 0 ? (
            <p className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-3 text-center text-xs text-muted-foreground">
              暂无 output artifact
            </p>
          ) : (
            latestOutputs.map((artifact) => (
              <ArtifactRow
                key={`${artifact.activity_key}:${artifact.port_key}`}
                title={`${artifact.activity_key}.${artifact.port_key}`}
                subtitle={`attempt #${artifact.attempt}`}
                value={artifact.value}
              />
            ))
          )}
        </div>

        <div className="space-y-2">
          <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            Input History
          </p>
          {state.inputs.length === 0 ? (
            <p className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-3 text-center text-xs text-muted-foreground">
              暂无 input artifact
            </p>
          ) : (
            state.inputs.map((artifact) => (
              <ArtifactRow
                key={`${artifact.activity_key}:${artifact.attempt}:${artifact.port_key}:${artifact.source_activity_key}:${artifact.source_attempt}`}
                title={`${artifact.activity_key}#${artifact.attempt}.${artifact.port_key}`}
                subtitle={`from ${artifact.source_activity_key}#${artifact.source_attempt}.${artifact.source_port_key}`}
                value={artifact.value}
              />
            ))
          )}
        </div>
      </div>
    </section>
  );
}

function ArtifactRow({
  title,
  subtitle,
  value,
}: {
  title: string;
  subtitle: string;
  value: unknown;
}) {
  return (
    <div className="rounded-[8px] border border-border bg-secondary/20 px-3 py-2">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-mono text-[11px] text-foreground">{title}</span>
        <span className="shrink-0 text-[10px] text-muted-foreground">{subtitle}</span>
      </div>
      <pre className="mt-2 max-h-32 overflow-auto whitespace-pre-wrap break-words rounded-[6px] bg-background px-2 py-1.5 text-[11px] text-muted-foreground">
        {formatArtifactValue(value)}
      </pre>
    </div>
  );
}

function latestOutputArtifacts(outputs: NonNullable<WorkflowRun["activity_state"]>["outputs"]) {
  const latest = new Map<string, (typeof outputs)[number]>();
  for (const artifact of outputs) {
    const key = `${artifact.activity_key}:${artifact.port_key}`;
    const current = latest.get(key);
    if (!current || artifact.attempt >= current.attempt) {
      latest.set(key, artifact);
    }
  }
  return Array.from(latest.values()).sort((a, b) =>
    `${a.activity_key}.${a.port_key}`.localeCompare(`${b.activity_key}.${b.port_key}`, "zh-CN"),
  );
}

function formatArtifactValue(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

// ─── LifecycleProgressBar ────────────────────────────────

function LifecycleProgressBar({ attempts }: { attempts: ActivityAttemptState[] }) {
  const total = attempts.length;
  const completed = attempts.filter((a) => a.status === "completed").length;
  const running = attempts.filter((a) => a.status === "running" || a.status === "claiming").length;
  const pct = total > 0 ? Math.round(((completed + running * 0.5) / total) * 100) : 0;

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>{completed}/{total} attempts completed</span>
        <span>{pct}%</span>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-[8px] bg-secondary/40">
        <div
          className="h-full rounded-[8px] bg-primary transition-all duration-300"
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
    void fetchRunsBySession(sessionId);
    const interval = setInterval(() => {
      void fetchRunsBySession(sessionId);
    }, POLL_INTERVAL);
    return () => clearInterval(interval);
  }, [fetchRunsBySession, sessionId]);

  const activeRun = useMemo(
    () =>
      runs.find((r) => r.status === "running")
      ?? runs.find((r) => r.status === "ready")
      ?? runs[0]
      ?? null,
    [runs],
  );

  useEffect(() => {
    if (!activeRun) return;
    void fetchLifecycles({ projectId: activeRun.project_id });
  }, [activeRun, fetchLifecycles]);

  const lifecycle: WorkflowGraph | null = useMemo(
    () => (activeRun ? lifecycleDefinitions.find((l) => l.id === activeRun.lifecycle_id) ?? null : null),
    [activeRun, lifecycleDefinitions],
  );

  const activeNodeKeys: Set<string> = useMemo(
    () => new Set(activeRun?.active_node_keys ?? []),
    [activeRun],
  );

  const findActivityDef = useCallback(
    (key: string): ActivityDefinition | null =>
      lifecycle?.activities.find((s) => s.key === key) ?? null,
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

  const activityState = activeRun.activity_state;

  if (!activityState) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 p-4">
        <div className="text-center">
          <p className="text-sm font-medium text-foreground">Lifecycle Run 初始化中</p>
          <p className="mt-1 text-xs text-muted-foreground">
            正在等待 Activity 状态机就绪…
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-4">
      <LifecycleProgressBar attempts={activityState.attempts} />

      <div className="flex flex-wrap items-center gap-2">
        <span className="rounded-[8px] border border-primary/30 bg-primary/10 px-2.5 py-0.5 text-xs text-primary">
          {lifecycle?.name ?? activeRun.lifecycle_id.slice(0, 8)}
        </span>
        {activeRun.active_node_keys && activeRun.active_node_keys.length > 0 && (
          <span className="text-xs text-muted-foreground">
            Active: {activeRun.active_node_keys.join(", ")}
          </span>
        )}
      </div>

      <div className="space-y-3">
        {activityState.attempts.map((attempt) => (
          <ActivityAttemptCard
            key={`${attempt.activity_key}#${attempt.attempt}`}
            run={activeRun}
            attempt={attempt}
            activityDef={findActivityDef(attempt.activity_key)}
            isActive={activeNodeKeys.has(attempt.activity_key)}
            onDecisionSubmitted={() => void fetchRunsBySession(sessionId)}
          />
        ))}
      </div>

      <ActivityArtifactPanel run={activeRun} />
    </div>
  );
}

export default LifecycleSessionView;
