import { useEffect, useMemo, useState } from "react";

import type { WorkflowAssignment, WorkflowDefinition, WorkflowTargetKind } from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";

const EMPTY_ASSIGNMENTS: WorkflowAssignment[] = [];

const TARGET_KIND_LABEL: Record<WorkflowTargetKind, string> = {
  project: "Project",
  story: "Story",
  task: "Task",
};

function DefinitionCard({
  definition,
  isAssigned,
  isDefault,
  isAssigning,
  onAssign,
}: {
  definition: WorkflowDefinition;
  isAssigned: boolean;
  isDefault: boolean;
  isAssigning: boolean;
  onAssign: () => void;
}) {
  return (
    <div className="rounded-[12px] border border-border bg-background p-4">
      <div className="flex flex-wrap items-center gap-2">
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          {TARGET_KIND_LABEL[definition.target_kind]}
        </span>
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          v{definition.version}
        </span>
        {definition.enabled && (
          <span className="rounded-full border border-emerald-300/40 bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-600">
            已启用
          </span>
        )}
        {isAssigned && (
          <span className="rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
            已绑定到当前 Project
          </span>
        )}
        {isDefault && (
          <span className="rounded-full border border-amber-300/40 bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-700">
            默认执行流程
          </span>
        )}
      </div>

      <div className="mt-3">
        <p className="text-sm font-medium text-foreground">{definition.name}</p>
        <p className="mt-1 text-xs text-muted-foreground">{definition.key}</p>
        <p className="mt-2 text-sm leading-6 text-foreground/80">{definition.description}</p>
      </div>

      <div className="mt-3 rounded-[10px] border border-border bg-secondary/20 p-3">
        <p className="text-xs font-medium text-muted-foreground">Phase</p>
        <div className="mt-2 flex flex-wrap gap-2">
          {definition.phases.map((phase) => (
            <span
              key={phase.key}
              className="rounded-full border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground"
            >
              {phase.title}
            </span>
          ))}
        </div>
      </div>

      {definition.target_kind === "task" && (
        <div className="mt-4 flex justify-end">
          <button
            type="button"
            onClick={onAssign}
            disabled={isAssigning}
            className={isDefault ? "agentdash-button-secondary" : "agentdash-button-primary"}
          >
            {isAssigning ? "保存中..." : isDefault ? "重新设为默认" : "设为 Task 默认流程"}
          </button>
        </div>
      )}
    </div>
  );
}

export function ProjectWorkflowPanel({ projectId }: { projectId: string }) {
  const definitions = useWorkflowStore((state) => state.definitions);
  const assignments = useWorkflowStore(
    (state) => state.assignmentsByProjectId[projectId] ?? EMPTY_ASSIGNMENTS,
  );
  const isLoading = useWorkflowStore((state) => state.isLoading);
  const error = useWorkflowStore((state) => state.error);
  const fetchDefinitions = useWorkflowStore((state) => state.fetchDefinitions);
  const fetchProjectAssignments = useWorkflowStore((state) => state.fetchProjectAssignments);
  const bootstrapTrellis = useWorkflowStore((state) => state.bootstrapTrellis);
  const assignWorkflowToProject = useWorkflowStore((state) => state.assignWorkflowToProject);

  const [message, setMessage] = useState<string | null>(null);
  const [bootstrappingTarget, setBootstrappingTarget] = useState<WorkflowTargetKind | null>(null);
  const [assigningWorkflowId, setAssigningWorkflowId] = useState<string | null>(null);

  useEffect(() => {
    void fetchDefinitions();
    void fetchProjectAssignments(projectId);
  }, [fetchDefinitions, fetchProjectAssignments, projectId]);

  const taskAssignments = useMemo(
    () => assignments.filter((item) => item.role === "task_execution_worker"),
    [assignments],
  );
  const defaultTaskAssignment = useMemo(
    () => taskAssignments.find((item) => item.is_default) ?? taskAssignments[0] ?? null,
    [taskAssignments],
  );

  const handleBootstrap = async (targetKind: WorkflowTargetKind) => {
    setMessage(null);
    setBootstrappingTarget(targetKind);
    try {
      const definition = await bootstrapTrellis(targetKind);
      if (definition) {
        setMessage(`已注册 ${TARGET_KIND_LABEL[targetKind]} Trellis Workflow`);
      }
    } finally {
      setBootstrappingTarget(null);
    }
  };

  const handleAssign = async (definition: WorkflowDefinition) => {
    setMessage(null);
    setAssigningWorkflowId(definition.id);
    try {
      const assignment = await assignWorkflowToProject({
        project_id: projectId,
        workflow_id: definition.id,
        role: "task_execution_worker",
        enabled: true,
        is_default: true,
      });
      if (assignment) {
        setMessage(`已将 ${definition.name} 设为当前 Project 的默认 Task 流程`);
      }
    } finally {
      setAssigningWorkflowId(null);
    }
  };

  return (
    <div className="space-y-4">
      <div className="rounded-[12px] border border-border bg-secondary/20 p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium text-foreground">Workflow 平台接线</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              当前 Project 可以绑定默认的 Task 执行流程。第一条真实流程是 Trellis Dev Workflow。
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            {(["project", "story", "task"] as WorkflowTargetKind[]).map((targetKind) => (
              <button
                key={targetKind}
                type="button"
                onClick={() => void handleBootstrap(targetKind)}
                disabled={isLoading}
                className="agentdash-button-secondary"
              >
                {bootstrappingTarget === targetKind
                  ? `注册 ${TARGET_KIND_LABEL[targetKind]} 中...`
                  : `注册 ${TARGET_KIND_LABEL[targetKind]} Trellis`}
              </button>
            ))}
          </div>
        </div>

        <div className="mt-3 flex flex-wrap gap-2">
          {defaultTaskAssignment ? (
            <span className="rounded-full border border-primary/30 bg-primary/10 px-2.5 py-1 text-xs text-primary">
              当前默认 Task 流程: {defaultTaskAssignment.workflow_id}
            </span>
          ) : (
            <span className="rounded-full border border-amber-300/40 bg-amber-500/10 px-2.5 py-1 text-xs text-amber-700">
              当前尚未设置默认 Task 流程
            </span>
          )}
        </div>
      </div>

      {message && <p className="text-xs text-emerald-600">{message}</p>}
      {error && <p className="text-xs text-destructive">{error}</p>}

      <div className="grid gap-3">
        {definitions.length === 0 ? (
          <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-4 py-8 text-center text-sm text-muted-foreground">
            还没有可用的 workflow definition，请先注册内置 Trellis Workflow。
          </div>
        ) : (
          definitions
            .slice()
            .sort((a, b) => a.name.localeCompare(b.name, "zh-CN"))
            .map((definition) => {
              const relatedAssignment = assignments.find(
                (assignment) => assignment.workflow_id === definition.id,
              );
              return (
                <DefinitionCard
                  key={definition.id}
                  definition={definition}
                  isAssigned={Boolean(relatedAssignment)}
                  isDefault={defaultTaskAssignment?.workflow_id === definition.id}
                  isAssigning={assigningWorkflowId === definition.id}
                  onAssign={() => void handleAssign(definition)}
                />
              );
            })
        )}
      </div>
    </div>
  );
}
