import { useEffect, useMemo } from "react";
import { useNavigate } from "react-router-dom";

import type {
  ValidationIssue,
  WorkflowAgentRole,
  WorkflowTargetKind,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import {
  ROLE_LABEL,
  ROLE_ORDER,
  TARGET_KIND_LABEL,
  DEFINITION_STATUS_LABEL,
} from "./shared-labels";
import { PhaseEditor } from "./phase-editor";

function ValidationPanel({ issues }: { issues: ValidationIssue[] }) {
  const errors = issues.filter((i) => i.severity === "error");
  const warnings = issues.filter((i) => i.severity === "warning");

  return (
    <div className="space-y-2">
      {errors.length > 0 && (
        <div className="rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2.5 space-y-1.5">
          <p className="text-[11px] font-medium text-destructive">
            {errors.length} 个错误
          </p>
          {errors.map((issue, i) => (
            <div key={i} className="text-[11px] text-destructive/80 leading-5">
              <span className="font-mono text-[10px] text-destructive/60">{issue.field_path}</span>
              <span className="mx-1.5">·</span>
              {issue.message}
            </div>
          ))}
        </div>
      )}
      {warnings.length > 0 && (
        <div className="rounded-[10px] border border-amber-300/30 bg-amber-500/5 px-3 py-2.5 space-y-1.5">
          <p className="text-[11px] font-medium text-amber-700">
            {warnings.length} 个警告
          </p>
          {warnings.map((issue, i) => (
            <div key={i} className="text-[11px] text-amber-700/80 leading-5">
              <span className="font-mono text-[10px] text-amber-700/60">{issue.field_path}</span>
              <span className="mx-1.5">·</span>
              {issue.message}
            </div>
          ))}
        </div>
      )}
      {errors.length === 0 && warnings.length === 0 && (
        <div className="rounded-[10px] border border-emerald-300/30 bg-emerald-500/5 px-3 py-2.5">
          <p className="text-[11px] text-emerald-700">校验通过，无错误或警告。</p>
        </div>
      )}
    </div>
  );
}

export function WorkflowEditor({ embedded }: { embedded?: boolean } = {}) {
  const navigate = useNavigate();
  const draft = useWorkflowStore((s) => s.editorDraft);
  const originalId = useWorkflowStore((s) => s.editorOriginalId);
  const validation = useWorkflowStore((s) => s.editorValidation);
  const isSaving = useWorkflowStore((s) => s.editorIsSaving);
  const isValidating = useWorkflowStore((s) => s.editorIsValidating);
  const isDirty = useWorkflowStore((s) => s.editorDirty);
  const error = useWorkflowStore((s) => s.error);
  const bindingMetadata = useWorkflowStore((s) => s.bindingMetadata);

  const closeDraft = useWorkflowStore((s) => s.closeDraft);
  const updateDraft = useWorkflowStore((s) => s.updateDraft);
  const updateDraftPhase = useWorkflowStore((s) => s.updateDraftPhase);
  const addDraftPhase = useWorkflowStore((s) => s.addDraftPhase);
  const removeDraftPhase = useWorkflowStore((s) => s.removeDraftPhase);
  const moveDraftPhase = useWorkflowStore((s) => s.moveDraftPhase);
  const updateDraftPhaseBinding = useWorkflowStore((s) => s.updateDraftPhaseBinding);
  const addDraftPhaseBinding = useWorkflowStore((s) => s.addDraftPhaseBinding);
  const removeDraftPhaseBinding = useWorkflowStore((s) => s.removeDraftPhaseBinding);
  const validateDraft = useWorkflowStore((s) => s.validateDraft);
  const saveDraft = useWorkflowStore((s) => s.saveDraft);
  const loadBindingMetadata = useWorkflowStore((s) => s.loadBindingMetadata);

  const definitions = useWorkflowStore((s) => s.definitions);
  const currentDefinition = useMemo(() => {
    if (!originalId) return null;
    return definitions.find((d) => d.id === originalId) ?? null;
  }, [originalId, definitions]);

  useEffect(() => {
    void loadBindingMetadata();
  }, [loadBindingMetadata]);

  if (!draft) return null;

  const isNew = originalId === null;
  const hasValidationErrors = validation?.issues.some((i) => i.severity === "error") ?? false;

  const handleSave = async () => {
    const result = await validateDraft();
    if (result && result.issues.some((i) => i.severity === "error")) return;
    await saveDraft();
  };

  return (
    <div className="rounded-[16px] border border-primary/20 bg-background shadow-lg">
      {/* Header */}
      <div className="flex items-center justify-between gap-3 border-b border-border px-5 py-4">
        <div className="min-w-0">
          <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
            {isNew ? "新建 Workflow Definition" : "编辑 Workflow Definition"}
          </p>
          <p className="mt-1 text-sm font-medium text-foreground truncate">
            {draft.name || "(未命名)"}
          </p>
          {currentDefinition && (
            <div className="mt-1 flex items-center gap-2">
              <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                v{currentDefinition.version}
              </span>
              <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                {DEFINITION_STATUS_LABEL[currentDefinition.status]}
              </span>
            </div>
          )}
        </div>
        <div className="flex items-center gap-2 shrink-0">
          {isDirty && (
            <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-700">
              未保存修改
            </span>
          )}
          <button
            type="button"
            onClick={() => void validateDraft()}
            disabled={isValidating}
            className="agentdash-button-secondary text-sm"
          >
            {isValidating ? "校验中…" : "校验"}
          </button>
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={isSaving || hasValidationErrors}
            className="agentdash-button-primary text-sm"
          >
            {isSaving ? "保存中…" : "保存"}
          </button>
          <button
            type="button"
            onClick={() => {
              closeDraft();
              if (!embedded) navigate("/dashboard/workflow");
            }}
            className="rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:bg-secondary"
          >
            关闭
          </button>
        </div>
      </div>

      {/* Validation results */}
      {validation && (
        <div className="border-b border-border px-5 py-3">
          <ValidationPanel issues={validation.issues} />
        </div>
      )}

      {/* Error banner */}
      {error && (
        <div className="border-b border-destructive/30 bg-destructive/5 px-5 py-2.5">
          <p className="text-[11px] text-destructive">{error}</p>
        </div>
      )}

      {/* Body */}
      <div className="px-5 py-5 space-y-6">
        {/* Basic info */}
        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">基本信息</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="text-[11px] text-muted-foreground">Key</label>
              <input
                value={draft.key}
                onChange={(e) => updateDraft({ key: e.target.value })}
                disabled={!isNew}
                className="agentdash-form-input mt-1 text-sm disabled:opacity-60"
                placeholder="unique_workflow_key"
              />
              {!isNew && (
                <p className="mt-0.5 text-[10px] text-muted-foreground/60">Key 在创建后不可修改</p>
              )}
            </div>
            <div>
              <label className="text-[11px] text-muted-foreground">名称</label>
              <input
                value={draft.name}
                onChange={(e) => updateDraft({ name: e.target.value })}
                className="agentdash-form-input mt-1 text-sm"
                placeholder="Workflow 显示名"
              />
            </div>
          </div>

          <div>
            <label className="text-[11px] text-muted-foreground">描述</label>
            <textarea
              value={draft.description}
              onChange={(e) => updateDraft({ description: e.target.value })}
              className="agentdash-form-input mt-1 min-h-[60px] text-sm"
              placeholder="Workflow 描述"
              rows={2}
            />
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="text-[11px] text-muted-foreground">适用目标类型</label>
              <select
                value={draft.target_kind}
                onChange={(e) => updateDraft({ target_kind: e.target.value as WorkflowTargetKind })}
                disabled={!isNew}
                className="agentdash-form-select mt-1 text-sm disabled:opacity-60"
              >
                {Object.entries(TARGET_KIND_LABEL).map(([k, v]) => (
                  <option key={k} value={k}>{v}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-[11px] text-muted-foreground">推荐角色</label>
              <select
                value={draft.recommended_role ?? ""}
                onChange={(e) => updateDraft({
                  recommended_role: (e.target.value || null) as WorkflowAgentRole | null,
                })}
                className="agentdash-form-select mt-1 text-sm"
              >
                <option value="">(无推荐角色)</option>
                {ROLE_ORDER.map((role) => (
                  <option key={role} value={role}>{ROLE_LABEL[role]}</option>
                ))}
              </select>
            </div>
          </div>
        </section>

        {/* Record policy */}
        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">Record Policy</h3>
          <div className="flex flex-wrap gap-x-6 gap-y-2">
            <label className="flex items-center gap-2 text-[11px] text-foreground">
              <input
                type="checkbox"
                checked={draft.record_policy.emit_summary}
                onChange={(e) => updateDraft({
                  record_policy: { ...draft.record_policy, emit_summary: e.target.checked },
                })}
              />
              产出摘要（emit_summary）
            </label>
            <label className="flex items-center gap-2 text-[11px] text-foreground">
              <input
                type="checkbox"
                checked={draft.record_policy.emit_journal_update}
                onChange={(e) => updateDraft({
                  record_policy: { ...draft.record_policy, emit_journal_update: e.target.checked },
                })}
              />
              更新日志（emit_journal_update）
            </label>
            <label className="flex items-center gap-2 text-[11px] text-foreground">
              <input
                type="checkbox"
                checked={draft.record_policy.emit_archive_suggestion}
                onChange={(e) => updateDraft({
                  record_policy: { ...draft.record_policy, emit_archive_suggestion: e.target.checked },
                })}
              />
              归档建议（emit_archive_suggestion）
            </label>
          </div>
        </section>

        {/* Phases */}
        <section className="space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">
              Phases ({draft.phases.length})
            </h3>
            <button
              type="button"
              onClick={addDraftPhase}
              className="agentdash-button-secondary text-sm"
            >
              + 新增阶段
            </button>
          </div>

          {draft.phases.length === 0 ? (
            <div className="rounded-[12px] border border-dashed border-border bg-secondary/10 px-4 py-8 text-center text-sm text-muted-foreground">
              还没有定义任何阶段，点击"新增阶段"开始构建 workflow。
            </div>
          ) : (
            <div className="space-y-3">
              {draft.phases.map((phase, phaseIndex) => (
                <PhaseEditor
                  key={`${phaseIndex}-${phase.key}`}
                  phase={phase}
                  index={phaseIndex}
                  totalPhases={draft.phases.length}
                  targetKind={draft.target_kind}
                  bindingMetadata={bindingMetadata}
                  onUpdate={(patch) => updateDraftPhase(phaseIndex, patch)}
                  onUpdateBinding={(bi, patch) => updateDraftPhaseBinding(phaseIndex, bi, patch)}
                  onAddBinding={() => addDraftPhaseBinding(phaseIndex)}
                  onRemoveBinding={(bi) => removeDraftPhaseBinding(phaseIndex, bi)}
                  onRemove={() => removeDraftPhase(phaseIndex)}
                  onMoveUp={() => moveDraftPhase(phaseIndex, phaseIndex - 1)}
                  onMoveDown={() => moveDraftPhase(phaseIndex, phaseIndex + 1)}
                />
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
