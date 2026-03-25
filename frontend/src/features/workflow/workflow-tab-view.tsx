import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";

import type {
  WorkflowAgentRole,
  WorkflowAssignment,
  WorkflowDefinition,
  WorkflowTemplate,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import { useProjectStore } from "../../stores/projectStore";
import {
  DEFAULT_ROLE_BY_TARGET,
  ROLE_LABEL,
  ROLE_ORDER,
  TARGET_KIND_LABEL,
} from "./shared-labels";

const EMPTY_ASSIGNMENTS: WorkflowAssignment[] = [];

function resolveDefinitionRole(
  definition: WorkflowDefinition,
  _templateMap: Map<string, WorkflowTemplate>,
): WorkflowAgentRole {
  return definition.recommended_role
    ?? DEFAULT_ROLE_BY_TARGET[definition.target_kind];
}

function DefinitionCard({
  definition,
  role,
  isAssigned,
  isDefault,
  isAssigning,
  onAssign,
  onEdit,
  onEnable,
  onDisable,
}: {
  definition: WorkflowDefinition;
  role: WorkflowAgentRole;
  isAssigned: boolean;
  isDefault: boolean;
  isAssigning: boolean;
  onAssign: () => void;
  onEdit: () => void;
  onEnable: () => void;
  onDisable: () => void;
}) {
  return (
    <div className="rounded-[12px] border border-border bg-background p-4 space-y-2">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-sm font-medium text-foreground">{definition.name}</p>
            {definition.status === "active" && (
              <span className="rounded-full border border-emerald-300/40 bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-600">
                已激活
              </span>
            )}
            {definition.status === "draft" && (
              <span className="rounded-full border border-amber-300/40 bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-600">
                草稿
              </span>
            )}
            {definition.status === "disabled" && (
              <span className="rounded-full border border-red-300/40 bg-red-500/10 px-2 py-0.5 text-[11px] text-red-600">
                已停用
              </span>
            )}
            {isDefault && (
              <span className="rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
                默认
              </span>
            )}
          </div>
          <p className="mt-0.5 text-xs text-muted-foreground font-mono">{definition.key}</p>
        </div>
        <span className="shrink-0 rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
          v{definition.version} · {TARGET_KIND_LABEL[definition.target_kind]}
        </span>
      </div>

      {definition.description && (
        <p className="text-xs leading-5 text-muted-foreground">{definition.description}</p>
      )}

      {definition.phases.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {definition.phases.map((phase) => (
            <span key={phase.key} className="rounded-[6px] border border-border bg-secondary/30 px-2 py-0.5 text-[10px] text-muted-foreground">
              {phase.title}
            </span>
          ))}
        </div>
      )}

      <div className="flex flex-wrap justify-end gap-2 pt-1">
        <button type="button" onClick={onEdit} className="agentdash-button-secondary text-sm">
          编辑
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

export function WorkflowTabView() {
  const navigate = useNavigate();
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const templates = useWorkflowStore((state) => state.templates);
  const definitions = useWorkflowStore((state) => state.definitions);
  const assignments = useWorkflowStore(
    (state) => currentProjectId ? (state.assignmentsByProjectId[currentProjectId] ?? EMPTY_ASSIGNMENTS) : EMPTY_ASSIGNMENTS,
  );
  const fetchTemplates = useWorkflowStore((state) => state.fetchTemplates);
  const fetchDefinitions = useWorkflowStore((state) => state.fetchDefinitions);
  const fetchProjectAssignments = useWorkflowStore((state) => state.fetchProjectAssignments);
  const bootstrapTemplate = useWorkflowStore((state) => state.bootstrapTemplate);
  const assignWorkflowToProject = useWorkflowStore((state) => state.assignWorkflowToProject);
  const enableDefinition = useWorkflowStore((state) => state.enableDefinition);
  const disableDefinition = useWorkflowStore((state) => state.disableDefinition);

  const [message, setMessage] = useState<string | null>(null);
  const [bootstrappingTemplateKey, setBootstrappingTemplateKey] = useState<string | null>(null);
  const [assigningKey, setAssigningKey] = useState<string | null>(null);

  useEffect(() => {
    void fetchTemplates();
    void fetchDefinitions();
    if (currentProjectId) void fetchProjectAssignments(currentProjectId);
  }, [fetchDefinitions, fetchProjectAssignments, fetchTemplates, currentProjectId]);

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
    if (!currentProjectId) return;
    setMessage(null);
    setAssigningKey(`${definition.id}:${role}`);
    try {
      const assignment = await assignWorkflowToProject({
        project_id: currentProjectId,
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

  const unregisteredTemplates = templates.filter(
    (template) => !definitions.some((def) => def.key === template.key),
  );

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Page header */}
      <div className="shrink-0 border-b border-border px-6 py-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-lg font-semibold text-foreground">Workflow 管理</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              管理 Workflow 定义、注册内置模板，并为项目角色分配默认流程。
            </p>
          </div>
          <button
            type="button"
            onClick={() => navigate("/workflow-editor/new")}
            className="agentdash-button-primary"
          >
            + 新建 Workflow
          </button>
        </div>
      </div>

      {/* Scrollable content */}
      <div className="flex-1 overflow-y-auto px-6 py-5 space-y-6">
        {message && (
          <div className="rounded-[10px] border border-emerald-300/30 bg-emerald-500/5 px-4 py-2.5 text-sm text-emerald-700">
            {message}
          </div>
        )}

        {!currentProjectId && (
          <div className="rounded-[12px] border border-dashed border-amber-300/30 bg-amber-500/5 px-4 py-6 text-center text-sm text-amber-700">
            请先在左侧选择一个项目，以便管理 Workflow Assignment。
          </div>
        )}

        {/* Unregistered templates */}
        {unregisteredTemplates.length > 0 && (
          <section className="space-y-3">
            <div>
              <h3 className="text-sm font-medium text-foreground">可注册的内置模板</h3>
              <p className="mt-0.5 text-xs text-muted-foreground">从内置模板注册为可使用的工作流。</p>
            </div>
            <div className="grid gap-3 lg:grid-cols-2 xl:grid-cols-3">
              {unregisteredTemplates.map((template) => {
                const isBootstrapping = bootstrappingTemplateKey === template.key;
                return (
                  <div key={template.key} className="rounded-[12px] border border-border bg-secondary/20 p-4 space-y-2">
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <p className="text-sm font-medium text-foreground">{template.name}</p>
                        <p className="mt-0.5 text-xs text-muted-foreground font-mono">{template.key}</p>
                      </div>
                      <span className="shrink-0 rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                        {TARGET_KIND_LABEL[template.target_kind]}
                      </span>
                    </div>
                    <p className="text-xs leading-5 text-muted-foreground">{template.description}</p>
                    {template.phases.length > 0 && (
                      <div className="flex flex-wrap gap-1.5">
                        {template.phases.map((phase) => (
                          <span key={phase.key} className="rounded-[6px] border border-border bg-secondary/30 px-2 py-0.5 text-[10px] text-muted-foreground">
                            {phase.title}
                          </span>
                        ))}
                      </div>
                    )}
                    <div className="flex justify-end">
                      <button
                        type="button"
                        onClick={() => void handleBootstrap(template)}
                        disabled={isBootstrapping}
                        className="agentdash-button-primary text-sm"
                      >
                        {isBootstrapping ? "注册中..." : "注册模板"}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </section>
        )}

        {/* Registered definitions by role */}
        <section className="space-y-4">
          <div>
            <h3 className="text-sm font-medium text-foreground">已注册的 Workflow Definition</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">为每个角色选择默认使用的工作流。</p>
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
                          onEdit={() => navigate(`/workflow-editor/${definition.id}`)}
                          onEnable={() => void enableDefinition(definition.id)}
                          onDisable={() => void disableDefinition(definition.id)}
                        />
                      );
                    })
                  )}
                </div>
              </div>
            );
          })}
        </section>
      </div>
    </div>
  );
}
