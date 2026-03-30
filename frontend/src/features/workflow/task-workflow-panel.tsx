import { useEffect, useMemo, useState } from "react";

import type {
  LifecycleDefinition,
  LifecycleExecutionEntry,
  LifecycleStepDefinition,
  SessionBindingOwner,
  Task,
  WorkflowDefinition,
  WorkflowRecordArtifact,
  WorkflowRecordArtifactType,
  WorkflowRun,
  WorkflowStepState,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import { fetchSessionBindings } from "../../services/session";
import {
  ARTIFACT_TYPE_LABEL,
  EXECUTION_EVENT_KIND_LABEL,
  RUN_STATUS_LABEL,
  STEP_STATUS_LABEL,
} from "./shared-labels";

const EMPTY_RUNS: WorkflowRun[] = [];

function findLifecycle(lifecycles: LifecycleDefinition[], lifecycleId: string): LifecycleDefinition | null {
  return lifecycles.find((item) => item.id === lifecycleId) ?? null;
}

function findWorkflowByKey(definitions: WorkflowDefinition[], workflowKey: string): WorkflowDefinition | null {
  return definitions.find((item) => item.key === workflowKey) ?? null;
}

function stepDefinition(
  lifecycle: LifecycleDefinition | null,
  stepKey: string,
): LifecycleStepDefinition | null {
  return lifecycle?.steps.find((item) => item.key === stepKey) ?? null;
}

function stepHeading(lifecycle: LifecycleDefinition | null, stepKey: string): string {
  const step = stepDefinition(lifecycle, stepKey);
  if (!step) return stepKey;
  return step.description?.trim() ? `${step.key} · ${step.description}` : step.key;
}

function stepWorkflowModeLabel(step: LifecycleStepDefinition | null): string {
  if (!step) return "";
  return step.workflow_key?.trim() ? `Workflow: ${step.workflow_key}` : "Manual Step";
}

function stepBadgeClass(status: WorkflowStepState["status"]) {
  switch (status) {
    case "completed":
      return "border-emerald-300/40 bg-emerald-500/10 text-emerald-700";
    case "running":
      return "border-primary/30 bg-primary/10 text-primary";
    case "ready":
      return "border-amber-300/40 bg-amber-500/10 text-amber-700";
    case "failed":
      return "border-destructive/30 bg-destructive/10 text-destructive";
    default:
      return "border-border bg-secondary/30 text-muted-foreground";
  }
}

function buildCompletionArtifacts(
  workflow: WorkflowDefinition | null,
  stepKey: string,
  summary: string,
): Array<{
  artifact_type: WorkflowRecordArtifactType;
  title: string;
  content: string;
}> {
  const trimmed = summary.trim();
  if (!trimmed) return [];
  const artifactType = workflow?.contract.completion.default_artifact_type ?? "phase_note";
  const artifactTitle =
    workflow?.contract.completion.default_artifact_title?.trim() || `${stepKey} 阶段记录`;

  return [
    {
      artifact_type: artifactType,
      title: artifactTitle,
      content: trimmed,
    },
  ];
}

function selectPreferredRun(runs: WorkflowRun[]): WorkflowRun | null {
  return runs.find((run) => run.status === "running")
    ?? runs.find((run) => run.status === "ready")
    ?? runs[0]
    ?? null;
}

function AgentInstructionsCollapsible({
  stepKey,
  instructions,
}: {
  stepKey: string;
  instructions: string[];
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="mt-3">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="text-xs text-muted-foreground/60 transition-colors hover:text-muted-foreground"
      >
        {open ? "▲ 收起 Workflow 注入指令" : `▶ ${instructions.length} 条 Workflow 注入指令`}
      </button>
      {open && (
        <div className="mt-1.5 rounded-[10px] border border-border bg-secondary/20 p-3">
          <div className="space-y-1 text-xs leading-5 text-foreground/60">
            {instructions.map((instruction, index) => (
              <p key={`${stepKey}-instruction-${index}`}>- {instruction}</p>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

type ArtifactCategory = {
  label: string;
  types: WorkflowRecordArtifactType[];
};

const ARTIFACT_CATEGORIES: ArtifactCategory[] = [
  { label: "执行轨迹", types: ["execution_trace", "decision_record"] },
  { label: "上下文快照", types: ["context_snapshot"] },
  { label: "阶段总结", types: ["phase_note", "session_summary", "journal_update"] },
  { label: "检查证据", types: ["checklist_evidence"] },
  { label: "其他", types: ["archive_suggestion"] },
];

function categorizeArtifacts(artifacts: WorkflowRecordArtifact[]) {
  const result: Array<{ category: ArtifactCategory; artifacts: WorkflowRecordArtifact[] }> = [];
  for (const category of ARTIFACT_CATEGORIES) {
    const matched = artifacts.filter((a) => category.types.includes(a.artifact_type));
    if (matched.length > 0) {
      result.push({ category, artifacts: matched });
    }
  }
  return result;
}

function eventKindBadgeClass(kind: string) {
  switch (kind) {
    case "step_completed":
      return "border-emerald-300/40 bg-emerald-500/10 text-emerald-700";
    case "step_activated":
      return "border-primary/30 bg-primary/10 text-primary";
    case "constraint_blocked":
      return "border-destructive/30 bg-destructive/10 text-destructive";
    case "completion_evaluated":
      return "border-amber-300/40 bg-amber-500/10 text-amber-700";
    case "artifact_appended":
      return "border-cyan-300/40 bg-cyan-500/10 text-cyan-700";
    case "context_injected":
      return "border-violet-300/40 bg-violet-500/10 text-violet-700";
    default:
      return "border-border bg-secondary/30 text-muted-foreground";
  }
}

function ExecutionLogTimeline({
  entries,
  filterStepKey,
}: {
  entries: LifecycleExecutionEntry[];
  filterStepKey?: string | null;
}) {
  const [open, setOpen] = useState(false);
  const [stepFilter, setStepFilter] = useState<string | null>(filterStepKey ?? null);

  const filtered = useMemo(() => {
    if (!stepFilter) return entries;
    return entries.filter((e) => e.step_key === stepFilter);
  }, [entries, stepFilter]);

  const stepKeys = useMemo(
    () => [...new Set(entries.map((e) => e.step_key))],
    [entries],
  );

  if (entries.length === 0) return null;

  return (
    <div className="rounded-[12px] border border-border bg-background p-4">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center justify-between text-sm font-medium text-foreground"
      >
        <span>执行日志 ({entries.length})</span>
        <span className="text-xs text-muted-foreground">{open ? "▲ 收起" : "▼ 展开"}</span>
      </button>
      {open && (
        <div className="mt-3 space-y-2">
          {stepKeys.length > 1 && (
            <div className="flex flex-wrap gap-1.5">
              <button
                type="button"
                onClick={() => setStepFilter(null)}
                className={`rounded-full border px-2 py-0.5 text-[11px] transition-colors ${
                  stepFilter === null
                    ? "border-primary/30 bg-primary/10 text-primary"
                    : "border-border bg-secondary/40 text-muted-foreground hover:text-foreground"
                }`}
              >
                全部
              </button>
              {stepKeys.map((key) => (
                <button
                  key={key}
                  type="button"
                  onClick={() => setStepFilter(key)}
                  className={`rounded-full border px-2 py-0.5 text-[11px] transition-colors ${
                    stepFilter === key
                      ? "border-primary/30 bg-primary/10 text-primary"
                      : "border-border bg-secondary/40 text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {key}
                </button>
              ))}
            </div>
          )}
          <div className="relative space-y-0.5">
            {filtered.map((entry, index) => (
              <div
                key={`${entry.timestamp}-${index}`}
                className="flex items-start gap-3 rounded-lg px-2 py-1.5 hover:bg-secondary/20"
              >
                <span className="mt-0.5 shrink-0 text-[10px] tabular-nums text-muted-foreground/60">
                  {new Date(entry.timestamp).toLocaleTimeString()}
                </span>
                <span className={`mt-0.5 shrink-0 rounded-full border px-1.5 py-0.5 text-[10px] ${eventKindBadgeClass(entry.event_kind)}`}>
                  {EXECUTION_EVENT_KIND_LABEL[entry.event_kind] ?? entry.event_kind}
                </span>
                <div className="min-w-0 flex-1">
                  <p className="text-xs text-foreground/80">{entry.summary}</p>
                  {entry.detail && (
                    <pre className="mt-0.5 overflow-x-auto text-[10px] leading-4 text-muted-foreground/60">
                      {JSON.stringify(entry.detail, null, 2)}
                    </pre>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function CategorizedArtifacts({ artifacts }: { artifacts: WorkflowRecordArtifact[] }) {
  const groups = useMemo(() => categorizeArtifacts(artifacts), [artifacts]);

  if (artifacts.length === 0) return null;

  return (
    <div className="rounded-[12px] border border-border bg-background p-4">
      <p className="text-sm font-medium text-foreground">结构化记录产物</p>
      <div className="mt-3 space-y-4">
        {groups.map(({ category, artifacts: items }) => (
          <div key={category.label}>
            <p className="mb-2 text-xs font-medium text-muted-foreground">{category.label}</p>
            <div className="space-y-2">
              {items.map((artifact) => (
                <div
                  key={artifact.id}
                  className="rounded-[10px] border border-border bg-secondary/15 px-3 py-3"
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="rounded-full border border-primary/20 bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
                      {artifact.step_key || "unknown_step"}
                    </span>
                    <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[11px] text-muted-foreground">
                      {ARTIFACT_TYPE_LABEL[artifact.artifact_type] ?? artifact.artifact_type}
                    </span>
                    <span className="text-sm font-medium text-foreground">{artifact.title}</span>
                  </div>
                  <p className="mt-2 whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">
                    {artifact.content}
                  </p>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function ContextSnapshotViewer({ snapshot }: { snapshot: Record<string, unknown> }) {
  const [open, setOpen] = useState(false);

  return (
    <div className="mt-2">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="text-[11px] text-muted-foreground/60 transition-colors hover:text-muted-foreground"
      >
        {open ? "▲ 收起上下文快照" : "▶ 查看上下文快照"}
      </button>
      {open && (
        <pre className="mt-1.5 max-h-60 overflow-auto rounded-lg border border-border bg-secondary/20 p-3 text-[11px] leading-4 text-foreground/70">
          {JSON.stringify(snapshot, null, 2)}
        </pre>
      )}
    </div>
  );
}

export function TaskWorkflowPanel({
  task,
  projectId,
}: {
  task: Task;
  projectId: string;
}) {
  const definitions = useWorkflowStore((state) => state.definitions);
  const lifecycleDefinitions = useWorkflowStore((state) => state.lifecycleDefinitions);
  const runs = useWorkflowStore(
    (state) => state.runsByTargetKey[`task:${task.id}`] ?? EMPTY_RUNS,
  );
  const isLoading = useWorkflowStore((state) => state.isLoading);
  const error = useWorkflowStore((state) => state.error);
  const fetchDefinitions = useWorkflowStore((state) => state.fetchDefinitions);
  const fetchLifecycles = useWorkflowStore((state) => state.fetchLifecycles);
  const fetchRunsByTarget = useWorkflowStore((state) => state.fetchRunsByTarget);
  const startRun = useWorkflowStore((state) => state.startRun);
  const activateStep = useWorkflowStore((state) => state.activateStep);
  const completeStep = useWorkflowStore((state) => state.completeStep);

  const [message, setMessage] = useState<string | null>(null);
  const [stepSummary, setStepSummary] = useState("");
  const [taskSessionBinding, setTaskSessionBinding] = useState<SessionBindingOwner | null>(null);
  const [isResolvingBinding, setIsResolvingBinding] = useState(false);

  useEffect(() => {
    void fetchDefinitions("task");
    void fetchLifecycles("task");
    void fetchRunsByTarget("task", task.id);
  }, [fetchDefinitions, fetchLifecycles, fetchRunsByTarget, projectId, task.id, task.status, task.session_id]);

  useEffect(() => {
    if (!task.session_id) {
      setTaskSessionBinding(null);
      return;
    }

    let cancelled = false;
    setIsResolvingBinding(true);
    void (async () => {
      try {
        const bindings = await fetchSessionBindings(task.session_id ?? "");
        if (cancelled) return;
        const binding = bindings.find(
          (item) => item.owner_type === "task" && item.task_id === task.id,
        ) ?? null;
        setTaskSessionBinding(binding);
      } catch {
        if (!cancelled) {
          setTaskSessionBinding(null);
        }
      } finally {
        if (!cancelled) {
          setIsResolvingBinding(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [task.id, task.session_id]);

  const activeRun = useMemo(() => selectPreferredRun(runs), [runs]);
  const activeLifecycle = useMemo(
    () => (activeRun ? findLifecycle(lifecycleDefinitions, activeRun.lifecycle_id) : null),
    [activeRun, lifecycleDefinitions],
  );
  const currentStepState = useMemo(
    () =>
      activeRun?.current_step_key
        ? activeRun.step_states.find((item) => item.step_key === activeRun.current_step_key) ?? null
        : null,
    [activeRun],
  );
  const currentStepDefinition = useMemo(
    () =>
      activeLifecycle?.steps.find((item) => item.key === activeRun?.current_step_key) ?? null,
    [activeLifecycle, activeRun?.current_step_key],
  );
  const currentWorkflowDefinition = useMemo(
    () =>
      currentStepDefinition?.workflow_key?.trim()
        ? findWorkflowByKey(definitions, currentStepDefinition.workflow_key.trim())
        : null,
    [currentStepDefinition, definitions],
  );

  const handleStartRun = async () => {
    const taskLifecycle = lifecycleDefinitions.find(
      (l) => l.status === "active" && l.recommended_roles?.includes("task"),
    );
    if (!taskLifecycle) {
      setMessage("暂无可用的 Task Lifecycle 定义，请先在 Workflow 页面创建并激活。");
      return;
    }

    setMessage(null);
    const run = await startRun({
      lifecycle_id: taskLifecycle.id,
      target_kind: "task",
      target_id: task.id,
    });
    if (run) {
      setStepSummary("");
      setMessage("已启动 Task lifecycle run");
    }
  };

  const handleActivateStep = async () => {
    if (!activeRun?.current_step_key) return;

    setMessage(null);
    const run = await activateStep({
      run_id: activeRun.id,
      step_key: activeRun.current_step_key,
    });
    if (run) {
      setMessage(`已激活 ${stepHeading(activeLifecycle, activeRun.current_step_key)}`);
    }
  };

  const handleCompleteStep = async () => {
    if (!activeRun?.current_step_key) return;
    const summary = stepSummary.trim() || `完成 ${stepHeading(activeLifecycle, activeRun.current_step_key)}`;
    setMessage(null);
    const run = await completeStep({
      run_id: activeRun.id,
      step_key: activeRun.current_step_key,
      summary,
      record_artifacts: buildCompletionArtifacts(
        currentWorkflowDefinition,
        activeRun.current_step_key,
        summary,
      ),
    });
    if (run) {
      setStepSummary("");
      setMessage("当前步骤已完成");
    }
  };

  return (
    <div className="space-y-4">
      <div className="rounded-[12px] border border-border bg-secondary/20 p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium text-foreground">Lifecycle Run</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              按 lifecycle step 推进任务执行流程，并在每个 step 内挂接对应的 workflow 定义。
            </p>
          </div>
          <button
            type="button"
            onClick={() => void handleStartRun()}
            disabled={isLoading || Boolean(activeRun)}
            className="agentdash-button-primary"
          >
            {activeRun ? "已有运行中的 Lifecycle" : "启动 Task Lifecycle"}
          </button>
        </div>

        <div className="mt-3 flex flex-wrap gap-2">
          {activeRun ? (
            <span className="rounded-full border border-primary/30 bg-primary/10 px-2.5 py-1 text-xs text-primary">
              Lifecycle 运行中
            </span>
          ) : (
            <span className="rounded-full border border-border bg-secondary/40 px-2.5 py-1 text-xs text-muted-foreground">
              无活跃 Lifecycle
            </span>
          )}
          {task.session_id ? (
            <span className="rounded-full border border-emerald-300/40 bg-emerald-500/10 px-2.5 py-1 text-xs text-emerald-700">
              Task Session 已存在
            </span>
          ) : (
            <span className="rounded-full border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground">
              当前还没有 Task Session
            </span>
          )}
          {isResolvingBinding ? (
            <span className="rounded-full border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground">
              正在解析 SessionBinding...
            </span>
          ) : taskSessionBinding ? (
            <span className="rounded-full border border-cyan-300/40 bg-cyan-500/10 px-2.5 py-1 text-xs text-cyan-700">
              已绑定 SessionBinding
            </span>
          ) : null}
        </div>
      </div>

      {message && <p className="text-xs text-emerald-600">{message}</p>}
      {error && <p className="text-xs text-destructive">{error}</p>}

      {!activeRun ? (
        <div className="rounded-[12px] border border-dashed border-border bg-background px-4 py-8 text-center text-sm text-muted-foreground">
          当前 Task 还没有 lifecycle run。
        </div>
      ) : (
        <>
          <div className="rounded-[12px] border border-border bg-background p-4">
            <div className="flex flex-wrap items-center gap-2">
              <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                {activeLifecycle?.name ?? activeRun.lifecycle_id}
              </span>
              <span className="rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
                {RUN_STATUS_LABEL[activeRun.status] ?? activeRun.status}
              </span>
              {activeRun.current_step_key && (
                <span className="rounded-full border border-amber-300/40 bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-700">
                  当前步骤: {stepHeading(activeLifecycle, activeRun.current_step_key)}
                </span>
              )}
            </div>

            <div className="mt-4 grid gap-2">
              {activeRun.step_states.map((runStep) => {
                const def = stepDefinition(activeLifecycle, runStep.step_key);
                return (
                  <div
                    key={runStep.step_key}
                    className="flex flex-wrap items-center justify-between gap-3 rounded-[10px] border border-border bg-secondary/15 px-3 py-2"
                  >
                    <div>
                      <p className="text-sm font-medium text-foreground">
                        {def?.key ?? runStep.step_key}
                      </p>
                      {def?.description && (
                        <p className="mt-0.5 text-xs text-muted-foreground">{def.description}</p>
                      )}
                      {def && (
                        <p className="mt-1 text-[11px] text-muted-foreground/80">
                          {stepWorkflowModeLabel(def)}
                        </p>
                      )}
                      {runStep.summary && (
                        <p className="mt-1 text-xs text-muted-foreground">{runStep.summary}</p>
                      )}
                    </div>
                    <div className="flex items-center gap-2">
                      <span className={`rounded-full border px-2 py-0.5 text-[11px] ${stepBadgeClass(runStep.status)}`}>
                        {STEP_STATUS_LABEL[runStep.status] ?? runStep.status}
                      </span>
                    </div>
                    {runStep.context_snapshot && (
                      <ContextSnapshotViewer snapshot={runStep.context_snapshot} />
                    )}
                  </div>
                );
              })}
            </div>
          </div>

          {currentStepState && activeRun.current_step_key && (
            <div className="rounded-[12px] border border-border bg-background p-4">
              <p className="text-sm font-medium text-foreground">
                推进当前步骤: {stepHeading(activeLifecycle, activeRun.current_step_key)}
              </p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {currentStepDefinition?.description ?? "当前步骤暂无说明"}
              </p>
              {currentStepDefinition && (
                <div className="mt-3 flex flex-wrap gap-2">
                  <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                    {stepWorkflowModeLabel(currentStepDefinition)}
                  </span>
                  {currentWorkflowDefinition?.contract.injection.context_bindings.map((binding, index) => (
                    <span
                      key={`${binding.locator}-${index}`}
                      className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground"
                      title={`${binding.reason} · ${binding.locator}`}
                    >
                      {binding.title?.trim() || binding.locator}
                      {binding.required && <span className="ml-0.5 text-amber-600">*</span>}
                    </span>
                  ))}
                </div>
              )}
              {currentWorkflowDefinition?.contract.injection.instructions.length ? (
                <AgentInstructionsCollapsible
                  stepKey={currentStepDefinition?.key ?? activeRun.current_step_key}
                  instructions={currentWorkflowDefinition.contract.injection.instructions}
                />
              ) : null}
              <textarea
                value={stepSummary}
                onChange={(event) => setStepSummary(event.target.value)}
                rows={3}
                placeholder="填写当前步骤总结；留空时会自动生成默认总结。"
                className="agentdash-form-textarea mt-3"
              />

              <div className="mt-3 flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => void handleActivateStep()}
                  disabled={isLoading || currentStepState.status !== "ready"}
                  className="agentdash-button-secondary"
                >
                  激活当前步骤
                </button>
                <button
                  type="button"
                  onClick={() => void handleCompleteStep()}
                  disabled={
                    isLoading
                    || !["ready", "running"].includes(currentStepState.status)
                  }
                  className="agentdash-button-primary"
                >
                  完成当前步骤
                </button>
              </div>
            </div>
          )}

          <CategorizedArtifacts artifacts={activeRun.record_artifacts} />

          <ExecutionLogTimeline
            entries={activeRun.execution_log ?? []}
            filterStepKey={activeRun.current_step_key}
          />
        </>
      )}
    </div>
  );
}
