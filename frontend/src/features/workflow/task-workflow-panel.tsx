import { useEffect, useMemo, useState } from "react";

import type {
  SessionBindingOwner,
  Task,
  WorkflowAssignment,
  WorkflowContextBinding,
  WorkflowDefinition,
  WorkflowPhaseCompletionMode,
  WorkflowPhaseDefinition,
  WorkflowPhaseState,
  WorkflowRecordArtifactType,
  WorkflowRun,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import { fetchSessionBindings } from "../../services/session";

const EMPTY_ASSIGNMENTS: WorkflowAssignment[] = [];
const EMPTY_RUNS: WorkflowRun[] = [];

const COMPLETION_MODE_LABEL: Record<WorkflowPhaseCompletionMode, string> = {
  manual: "手动完成",
  session_ended: "会话结束后完成",
  checklist_passed: "检查通过后完成",
};

const BINDING_KIND_LABEL: Record<WorkflowContextBinding["kind"], string> = {
  document_path: "文档",
  runtime_context: "运行时上下文",
  checklist: "检查清单",
  journal_target: "记录目标",
  action_ref: "动作引用",
};

function findDefinition(definitions: WorkflowDefinition[], workflowId: string): WorkflowDefinition | null {
  return definitions.find((item) => item.id === workflowId) ?? null;
}

function phaseTitle(definition: WorkflowDefinition | null, phaseKey: string): string {
  return definition?.phases.find((item) => item.key === phaseKey)?.title ?? phaseKey;
}

function phaseBadgeClass(status: WorkflowPhaseState["status"]) {
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
  phase: WorkflowPhaseDefinition | null,
  phaseKey: string,
  summary: string,
): Array<{
  artifact_type: WorkflowRecordArtifactType;
  title: string;
  content: string;
}> {
  const trimmed = summary.trim();
  if (!trimmed) return [];
  const artifactType = phase?.default_artifact_type ?? "phase_note";
  const artifactTitle = phase?.default_artifact_title?.trim() || `${phaseKey} 阶段记录`;

  return [
    {
      artifact_type: artifactType,
      title: artifactTitle,
      content: trimmed,
    },
  ];
}

function selectPreferredRun(runs: WorkflowRun[]): WorkflowRun | null {
  return (
    runs.find((run) => run.status === "running")
    ?? runs.find((run) => run.status === "ready")
    ?? runs[0]
    ?? null
  );
}

function selectExecutionAssignment(assignments: WorkflowAssignment[]): WorkflowAssignment | null {
  const executionAssignments = assignments.filter(
    (item) => item.role === "task_execution_worker" && item.enabled,
  );
  return executionAssignments.find((item) => item.is_default) ?? executionAssignments[0] ?? null;
}

export function TaskWorkflowPanel({
  task,
  projectId,
}: {
  task: Task;
  projectId: string;
}) {
  const definitions = useWorkflowStore((state) => state.definitions);
  const assignments = useWorkflowStore(
    (state) => state.assignmentsByProjectId[projectId] ?? EMPTY_ASSIGNMENTS,
  );
  const runs = useWorkflowStore(
    (state) => state.runsByTargetKey[`task:${task.id}`] ?? EMPTY_RUNS,
  );
  const isLoading = useWorkflowStore((state) => state.isLoading);
  const error = useWorkflowStore((state) => state.error);
  const fetchDefinitions = useWorkflowStore((state) => state.fetchDefinitions);
  const fetchProjectAssignments = useWorkflowStore((state) => state.fetchProjectAssignments);
  const fetchRunsByTarget = useWorkflowStore((state) => state.fetchRunsByTarget);
  const startRun = useWorkflowStore((state) => state.startRun);
  const activatePhase = useWorkflowStore((state) => state.activatePhase);
  const completePhase = useWorkflowStore((state) => state.completePhase);

  const [message, setMessage] = useState<string | null>(null);
  const [phaseSummary, setPhaseSummary] = useState("");
  const [taskSessionBinding, setTaskSessionBinding] = useState<SessionBindingOwner | null>(null);
  const [isResolvingBinding, setIsResolvingBinding] = useState(false);

  useEffect(() => {
    void fetchDefinitions("task");
    void fetchProjectAssignments(projectId);
    void fetchRunsByTarget("task", task.id);
  }, [fetchDefinitions, fetchProjectAssignments, fetchRunsByTarget, projectId, task.id, task.status, task.session_id]);

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
  const activeDefinition = useMemo(
    () => (activeRun ? findDefinition(definitions, activeRun.workflow_id) : null),
    [activeRun, definitions],
  );
  const currentPhaseState = useMemo(
    () =>
      activeRun?.current_phase_key
        ? activeRun.phase_states.find((item) => item.phase_key === activeRun.current_phase_key) ?? null
        : null,
    [activeRun],
  );
  const currentPhaseDefinition = useMemo(
    () =>
      activeDefinition?.phases.find((item) => item.key === activeRun?.current_phase_key) ?? null,
    [activeDefinition, activeRun?.current_phase_key],
  );

  const handleStartRun = async () => {
    if (!activeAssignment) {
      setMessage("当前 Project 尚未配置默认 Task workflow，请先在项目详情里绑定。");
      return;
    }

    setMessage(null);
    const run = await startRun({
      workflow_id: activeAssignment.workflow_id,
      target_kind: "task",
      target_id: task.id,
    });
    if (run) {
      setPhaseSummary("");
      setMessage("已启动 Task workflow run");
    }
  };

  const handleActivatePhase = async () => {
    if (!activeRun?.current_phase_key) return;
    if (currentPhaseDefinition?.requires_session && !taskSessionBinding?.id) {
      setMessage("当前阶段需要 Task Session 绑定，请先启动 Task 会话。");
      return;
    }

    setMessage(null);
    const run = await activatePhase({
      run_id: activeRun.id,
      phase_key: activeRun.current_phase_key,
      session_binding_id: currentPhaseDefinition?.requires_session ? taskSessionBinding?.id ?? undefined : undefined,
    });
    if (run) {
      setMessage(`已激活 ${phaseTitle(activeDefinition, activeRun.current_phase_key)}`);
    }
  };

  const handleCompletePhase = async () => {
    if (!activeRun?.current_phase_key) return;
    const summary = phaseSummary.trim() || `完成 ${phaseTitle(activeDefinition, activeRun.current_phase_key)}`;
    setMessage(null);
    const run = await completePhase({
      run_id: activeRun.id,
      phase_key: activeRun.current_phase_key,
      summary,
      record_artifacts: buildCompletionArtifacts(
        currentPhaseDefinition,
        activeRun.current_phase_key,
        summary,
      ),
    });
    if (run) {
      setPhaseSummary("");
      setMessage("当前阶段已完成");
    }
  };

  return (
    <div className="space-y-4">
      <div className="rounded-[12px] border border-border bg-secondary/20 p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium text-foreground">Workflow Run</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              这里把 Task 执行会话正式挂到 workflow phase 上，并让阶段约束真正进入 Agent prompt。
            </p>
          </div>
          <button
            type="button"
            onClick={() => void handleStartRun()}
            disabled={isLoading || Boolean(activeRun)}
            className="agentdash-button-primary"
          >
            {activeRun ? "已有运行中的 Workflow" : "启动 Task Workflow"}
          </button>
        </div>

        <div className="mt-3 flex flex-wrap gap-2">
          {activeAssignment ? (
            <span className="rounded-full border border-primary/30 bg-primary/10 px-2.5 py-1 text-xs text-primary">
              默认流程已绑定
            </span>
          ) : (
            <span className="rounded-full border border-amber-300/40 bg-amber-500/10 px-2.5 py-1 text-xs text-amber-700">
              Project 尚未配置默认 Task workflow
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
          当前 Task 还没有 workflow run。
        </div>
      ) : (
        <>
          <div className="rounded-[12px] border border-border bg-background p-4">
            <div className="flex flex-wrap items-center gap-2">
              <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                {activeDefinition?.name ?? activeRun.workflow_id}
              </span>
              <span className="rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
                {activeRun.status}
              </span>
              {activeRun.current_phase_key && (
                <span className="rounded-full border border-amber-300/40 bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-700">
                  当前阶段: {phaseTitle(activeDefinition, activeRun.current_phase_key)}
                </span>
              )}
            </div>

            <div className="mt-4 grid gap-2">
              {activeRun.phase_states.map((phase) => (
                <div
                  key={phase.phase_key}
                  className="flex flex-wrap items-center justify-between gap-3 rounded-[10px] border border-border bg-secondary/15 px-3 py-2"
                >
                  <div>
                    <p className="text-sm font-medium text-foreground">
                      {phaseTitle(activeDefinition, phase.phase_key)}
                    </p>
                    {phase.summary && (
                      <p className="mt-1 text-xs text-muted-foreground">{phase.summary}</p>
                    )}
                  </div>
                  <div className="flex items-center gap-2">
                    {phase.session_binding_id && (
                      <span className="rounded-full border border-cyan-300/40 bg-cyan-500/10 px-2 py-0.5 text-[11px] text-cyan-700">
                        session 已挂接
                      </span>
                    )}
                    <span className={`rounded-full border px-2 py-0.5 text-[11px] ${phaseBadgeClass(phase.status)}`}>
                      {phase.status}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {currentPhaseState && activeRun.current_phase_key && (
            <div className="rounded-[12px] border border-border bg-background p-4">
              <p className="text-sm font-medium text-foreground">
                推进当前阶段: {phaseTitle(activeDefinition, activeRun.current_phase_key)}
              </p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {currentPhaseDefinition?.description ?? "当前阶段暂无说明"}
              </p>
              {currentPhaseDefinition && (
                <div className="mt-3 flex flex-wrap gap-2">
                  <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                    完成方式: {COMPLETION_MODE_LABEL[currentPhaseDefinition.completion_mode]}
                  </span>
                  {currentPhaseDefinition.context_bindings.map((binding, index) => (
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
              {currentPhaseDefinition?.agent_instructions.length ? (
                <div className="mt-3 rounded-[10px] border border-border bg-secondary/20 p-3">
                  <p className="text-xs font-medium text-muted-foreground">自动注入给 Agent 的阶段约束</p>
                  <div className="mt-2 space-y-1 text-xs leading-5 text-foreground/80">
                    {currentPhaseDefinition.agent_instructions.map((instruction, index) => (
                      <p key={`${currentPhaseDefinition.key}-instruction-${index}`}>- {instruction}</p>
                    ))}
                  </div>
                </div>
              ) : null}
              {currentPhaseDefinition?.requires_session && !task.session_id && (
                <p className="mt-2 text-xs text-amber-700">
                  该阶段要求先有 Task Session。请先在上方“Agent 执行会话”里启动任务执行。
                </p>
              )}
              {currentPhaseDefinition?.requires_session && task.session_id && !taskSessionBinding && !isResolvingBinding && (
                <p className="mt-2 text-xs text-amber-700">
                  已检测到 Task Session，但还没有解析到对应的 SessionBinding。
                </p>
              )}

              <textarea
                value={phaseSummary}
                onChange={(event) => setPhaseSummary(event.target.value)}
                rows={3}
                placeholder="填写当前阶段总结；留空时会自动生成默认总结。"
                className="agentdash-form-textarea mt-3"
              />

              <div className="mt-3 flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => void handleActivatePhase()}
                  disabled={
                    isLoading
                    || currentPhaseState.status !== "ready"
                    || Boolean(currentPhaseDefinition?.requires_session && !taskSessionBinding)
                  }
                  className="agentdash-button-secondary"
                >
                  激活当前阶段
                </button>
                <button
                  type="button"
                  onClick={() => void handleCompletePhase()}
                  disabled={
                    isLoading
                    || !["ready", "running"].includes(currentPhaseState.status)
                    || Boolean(currentPhaseDefinition?.requires_session && !taskSessionBinding)
                  }
                  className="agentdash-button-primary"
                >
                  完成当前阶段
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
