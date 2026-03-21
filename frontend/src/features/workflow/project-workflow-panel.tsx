import { useEffect, useMemo, useState } from "react";

import type {
  WorkflowAgentRole,
  WorkflowAssignment,
  WorkflowContextBinding,
  WorkflowDefinition,
  WorkflowPhaseCompletionMode,
  WorkflowPhaseDefinition,
  WorkflowTargetKind,
  WorkflowTemplate,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";

const EMPTY_ASSIGNMENTS: WorkflowAssignment[] = [];

const TARGET_KIND_LABEL: Record<WorkflowTargetKind, string> = {
  project: "Project",
  story: "Story",
  task: "Task",
};

const ROLE_LABEL: Record<WorkflowAgentRole, string> = {
  project_context_maintainer: "Project 上下文维护",
  story_lifecycle_companion: "Story 生命周期协作",
  task_execution_worker: "Task 执行",
  review_agent: "Review",
  record_agent: "Record",
};

const ROLE_ORDER: WorkflowAgentRole[] = [
  "project_context_maintainer",
  "story_lifecycle_companion",
  "task_execution_worker",
  "review_agent",
  "record_agent",
];

const DEFAULT_ROLE_BY_TARGET: Record<WorkflowTargetKind, WorkflowAgentRole> = {
  project: "project_context_maintainer",
  story: "story_lifecycle_companion",
  task: "task_execution_worker",
};

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

function findDefinitionByTemplate(
  definitions: WorkflowDefinition[],
  template: WorkflowTemplate,
): WorkflowDefinition | null {
  return definitions.find((item) => item.key === template.key) ?? null;
}

function resolveDefinitionRole(
  definition: WorkflowDefinition,
  templateMap: Map<string, WorkflowTemplate>,
): WorkflowAgentRole {
  return templateMap.get(definition.key)?.recommended_role
    ?? DEFAULT_ROLE_BY_TARGET[definition.target_kind];
}

function PhaseSummary({ phase }: { phase: WorkflowPhaseDefinition }) {
  return (
    <div className="rounded-[10px] border border-border bg-background px-3 py-2 text-[11px]">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium text-foreground">{phase.title}</span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
          {COMPLETION_MODE_LABEL[phase.completion_mode]}
        </span>
      </div>
      {phase.agent_instructions.length > 0 && (
        <div className="mt-2 space-y-1 text-[10px] leading-5 text-foreground/75">
          {phase.agent_instructions.map((instruction, index) => (
            <p key={`${phase.key}-instruction-${index}`}>- {instruction}</p>
          ))}
        </div>
      )}
      {phase.context_bindings.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {phase.context_bindings.map((binding, index) => (
            <span
              key={`${phase.key}-${binding.locator}-${index}`}
              className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground"
              title={`${binding.reason} · ${binding.locator}`}
            >
              {BINDING_KIND_LABEL[binding.kind]}: {binding.title?.trim() || binding.locator}
            </span>
          ))}
        </div>
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
            已注册为 Definition
          </span>
        )}
      </div>

      <div className="mt-3">
        <p className="text-sm font-medium text-foreground">{template.name}</p>
        <p className="mt-1 text-xs text-muted-foreground">{template.key}</p>
        <p className="mt-2 text-sm leading-6 text-foreground/80">{template.description}</p>
      </div>

      <div className="mt-3 rounded-[10px] border border-border bg-secondary/20 p-3">
        <p className="text-xs font-medium text-muted-foreground">Phase</p>
        <div className="mt-2 grid gap-2">
          {template.phases.map((phase) => (
            <PhaseSummary key={phase.key} phase={phase} />
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
          {isBootstrapping ? "注册中..." : isRegistered ? "已注册" : "注册模板"}
        </button>
      </div>
    </div>
  );
}

function DefinitionCard({
  definition,
  role,
  isAssigned,
  isDefault,
  isAssigning,
  onAssign,
}: {
  definition: WorkflowDefinition;
  role: WorkflowAgentRole;
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
          {ROLE_LABEL[role]}
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
            当前角色默认流程
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
        <div className="mt-2 grid gap-2">
          {definition.phases.map((phase) => (
            <PhaseSummary key={phase.key} phase={phase} />
          ))}
        </div>
      </div>

      <div className="mt-4 flex justify-end">
        <button
          type="button"
          onClick={onAssign}
          disabled={isAssigning}
          className={isDefault ? "agentdash-button-secondary" : "agentdash-button-primary"}
        >
          {isAssigning ? "保存中..." : isDefault ? "重新设为默认" : "设为当前角色默认流程"}
        </button>
      </div>
    </div>
  );
}

export function ProjectWorkflowPanel({ projectId }: { projectId: string }) {
  const templates = useWorkflowStore((state) => state.templates);
  const definitions = useWorkflowStore((state) => state.definitions);
  const assignments = useWorkflowStore(
    (state) => state.assignmentsByProjectId[projectId] ?? EMPTY_ASSIGNMENTS,
  );
  const error = useWorkflowStore((state) => state.error);
  const fetchTemplates = useWorkflowStore((state) => state.fetchTemplates);
  const fetchDefinitions = useWorkflowStore((state) => state.fetchDefinitions);
  const fetchProjectAssignments = useWorkflowStore((state) => state.fetchProjectAssignments);
  const bootstrapTemplate = useWorkflowStore((state) => state.bootstrapTemplate);
  const assignWorkflowToProject = useWorkflowStore((state) => state.assignWorkflowToProject);

  const [message, setMessage] = useState<string | null>(null);
  const [bootstrappingTemplateKey, setBootstrappingTemplateKey] = useState<string | null>(null);
  const [assigningKey, setAssigningKey] = useState<string | null>(null);

  useEffect(() => {
    void fetchTemplates();
    void fetchDefinitions();
    void fetchProjectAssignments(projectId);
  }, [fetchDefinitions, fetchProjectAssignments, fetchTemplates, projectId]);

  const templateMap = useMemo(
    () => new Map(templates.map((template) => [template.key, template] as const)),
    [templates],
  );

  const defaultAssignmentsByRole = useMemo(() => {
    const entries = ROLE_ORDER.map((role) => {
      const roleAssignments = assignments.filter((item) => item.role === role && item.enabled);
      const activeAssignment = roleAssignments.find((item) => item.is_default) ?? roleAssignments[0] ?? null;
      const definition = activeAssignment
        ? definitions.find((item) => item.id === activeAssignment.workflow_id) ?? null
        : null;
      return [role, definition] as const;
    });
    return new Map(entries);
  }, [assignments, definitions]);

  const definitionsByRole = useMemo(() => {
    const grouped = new Map<WorkflowAgentRole, WorkflowDefinition[]>();
    for (const role of ROLE_ORDER) {
      grouped.set(role, []);
    }
    for (const definition of definitions) {
      const role = resolveDefinitionRole(definition, templateMap);
      grouped.set(role, [...(grouped.get(role) ?? []), definition]);
    }
    return grouped;
  }, [definitions, templateMap]);

  const handleBootstrap = async (template: WorkflowTemplate) => {
    setMessage(null);
    setBootstrappingTemplateKey(template.key);
    try {
      const definition = await bootstrapTemplate(template.key);
      if (definition) {
        setMessage(`已注册模板：${template.name}`);
      }
    } finally {
      setBootstrappingTemplateKey(null);
    }
  };

  const handleAssign = async (definition: WorkflowDefinition, role: WorkflowAgentRole) => {
    setMessage(null);
    setAssigningKey(`${definition.id}:${role}`);
    try {
      const assignment = await assignWorkflowToProject({
        project_id: projectId,
        workflow_id: definition.id,
        role,
        enabled: true,
        is_default: true,
      });
      if (assignment) {
        setMessage(`已将 ${definition.name} 设为 ${ROLE_LABEL[role]} 的默认流程`);
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
            <p className="text-sm font-medium text-foreground">Workflow 模板与定义</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              builtin workflow 只是全局数据模板；注册后成为 definition，再按不同 Agent role 绑定到当前 Project。
            </p>
          </div>
        </div>

        <div className="mt-3 flex flex-wrap gap-2">
          {ROLE_ORDER.map((role) => {
            const definition = defaultAssignmentsByRole.get(role) ?? null;
            return definition ? (
              <span
                key={role}
                className="rounded-full border border-primary/30 bg-primary/10 px-2.5 py-1 text-xs text-primary"
              >
                {ROLE_LABEL[role]}: {definition.name}
              </span>
            ) : (
              <span
                key={role}
                className="rounded-full border border-amber-300/40 bg-amber-500/10 px-2.5 py-1 text-xs text-amber-700"
              >
                {ROLE_LABEL[role]}: 未配置默认流程
              </span>
            );
          })}
        </div>
      </div>

      {message && <p className="text-xs text-emerald-600">{message}</p>}
      {error && <p className="text-xs text-destructive">{error}</p>}

      <div className="space-y-3">
        <div>
          <p className="text-sm font-medium text-foreground">可注册的内置模板</p>
          <p className="mt-1 text-xs text-muted-foreground">
            template 只描述 phase、binding 与自动注入约束；真正生效的是注册后的 workflow definition。
          </p>
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
                const definition = findDefinitionByTemplate(definitions, template);
                return (
                  <TemplateCard
                    key={template.key}
                    template={template}
                    isRegistered={Boolean(definition)}
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
          <p className="text-sm font-medium text-foreground">已注册的 Workflow Definition</p>
          <p className="mt-1 text-xs text-muted-foreground">
            每个 role 可以选择自己的默认流程，不再只局限于 Task 执行流程。
          </p>
        </div>

        {ROLE_ORDER.map((role) => {
          const roleDefinitions = (definitionsByRole.get(role) ?? [])
            .slice()
            .sort((left, right) => left.name.localeCompare(right.name, "zh-CN"));
          return (
            <div key={role} className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <p className="text-sm font-medium text-foreground">{ROLE_LABEL[role]}</p>
                <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[11px] text-muted-foreground">
                  {roleDefinitions.length} 个 definition
                </span>
              </div>
              <div className="grid gap-3">
                {roleDefinitions.length === 0 ? (
                  <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-4 py-6 text-center text-sm text-muted-foreground">
                    当前还没有适用于 {ROLE_LABEL[role]} 的 workflow definition。
                  </div>
                ) : (
                  roleDefinitions.map((definition) => {
                    const relatedAssignment = assignments.find(
                      (assignment) => assignment.workflow_id === definition.id && assignment.role === role,
                    );
                    return (
                      <DefinitionCard
                        key={`${role}:${definition.id}`}
                        definition={definition}
                        role={role}
                        isAssigned={Boolean(relatedAssignment)}
                        isDefault={defaultAssignmentsByRole.get(role)?.id === definition.id}
                        isAssigning={assigningKey === `${definition.id}:${role}`}
                        onAssign={() => void handleAssign(definition, role)}
                      />
                    );
                  })
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
