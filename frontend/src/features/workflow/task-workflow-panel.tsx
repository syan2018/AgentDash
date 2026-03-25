import { useEffect, useMemo, useState } from "react";

import type {
  LifecycleDefinition,
  LifecycleStepDefinition,
  SessionBindingOwner,
  Task,
  WorkflowAssignment,
  WorkflowDefinition,
  WorkflowRecordArtifactType,
  WorkflowRun,
  WorkflowStepState,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import { fetchSessionBindings } from "../../services/session";
import {
  BINDING_KIND_LABEL,
  RUN_STATUS_LABEL,
  STEP_STATUS_LABEL,
  TRANSITION_POLICY_LABEL,
} from "./shared-labels";

const EMPTY_ASSIGNMENTS: WorkflowAssignment[] = [];
const EMPTY_RUNS: WorkflowRun[] = [];

function findLifecycle(lifecycles: LifecycleDefinition[], lifecycleId: string): LifecycleDefinition | null {
  return lifecycles.find((item) => item.id === lifecycleId) ?? null;
}

function findWorkflowByKey(definitions: WorkflowDefinition[], workflowKey: string): WorkflowDefinition | null {
  return definitions.find((item) => item.key === workflowKey) ?? null;
}

function stepTitle(lifecycle: LifecycleDefinition | null, stepKey: string): string {
  return lifecycle?.steps.find((item) => item.key === stepKey)?.title ?? stepKey;
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

function selectExecutionAssignment(assignments: WorkflowAssignment[]): WorkflowAssignment | null {
  const executionAssignments = assignments.filter(
    (item) => item.role === "task_execution_worker" && item.enabled,
  );
  return executionAssignments.find((item) => item.is_default) ?? executionAssignments[0] ?? null;
}

function stepRequiresSession(
  step: LifecycleStepDefinition | null,
  workflow: WorkflowDefinition | null,
): boolean {
  return (
    step?.session_binding === "required"
    || workflow?.contract.injection.session_binding === "required"
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
  const assignments = useWorkflowStore(
    (state) => state.assignmentsByProjectId[projectId] ?? EMPTY_ASSIGNMENTS,
  );
  const runs = useWorkflowStore(
    (state) => state.runsByTargetKey[`task:${task.id}`] ?? EMPTY_RUNS,
  );
  const isLoading = useWorkflowStore((state) => state.isLoading);
  const error = useWorkflowStore((state) => state.error);
  const fetchDefinitions = useWorkflowStore((state) => state.fetchDefinitions);
  const fetchLifecycles = useWorkflowStore((state) => state.fetchLifecycles);
  const fetchProjectAssignments = useWorkflowStore((state) => state.fetchProjectAssignments);
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
    void fetchProjectAssignments(projectId);
    void fetchRunsByTarget("task", task.id);
  }, [fetchDefinitions, fetchLifecycles, fetchProjectAssignments, fetchRunsByTarget, projectId, task.id, task.status, task.session_id]);

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

  const activeAssignment = useMemo(() => selectExecutionAssignment(assignments), [assignments]);
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
      currentStepDefinition
        ? findWorkflowByKey(definitions, currentStepDefinition.primary_workflow_key)
        : null,
    [currentStepDefinition, definitions],
  );
  const requiresSession = stepRequiresSession(currentStepDefinition, currentWorkflowDefinition);

  const handleStartRun = async () => {
    if (!activeAssignment) {
      setMessage("当前 Project 尚未配置默认 Task lifecycle，请先在项目详情里绑定。");
      return;
    }

    setMessage(null);
    const run = await startRun({
      lifecycle_id: activeAssignment.lifecycle_id,
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
    if (requiresSession && !taskSessionBinding?.id) {
      setMessage("当前步骤需要 Task Session 绑定，请先启动 Task 会话。");
      return;
    }

    setMessage(null);
    const run = await activateStep({
      run_id: activeRun.id,
      step_key: activeRun.current_step_key,
      session_binding_id: requiresSession ? taskSessionBinding?.id ?? undefined : undefined,
    });
    if (run) {
      setMessage(`已激活 ${stepTitle(activeLifecycle, activeRun.current_step_key)}`);
    }
  };

  const handleCompleteStep = async () => {
    if (!activeRun?.current_step_key) return;
    const summary = stepSummary.trim() || `完成 ${stepTitle(activeLifecycle, activeRun.current_step_key)}`;
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
          {activeAssignment ? (
            <span className="rounded-full border border-primary/30 bg-primary/10 px-2.5 py-1 text-xs text-primary">
              默认生命周期已绑定
            </span>
          ) : (
            <span className="rounded-full border border-amber-300/40 bg-amber-500/10 px-2.5 py-1 text-xs text-amber-700">
              Project 尚未配置默认 Task lifecycle
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
                  当前步骤: {stepTitle(activeLifecycle, activeRun.current_step_key)}
                </span>
              )}
            </div>

            <div className="mt-4 grid gap-2">
              {activeRun.step_states.map((step) => (
                <div
                  key={step.step_key}
                  className="flex flex-wrap items-center justify-between gap-3 rounded-[10px] border border-border bg-secondary/15 px-3 py-2"
                >
                  <div>
                    <p className="text-sm font-medium text-foreground">
                      {stepTitle(activeLifecycle, step.step_key)}
                    </p>
                    {step.summary && (
                      <p className="mt-1 text-xs text-muted-foreground">{step.summary}</p>
                    )}
                  </div>
                  <div className="flex items-center gap-2">
                    {step.session_binding_id && (
                      <span className="rounded-full border border-cyan-300/40 bg-cyan-500/10 px-2 py-0.5 text-[11px] text-cyan-700">
                        session 已挂接
                      </span>
                    )}
                    <span className={`rounded-full border px-2 py-0.5 text-[11px] ${stepBadgeClass(step.status)}`}>
                      {STEP_STATUS_LABEL[step.status] ?? step.status}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {currentStepState && activeRun.current_step_key && (
            <div className="rounded-[12px] border border-border bg-background p-4">
              <p className="text-sm font-medium text-foreground">
                推进当前步骤: {stepTitle(activeLifecycle, activeRun.current_step_key)}
              </p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {currentStepDefinition?.description ?? "当前步骤暂无说明"}
              </p>
              {currentStepDefinition && (
                <div className="mt-3 flex flex-wrap gap-2">
                  <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                    推进策略: {TRANSITION_POLICY_LABEL[currentStepDefinition.transition_policy]}
                  </span>
                  <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                    主 Workflow: {currentStepDefinition.primary_workflow_key}
                  </span>
                  {currentWorkflowDefinition?.contract.injection.context_bindings.map((binding, index) => (
                    <span
                      key={`${binding.locator}-${index}`}
                      className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground"
                      title={`${binding.reason} · ${binding.locator}`}
                    >
                      {BINDING_KIND_LABEL[binding.kind]}: {binding.title?.trim() || binding.locator}
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
              {requiresSession && !task.session_id && (
                <p className="mt-2 text-xs text-amber-700">
                  该步骤要求先有 Task Session。请先在上方“Agent 执行会话”里启动任务执行。
                </p>
              )}
              {requiresSession && task.session_id && !taskSessionBinding && !isResolvingBinding && (
                <p className="mt-2 text-xs text-amber-700">
                  已检测到 Task Session，但还没有解析到对应的 SessionBinding。
                </p>
              )}

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
                  disabled={
                    isLoading
                    || currentStepState.status !== "ready"
                    || Boolean(requiresSession && !taskSessionBinding)
                  }
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
                    || Boolean(requiresSession && !taskSessionBinding)
                  }
                  className="agentdash-button-primary"
                >
                  完成当前步骤
                </button>
              </div>
            </div>
          )}

          {activeRun.record_artifacts.length > 0 && (
            <div className="rounded-[12px] border border-border bg-background p-4">
              <p className="text-sm font-medium text-foreground">结构化记录产物</p>
              <div className="mt-3 space-y-2">
                {activeRun.record_artifacts.map((artifact) => (
                  <div
                    key={artifact.id}
                    className="rounded-[10px] border border-border bg-secondary/15 px-3 py-3"
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="rounded-full border border-primary/20 bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
                        {artifact.step_key || "unknown_step"}
                      </span>
                      <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[11px] text-muted-foreground">
                        {artifact.artifact_type}
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
          )}
        </>
      )}
    </div>
  );
}
