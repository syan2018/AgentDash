import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";

import type {
  LifecycleDefinition,
  LifecycleStepDefinition,
  WorkflowAgentRole,
  WorkflowAssignment,
  WorkflowDefinition,
  WorkflowTemplate,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import {
  DEFINITION_STATUS_LABEL,
  DEFAULT_ROLE_BY_TARGET,
  ROLE_LABEL,
  ROLE_ORDER,
  TARGET_KIND_LABEL,
  TRANSITION_POLICY_LABEL,
} from "./shared-labels";

const EMPTY_ASSIGNMENTS: WorkflowAssignment[] = [];

function resolveLifecycleRole(lifecycle: LifecycleDefinition): WorkflowAgentRole {
  return lifecycle.recommended_role ?? DEFAULT_ROLE_BY_TARGET[lifecycle.target_kind];
}

function resolveWorkflowRole(definition: WorkflowDefinition): WorkflowAgentRole {
  return definition.recommended_role ?? DEFAULT_ROLE_BY_TARGET[definition.target_kind];
}

function StepSummary({ step }: { step: LifecycleStepDefinition }) {
  return (
    <div className="rounded-[10px] border border-border bg-background px-3 py-2 text-[11px]">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium text-foreground">{step.title}</span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
          {TRANSITION_POLICY_LABEL[step.transition_policy]}
        </span>
        {step.session_binding !== "not_required" && (
          <span className="rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
            {step.session_binding === "required" ? "需要 Session" : "可挂接 Session"}
          </span>
        )}
      </div>
      <p className="mt-1 text-[10px] leading-5 text-muted-foreground">
        {step.primary_workflow_key}
        {step.next_step_key ? ` -> ${step.next_step_key}` : ""}
      </p>
      {step.description && (
        <p className="mt-1 text-[10px] leading-5 text-foreground/65">{step.description}</p>
      )}
    </div>
  );
}

function TemplateCard({
  template,
  isRegistered,
  isBootstrapping,
  onBootstrap,
}: {
  template: WorkflowTemplate;
  isRegistered: boolean;
  isBootstrapping: boolean;
  onBootstrap: () => void;
}) {
  return (
    <div className="rounded-[12px] border border-border bg-background p-4">
      <div className="flex flex-wrap items-center gap-2">
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          {TARGET_KIND_LABEL[template.target_kind]}
        </span>
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          {ROLE_LABEL[template.recommended_role]}
        </span>
        {isRegistered && (
          <span className="rounded-full border border-emerald-300/40 bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-600">
            Lifecycle 已注册
          </span>
        )}
      </div>

      <div className="mt-3">
        <p className="text-sm font-medium text-foreground">{template.name}</p>
        <p className="mt-1 text-xs text-muted-foreground">{template.key}</p>
        <p className="mt-2 text-sm leading-6 text-foreground/80">{template.description}</p>
      </div>

      <div className="mt-3 rounded-[10px] border border-border bg-secondary/20 p-3">
        <p className="text-xs font-medium text-muted-foreground">
          Bundle 内容
        </p>
        <p className="mt-1 text-[11px] text-muted-foreground">
          {template.workflows.length} 个 Workflow 定义 · {template.lifecycle.steps.length} 个 Lifecycle Step
        </p>
        <div className="mt-2 grid gap-2">
          {template.lifecycle.steps.map((step) => (
            <StepSummary key={step.key} step={step} />
          ))}
        </div>
      </div>

      <div className="mt-4 flex justify-end">
        <button
          type="button"
          onClick={onBootstrap}
          disabled={isBootstrapping || isRegistered}
          className={isRegistered ? "agentdash-button-secondary" : "agentdash-button-primary"}
        >
          {isBootstrapping ? "注册中..." : isRegistered ? "已注册" : "注册 Lifecycle Bundle"}
        </button>
      </div>
    </div>
  );
}

function LifecycleCard({
  lifecycle,
  role,
  isAssigned,
  isDefault,
  isAssigning,
  onEdit,
  onEnable,
  onDisable,
  onAssign,
}: {
  lifecycle: LifecycleDefinition;
  role: WorkflowAgentRole;
  isAssigned: boolean;
  isDefault: boolean;
  isAssigning: boolean;
  onEdit: () => void;
  onEnable: () => void;
  onDisable: () => void;
  onAssign: () => void;
}) {
  return (
    <div className="rounded-[12px] border border-border bg-background p-4">
      <div className="flex flex-wrap items-center gap-2">
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          {TARGET_KIND_LABEL[lifecycle.target_kind]}
        </span>
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          {ROLE_LABEL[role]}
        </span>
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          v{lifecycle.version}
        </span>
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          {DEFINITION_STATUS_LABEL[lifecycle.status]}
        </span>
        {isAssigned && (
          <span className="rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
            已绑定到当前 Project
          </span>
        )}
        {isDefault && (
          <span className="rounded-full border border-amber-300/40 bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-700">
            当前角色默认生命周期
          </span>
        )}
      </div>

      <div className="mt-3">
        <p className="text-sm font-medium text-foreground">{lifecycle.name}</p>
        <p className="mt-1 text-xs text-muted-foreground">{lifecycle.key}</p>
        <p className="mt-2 text-sm leading-6 text-foreground/80">{lifecycle.description}</p>
      </div>

      <div className="mt-3 rounded-[10px] border border-border bg-secondary/20 p-3">
        <p className="text-xs font-medium text-muted-foreground">Lifecycle Steps</p>
        <div className="mt-2 grid gap-2">
          {lifecycle.steps.map((step) => (
            <StepSummary key={step.key} step={step} />
          ))}
        </div>
      </div>

      <div className="mt-4 flex flex-wrap justify-end gap-2">
        <button type="button" onClick={onEdit} className="agentdash-button-secondary text-sm">
          编辑 Lifecycle
        </button>
        {lifecycle.status === "active" ? (
          <button
            type="button"
            onClick={onDisable}
            className="rounded-[10px] border border-amber-300/30 bg-amber-500/5 px-3 py-1.5 text-sm text-amber-700 transition-colors hover:bg-amber-500/10"
          >
            停用
          </button>
        ) : (
          <button
            type="button"
            onClick={onEnable}
            className="rounded-[10px] border border-emerald-300/30 bg-emerald-500/5 px-3 py-1.5 text-sm text-emerald-700 transition-colors hover:bg-emerald-500/10"
          >
            激活
          </button>
        )}
        <button
          type="button"
          onClick={onAssign}
          disabled={isAssigning}
          className={isDefault ? "agentdash-button-secondary" : "agentdash-button-primary"}
        >
          {isAssigning ? "保存中..." : isDefault ? "重新设为默认" : "设为当前角色默认生命周期"}
        </button>
      </div>
    </div>
  );
}

function WorkflowContractCard({
  definition,
  role,
  onEdit,
  onEnable,
  onDisable,
}: {
  definition: WorkflowDefinition;
  role: WorkflowAgentRole;
  onEdit: () => void;
  onEnable: () => void;
  onDisable: () => void;
}) {
  return (
    <div className="rounded-[12px] border border-border bg-background p-4">
      <div className="flex flex-wrap items-center gap-2">
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          {TARGET_KIND_LABEL[definition.target_kind]}
        </span>
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          {ROLE_LABEL[role]}
        </span>
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          v{definition.version}
        </span>
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
          {DEFINITION_STATUS_LABEL[definition.status]}
        </span>
      </div>

      <div className="mt-3">
        <p className="text-sm font-medium text-foreground">{definition.name}</p>
        <p className="mt-1 text-xs text-muted-foreground">{definition.key}</p>
        <p className="mt-2 text-sm leading-6 text-foreground/80">{definition.description}</p>
      </div>

      <div className="mt-3 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5">
          bindings {definition.contract.injection.context_bindings.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5">
          constraints {definition.contract.hook_policy.constraints.length}
        </span>
        <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5">
          checks {definition.contract.completion.checks.length}
        </span>
      </div>

      <div className="mt-4 flex flex-wrap justify-end gap-2">
        <button type="button" onClick={onEdit} className="agentdash-button-secondary text-sm">
          编辑 Workflow
        </button>
        {definition.status === "active" ? (
          <button
            type="button"
            onClick={onDisable}
            className="rounded-[10px] border border-amber-300/30 bg-amber-500/5 px-3 py-1.5 text-sm text-amber-700 transition-colors hover:bg-amber-500/10"
          >
            停用
          </button>
        ) : (
          <button
            type="button"
            onClick={onEnable}
            className="rounded-[10px] border border-emerald-300/30 bg-emerald-500/5 px-3 py-1.5 text-sm text-emerald-700 transition-colors hover:bg-emerald-500/10"
          >
            激活
          </button>
        )}
      </div>
    </div>
  );
}

export function ProjectWorkflowPanel({ projectId }: { projectId: string }) {
  const navigate = useNavigate();
  const templates = useWorkflowStore((state) => state.templates);
  const definitions = useWorkflowStore((state) => state.definitions);
  const lifecycleDefinitions = useWorkflowStore((state) => state.lifecycleDefinitions);
  const assignments = useWorkflowStore(
    (state) => state.assignmentsByProjectId[projectId] ?? EMPTY_ASSIGNMENTS,
  );
  const error = useWorkflowStore((state) => state.error);
  const fetchTemplates = useWorkflowStore((state) => state.fetchTemplates);
  const fetchDefinitions = useWorkflowStore((state) => state.fetchDefinitions);
  const fetchLifecycles = useWorkflowStore((state) => state.fetchLifecycles);
  const fetchProjectAssignments = useWorkflowStore((state) => state.fetchProjectAssignments);
  const bootstrapTemplate = useWorkflowStore((state) => state.bootstrapTemplate);
  const assignLifecycleToProject = useWorkflowStore((state) => state.assignLifecycleToProject);
  const enableDefinition = useWorkflowStore((state) => state.enableDefinition);
  const disableDefinition = useWorkflowStore((state) => state.disableDefinition);
  const enableLifecycle = useWorkflowStore((state) => state.enableLifecycle);
  const disableLifecycle = useWorkflowStore((state) => state.disableLifecycle);

  const [message, setMessage] = useState<string | null>(null);
  const [bootstrappingTemplateKey, setBootstrappingTemplateKey] = useState<string | null>(null);
  const [assigningKey, setAssigningKey] = useState<string | null>(null);

  useEffect(() => {
    void fetchTemplates();
    void fetchDefinitions();
    void fetchLifecycles();
    void fetchProjectAssignments(projectId);
  }, [fetchDefinitions, fetchLifecycles, fetchProjectAssignments, fetchTemplates, projectId]);

  const defaultAssignmentsByRole = useMemo(() => {
    const entries = ROLE_ORDER.map((role) => {
      const roleAssignments = assignments.filter((item) => item.role === role && item.enabled);
      const activeAssignment = roleAssignments.find((item) => item.is_default) ?? roleAssignments[0] ?? null;
      const lifecycle = activeAssignment
        ? lifecycleDefinitions.find((item) => item.id === activeAssignment.lifecycle_id) ?? null
        : null;
      return [role, lifecycle] as const;
    });
    return new Map(entries);
  }, [assignments, lifecycleDefinitions]);

  const lifecyclesByRole = useMemo(() => {
    const grouped = new Map<WorkflowAgentRole, LifecycleDefinition[]>();
    for (const role of ROLE_ORDER) {
      grouped.set(role, []);
    }
    for (const lifecycle of lifecycleDefinitions) {
      const role = resolveLifecycleRole(lifecycle);
      grouped.set(role, [...(grouped.get(role) ?? []), lifecycle]);
    }
    return grouped;
  }, [lifecycleDefinitions]);

  const contractsByRole = useMemo(() => {
    const grouped = new Map<WorkflowAgentRole, WorkflowDefinition[]>();
    for (const role of ROLE_ORDER) {
      grouped.set(role, []);
    }
    for (const definition of definitions) {
      const role = resolveWorkflowRole(definition);
      grouped.set(role, [...(grouped.get(role) ?? []), definition]);
    }
    return grouped;
  }, [definitions]);

  const handleBootstrap = async (template: WorkflowTemplate) => {
    setMessage(null);
    setBootstrappingTemplateKey(template.key);
    try {
      const lifecycle = await bootstrapTemplate(template.key);
      if (lifecycle) {
        setMessage(`已注册 lifecycle bundle：${template.name}`);
      }
    } finally {
      setBootstrappingTemplateKey(null);
    }
  };

  const handleAssign = async (lifecycle: LifecycleDefinition, role: WorkflowAgentRole) => {
    setMessage(null);
    setAssigningKey(`${lifecycle.id}:${role}`);
    try {
      const assignment = await assignLifecycleToProject({
        project_id: projectId,
        lifecycle_id: lifecycle.id,
        role,
        enabled: true,
        is_default: true,
      });
      if (assignment) {
        setMessage(`已将 ${lifecycle.name} 设为 ${ROLE_LABEL[role]} 的默认生命周期`);
      }
    } finally {
      setAssigningKey(null);
    }
  };

  return (
    <div className="space-y-4">
      <div className="rounded-[12px] border border-border bg-secondary/20 p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium text-foreground">Workflow 定义 / Lifecycles</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              Workflow 负责定义注入、Hook 策略与完成检查，Lifecycle 负责为项目角色编排可执行步骤。
              <button
                type="button"
                onClick={() => navigate("/dashboard/workflow")}
                className="ml-2 text-primary underline underline-offset-2 hover:text-primary/80"
              >
                在工作流系统视图中管理 →
              </button>
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              onClick={() => navigate("/lifecycle-editor/new")}
              className="agentdash-button-secondary text-sm"
            >
              + 新建 Lifecycle
            </button>
            <button
              type="button"
              onClick={() => navigate("/workflow-editor/new")}
              className="agentdash-button-primary text-sm"
            >
              + 新建 Workflow 定义
            </button>
          </div>
        </div>

        <div className="mt-3 flex flex-wrap gap-2">
          {ROLE_ORDER.map((role) => {
            const lifecycle = defaultAssignmentsByRole.get(role) ?? null;
            return lifecycle ? (
              <span
                key={role}
                className="rounded-full border border-primary/30 bg-primary/10 px-2.5 py-1 text-xs text-primary"
              >
                {ROLE_LABEL[role]}: {lifecycle.name}
              </span>
            ) : (
              <span
                key={role}
                className="rounded-full border border-amber-300/40 bg-amber-500/10 px-2.5 py-1 text-xs text-amber-700"
              >
                {ROLE_LABEL[role]}: 未配置默认生命周期
              </span>
            );
          })}
        </div>
      </div>

      {message && <p className="text-xs text-emerald-600">{message}</p>}
      {error && <p className="text-xs text-destructive">{error}</p>}

      <div className="space-y-3">
        <div>
          <p className="text-sm font-medium text-foreground">可注册的内置 Lifecycle Bundle</p>
          <p className="mt-1 text-xs text-muted-foreground">从内置 bundle 一次性注册 workflows 与 lifecycle。</p>
        </div>
        <div className="grid gap-3">
          {templates.length === 0 ? (
            <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-4 py-8 text-center text-sm text-muted-foreground">
              当前还没有可用的 workflow template。
            </div>
          ) : (
            templates
              .slice()
              .sort((a, b) => a.name.localeCompare(b.name, "zh-CN"))
              .map((template) => {
                const lifecycle = lifecycleDefinitions.find((item) => item.key === template.lifecycle.key) ?? null;
                return (
                  <TemplateCard
                    key={template.key}
                    template={template}
                    isRegistered={Boolean(lifecycle)}
                    isBootstrapping={bootstrappingTemplateKey === template.key}
                    onBootstrap={() => void handleBootstrap(template)}
                  />
                );
              })
          )}
        </div>
      </div>

      <div className="space-y-4">
        <div>
          <p className="text-sm font-medium text-foreground">Lifecycle Definitions</p>
          <p className="mt-1 text-xs text-muted-foreground">
            为每个角色选择默认使用的生命周期编排。
          </p>
        </div>

        {ROLE_ORDER.map((role) => {
          const roleLifecycles = (lifecyclesByRole.get(role) ?? [])
            .slice()
            .sort((left, right) => left.name.localeCompare(right.name, "zh-CN"));
          return (
            <div key={role} className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <p className="text-sm font-medium text-foreground">{ROLE_LABEL[role]}</p>
                <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                  {roleLifecycles.length} 个 lifecycle
                </span>
              </div>
              <div className="grid gap-3">
                {roleLifecycles.length === 0 ? (
                  <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-4 py-6 text-center text-sm text-muted-foreground">
                    当前还没有适用于 {ROLE_LABEL[role]} 的 lifecycle definition。
                  </div>
                ) : (
                  roleLifecycles.map((lifecycle) => {
                    const relatedAssignment = assignments.find(
                      (assignment) => assignment.lifecycle_id === lifecycle.id && assignment.role === role,
                    );
                    return (
                      <LifecycleCard
                        key={`${role}:${lifecycle.id}`}
                        lifecycle={lifecycle}
                        role={role}
                        isAssigned={Boolean(relatedAssignment)}
                        isDefault={defaultAssignmentsByRole.get(role)?.id === lifecycle.id}
                        isAssigning={assigningKey === `${lifecycle.id}:${role}`}
                        onEdit={() => navigate(`/lifecycle-editor/${lifecycle.id}`)}
                        onEnable={() => void enableLifecycle(lifecycle.id)}
                        onDisable={() => void disableLifecycle(lifecycle.id)}
                        onAssign={() => void handleAssign(lifecycle, role)}
                      />
                    );
                  })
                )}
              </div>
            </div>
          );
        })}
      </div>

      <div className="space-y-4">
        <div>
          <p className="text-sm font-medium text-foreground">Workflow 定义</p>
          <p className="mt-1 text-xs text-muted-foreground">
            Workflow 是 lifecycle step 的原子行为单元，可单独编辑与启停。
          </p>
        </div>
        {ROLE_ORDER.map((role) => {
          const roleDefinitions = (contractsByRole.get(role) ?? [])
            .slice()
            .sort((left, right) => left.name.localeCompare(right.name, "zh-CN"));
          return (
            <div key={role} className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <p className="text-sm font-medium text-foreground">{ROLE_LABEL[role]}</p>
                <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                  {roleDefinitions.length} 个 workflow
                </span>
              </div>
              <div className="grid gap-3">
                {roleDefinitions.length === 0 ? (
                  <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-4 py-6 text-center text-sm text-muted-foreground">
                    当前还没有适用于 {ROLE_LABEL[role]} 的 workflow 定义。
                  </div>
                ) : (
                  roleDefinitions.map((definition) => (
                    <WorkflowContractCard
                      key={`${role}:${definition.id}`}
                      definition={definition}
                      role={role}
                      onEdit={() => navigate(`/workflow-editor/${definition.id}`)}
                      onEnable={() => void enableDefinition(definition.id)}
                      onDisable={() => void disableDefinition(definition.id)}
                    />
                  ))
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
