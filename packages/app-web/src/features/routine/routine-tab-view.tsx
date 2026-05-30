import { useCallback, useEffect, useState } from "react";
import type { Routine, ProjectAgent } from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import {
  useCreateRoutineMutation,
  useDeleteRoutineMutation,
  useProjectRoutinesQuery,
  useSetRoutineEnabledMutation,
  useUpdateRoutineMutation,
} from "./model/routineQueries";
import { CreateButton, DetailPanel, DangerConfirmDialog } from "@agentdash/ui";
import { INITIAL_FORM, routineToForm, formToPayload } from "./form-state";
import { RoutineCard } from "./routine-card";
import { RoutineDialog } from "./routine-dialog";
import { WebhookTokenAlert } from "./webhook-token-alert";
import { ExecutionHistoryContent } from "./execution-history-panel";
import { ROUTINE_TEMPLATES, templateToFormPatch } from "./routine-templates";

// ─── Empty State with Templates ───

function EmptyState({
  projectAgents,
  onCreateBlank,
  onCreateFromTemplate,
}: {
  projectAgents: ProjectAgent[];
  onCreateBlank: () => void;
  onCreateFromTemplate: (patch: ReturnType<typeof templateToFormPatch>) => void;
}) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-5 px-6 py-12 text-center">
      <div>
        <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-[12px] bg-secondary/50">
          <svg className="h-6 w-6 text-muted-foreground/50" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z" />
          </svg>
        </div>
        <p className="text-sm font-medium text-foreground">还没有 Routine</p>
        <p className="mt-1 text-xs text-muted-foreground">
          创建自动化任务，让 Agent 按计划或事件自动执行
        </p>
      </div>

      {/* Template cards */}
      <div className="grid w-full max-w-2xl grid-cols-1 gap-3 sm:grid-cols-2">
        {ROUTINE_TEMPLATES.map((tpl) => (
          <button
            key={tpl.name}
            type="button"
            onClick={() => onCreateFromTemplate(templateToFormPatch(tpl))}
            disabled={projectAgents.length === 0}
            className="rounded-[8px] border border-border bg-background p-4 text-left transition-colors hover:border-primary/30 hover:bg-primary/5 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <div className="flex items-center gap-2">
              <span className={`inline-block rounded-[6px] border px-2 py-0.5 text-[10px] ${
                tpl.trigger_type === "scheduled"
                  ? "border-info/30 bg-info/10 text-info"
                  : "border-success/30 bg-success/10 text-success"
              }`}>
                {tpl.trigger_type === "scheduled" ? "定时" : "Webhook"}
              </span>
              <span className="text-xs font-medium text-foreground">{tpl.name}</span>
            </div>
            <p className="mt-1.5 text-[11px] leading-relaxed text-muted-foreground line-clamp-2">
              {tpl.description}
            </p>
          </button>
        ))}
      </div>

      <button
        type="button"
        onClick={onCreateBlank}
        className="agentdash-button-secondary text-xs"
      >
        从空白创建
      </button>
    </div>
  );
}

// ─── RoutineTabView ───

export function RoutineTabView() {
  const { currentProjectId, projectAgentConfigsByProjectId, fetchProjectAgentConfigs } = useProjectStore();
  const routinesQuery = useProjectRoutinesQuery(currentProjectId);
  const createRoutine = useCreateRoutineMutation(currentProjectId);
  const updateRoutine = useUpdateRoutineMutation(currentProjectId);
  const deleteRoutine = useDeleteRoutineMutation(currentProjectId);
  const setRoutineEnabled = useSetRoutineEnabledMutation(currentProjectId);

  const [showCreate, setShowCreate] = useState(false);
  const [createInitial, setCreateInitial] = useState(INITIAL_FORM);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [historyId, setHistoryId] = useState<string | null>(null);
  const [tokenAlert, setTokenAlert] = useState<{ token: string; endpointId: string; name: string } | null>(null);

  const routines = routinesQuery.data ?? [];
  const projectAgents: ProjectAgent[] = currentProjectId
    ? projectAgentConfigsByProjectId[currentProjectId] ?? []
    : [];
  const editingRoutine = editingId ? routines.find((r) => r.id === editingId) : undefined;
  const deletingRoutine = deletingId ? routines.find((r) => r.id === deletingId) : undefined;
  const historyRoutine = historyId ? routines.find((r) => r.id === historyId) : undefined;

  useEffect(() => {
    if (currentProjectId) {
      void fetchProjectAgentConfigs(currentProjectId);
    }
  }, [currentProjectId, fetchProjectAgentConfigs]);

  const openCreateBlank = () => {
    setCreateInitial({ ...INITIAL_FORM, project_agent_id: projectAgents[0]?.id ?? "" });
    setShowCreate(true);
  };

  const openCreateFromTemplate = (patch: Partial<typeof INITIAL_FORM>) => {
    setCreateInitial({ ...INITIAL_FORM, ...patch, project_agent_id: projectAgents[0]?.id ?? "" });
    setShowCreate(true);
  };

  const handleCreate = useCallback(
    async (payload: ReturnType<typeof formToPayload>) => {
      if (!currentProjectId) return;
      const result = await createRoutine.mutateAsync(payload);
      if (result) {
        setShowCreate(false);
        if (result.webhook_token && result.trigger_config?.endpoint_id) {
          setTokenAlert({
            token: result.webhook_token,
            endpointId: result.trigger_config.endpoint_id,
            name: result.name,
          });
        }
      }
    },
    [currentProjectId, createRoutine],
  );

  const handleUpdate = useCallback(
    async (payload: ReturnType<typeof formToPayload>) => {
      if (!editingId) return;
      const result = await updateRoutine.mutateAsync({ id: editingId, payload });
      if (result) setEditingId(null);
    },
    [editingId, updateRoutine],
  );

  const handleDelete = useCallback(async () => {
    if (!deletingId || !currentProjectId) return;
    await deleteRoutine.mutateAsync(deletingId);
    setDeletingId(null);
  }, [deletingId, currentProjectId, deleteRoutine]);

  const handleToggleEnable = useCallback(
    async (routine: Routine) => {
      if (!currentProjectId) return;
      await setRoutineEnabled.mutateAsync({ id: routine.id, enabled: !routine.enabled });
    },
    [currentProjectId, setRoutineEnabled],
  );

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-sm text-muted-foreground">请先选择项目</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <header className="flex items-center justify-between border-b border-border px-6 py-4">
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-semibold text-foreground">Routine</h2>
          {routines.length > 0 && (
            <span className="rounded-[6px] border border-border bg-secondary px-2 py-0.5 text-[10px] text-muted-foreground">
              {routines.length}
            </span>
          )}
        </div>
        <CreateButton entity="Routine" onClick={openCreateBlank} />
      </header>

      {/* Content */}
      <main className="flex-1 overflow-y-auto">
        {routines.length === 0 ? (
          <EmptyState
            projectAgents={projectAgents}
            onCreateBlank={openCreateBlank}
            onCreateFromTemplate={openCreateFromTemplate}
          />
        ) : (
          <div className="space-y-3 p-4">
            {routines.map((routine) => (
              <RoutineCard
                key={routine.id}
                routine={routine}
                projectAgents={projectAgents}
                onEdit={() => setEditingId(routine.id)}
                onToggleEnable={() => void handleToggleEnable(routine)}
                onViewHistory={() => setHistoryId(routine.id)}
                onDelete={() => setDeletingId(routine.id)}
              />
            ))}
          </div>
        )}
      </main>

      {/* Create Dialog */}
      {showCreate && (
        <RoutineDialog
          mode="create"
          initial={createInitial}
          projectAgents={projectAgents}
          onSave={handleCreate}
          onClose={() => setShowCreate(false)}
        />
      )}

      {/* Edit Dialog */}
      {editingRoutine && (
        <RoutineDialog
          mode="edit"
          initial={routineToForm(editingRoutine)}
          projectAgents={projectAgents}
          editingRoutine={editingRoutine}
          onSave={handleUpdate}
          onClose={() => setEditingId(null)}
        />
      )}

      {/* Delete Confirm */}
      <DangerConfirmDialog
        open={!!deletingRoutine}
        title={`删除 Routine「${deletingRoutine?.name ?? ""}」`}
        description="删除后不可恢复，关联的执行记录也将被清除。"
        confirmLabel="确认删除"
        onClose={() => setDeletingId(null)}
        onConfirm={() => void handleDelete()}
      />

      {/* Execution History Panel */}
      <DetailPanel
        open={!!historyRoutine}
        title={`执行历史 — ${historyRoutine?.name ?? ""}`}
        onClose={() => setHistoryId(null)}
      >
        {historyId && <ExecutionHistoryContent routineId={historyId} />}
      </DetailPanel>

      {/* Webhook Token Alert */}
      {tokenAlert && (
        <WebhookTokenAlert
          token={tokenAlert.token}
          endpointId={tokenAlert.endpointId}
          routineName={tokenAlert.name}
          onClose={() => setTokenAlert(null)}
        />
      )}
    </div>
  );
}
