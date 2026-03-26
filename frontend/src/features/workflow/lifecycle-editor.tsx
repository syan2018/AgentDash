import { useCallback, useEffect, useMemo } from "react";

import type {
  LifecycleStepDefinition,
  LifecycleTransitionPolicyKind,
  WorkflowDefinition,
  WorkflowSessionTerminalState,
  WorkflowTargetKind,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import { DetailSection } from "../../components/ui/detail-panel";
import {
  DEFINITION_STATUS_LABEL,
  ROLE_LABEL,
  ROLE_ORDER,
  TARGET_KIND_LABEL,
  TRANSITION_POLICY_LABEL,
} from "./shared-labels";
import { ValidationPanel } from "./ui/validation-panel";

function LifecycleStepCard({
  step,
  index,
  availableWorkflows,
  availableStepKeys,
  onChange,
  onRemove,
}: {
  step: LifecycleStepDefinition;
  index: number;
  availableWorkflows: WorkflowDefinition[];
  availableStepKeys: string[];
  onChange: (patch: Partial<LifecycleStepDefinition>) => void;
  onRemove: () => void;
}) {
  const workflowDatalistId = `wf-opts-${index}`;
  const nextStepDatalistId = `ns-opts-${index}`;

  const updateTransitionPolicy = (patch: Partial<LifecycleStepDefinition["transition"]["policy"]>) => {
    onChange({
      transition: {
        ...step.transition,
        policy: { ...step.transition.policy, ...patch },
      },
    });
  };

  return (
    <div className="space-y-3 rounded-[12px] border border-border bg-secondary/35 p-4">
      <div className="flex items-center justify-between gap-3">
        <p className="text-sm font-medium text-foreground">
          Step {index + 1}: {step.title || "(未命名)"}
        </p>
        <button
          type="button"
          onClick={onRemove}
          className="rounded-[8px] px-2 py-1 text-xs text-destructive transition-colors hover:bg-destructive/10"
        >
          Remove
        </button>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <div>
          <label className="agentdash-form-label">Step Key</label>
          <input
            value={step.key}
            onChange={(e) => onChange({ key: e.target.value })}
            className="agentdash-form-input"
            placeholder="implement"
          />
        </div>
        <div>
          <label className="agentdash-form-label">Title</label>
          <input
            value={step.title}
            onChange={(e) => onChange({ title: e.target.value })}
            className="agentdash-form-input"
            placeholder="实现"
          />
        </div>
      </div>

      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={step.description}
          onChange={(e) => onChange({ description: e.target.value })}
          rows={2}
          className="agentdash-form-textarea"
          placeholder="当前 step 的职责与边界"
        />
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <div>
          <label className="agentdash-form-label">Primary Workflow Key</label>
          <input
            value={step.primary_workflow_key}
            onChange={(e) => onChange({ primary_workflow_key: e.target.value })}
            list={workflowDatalistId}
            className="agentdash-form-input"
            placeholder="task_implement"
          />
          <datalist id={workflowDatalistId}>
            {availableWorkflows.map((wf) => (
              <option key={wf.id} value={wf.key}>{wf.name}</option>
            ))}
          </datalist>
        </div>
        <div>
          <label className="agentdash-form-label">Session Binding</label>
          <select
            value={step.session_binding}
            onChange={(e) => onChange({ session_binding: e.target.value as LifecycleStepDefinition["session_binding"] })}
            className="agentdash-form-select"
          >
            <option value="not_required">Not required</option>
            <option value="optional">Optional</option>
            <option value="required">Required</option>
          </select>
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-3">
        <div>
          <label className="agentdash-form-label">Transition policy</label>
          <select
            value={step.transition.policy.kind}
            onChange={(e) => updateTransitionPolicy({ kind: e.target.value as LifecycleTransitionPolicyKind })}
            className="agentdash-form-select"
          >
            {Object.entries(TRANSITION_POLICY_LABEL).map(([k, v]) => (
              <option key={k} value={k}>{v}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="agentdash-form-label">Next Step Key</label>
          <input
            value={step.transition.policy.next_step_key ?? ""}
            onChange={(e) => updateTransitionPolicy({ next_step_key: e.target.value || null })}
            list={nextStepDatalistId}
            className="agentdash-form-input"
            placeholder="check"
          />
          <datalist id={nextStepDatalistId}>
            {availableStepKeys.filter((k) => k && k !== step.key).map((k) => (
              <option key={k} value={k} />
            ))}
          </datalist>
        </div>
        <div>
          <label className="agentdash-form-label">Action Key</label>
          <input
            value={step.transition.policy.action_key ?? ""}
            onChange={(e) => updateTransitionPolicy({ action_key: e.target.value || null })}
            className="agentdash-form-input"
            placeholder="record_complete"
          />
        </div>
      </div>

      {/* Terminal States */}
      <div>
        <label className="agentdash-form-label">Session Terminal States</label>
        <div className="mt-1 flex flex-wrap gap-3">
          {(["completed", "failed", "interrupted"] as WorkflowSessionTerminalState[]).map((opt) => (
            <label key={opt} className="flex items-center gap-1.5 text-xs text-foreground">
              <input
                type="checkbox"
                checked={step.transition.policy.session_terminal_states.includes(opt)}
                onChange={(e) => {
                  const next = e.target.checked
                    ? [...step.transition.policy.session_terminal_states, opt]
                    : step.transition.policy.session_terminal_states.filter((s) => s !== opt);
                  updateTransitionPolicy({ session_terminal_states: next });
                }}
              />
              {opt}
            </label>
          ))}
        </div>
      </div>
    </div>
  );
}

export function LifecycleEditor() {
  const draft = useWorkflowStore((s) => s.lifecycleEditorDraft);
  const originalId = useWorkflowStore((s) => s.lifecycleEditorOriginalId);
  const validation = useWorkflowStore((s) => s.lifecycleEditorValidation);
  const isSaving = useWorkflowStore((s) => s.lifecycleEditorIsSaving);
  const isValidating = useWorkflowStore((s) => s.lifecycleEditorIsValidating);
  const isDirty = useWorkflowStore((s) => s.lifecycleEditorDirty);
  const error = useWorkflowStore((s) => s.lifecycleEditorError);
  const lifecycleDefinitions = useWorkflowStore((s) => s.lifecycleDefinitions);
  const workflowDefinitions = useWorkflowStore((s) => s.definitions);

  const updateLifecycleDraft = useWorkflowStore((s) => s.updateLifecycleDraft);
  const updateLifecycleStep = useWorkflowStore((s) => s.updateLifecycleStep);
  const addLifecycleStep = useWorkflowStore((s) => s.addLifecycleStep);
  const removeLifecycleStep = useWorkflowStore((s) => s.removeLifecycleStep);
  const validateLifecycleDraft = useWorkflowStore((s) => s.validateLifecycleDraft);
  const saveLifecycleDraft = useWorkflowStore((s) => s.saveLifecycleDraft);
  const fetchDefinitions = useWorkflowStore((s) => s.fetchDefinitions);
  const targetKind = draft?.target_kind;

  const currentDefinition = useMemo(() => {
    if (!originalId) return null;
    return lifecycleDefinitions.find((d) => d.id === originalId) ?? null;
  }, [lifecycleDefinitions, originalId]);

  useEffect(() => {
    if (targetKind) void fetchDefinitions(targetKind);
  }, [fetchDefinitions, targetKind]);

  const handleSave = useCallback(async () => {
    const result = await validateLifecycleDraft();
    if (result && result.issues.some((i) => i.severity === "error")) return;
    await saveLifecycleDraft();
  }, [validateLifecycleDraft, saveLifecycleDraft]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        if (!isSaving) void handleSave();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [handleSave, isSaving]);

  useEffect(() => {
    if (!isDirty) return;
    const handler = (e: BeforeUnloadEvent) => { e.preventDefault(); };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [isDirty]);

  if (!draft) return null;

  const isNew = originalId === null;
  const hasErrors = validation?.issues.some((i) => i.severity === "error") ?? false;
  const availableWorkflows = workflowDefinitions
    .filter((d) => d.target_kind === draft.target_kind)
    .sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));
  const availableStepKeys = draft.steps.map((s) => s.key).filter(Boolean);

  return (
    <div className="space-y-4 p-5">
      {/* 操作栏 */}
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          {isDirty && <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-700">未保存</span>}
          {currentDefinition && (
            <>
              <StatusPill status={currentDefinition.status} label={DEFINITION_STATUS_LABEL[currentDefinition.status]} />
              <span className="text-[10px] text-muted-foreground">v{currentDefinition.version}</span>
            </>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button type="button" onClick={() => void validateLifecycleDraft()} disabled={isValidating} className="agentdash-button-secondary text-sm">
            {isValidating ? "校验中…" : "校验"}
          </button>
          <button type="button" onClick={() => void handleSave()} disabled={isSaving || hasErrors} className="agentdash-button-primary text-sm">
            {isSaving ? "保存中…" : "保存"}
          </button>
        </div>
      </div>

      {validation && <ValidationPanel issues={validation.issues} />}
      {error && <div className="rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2"><p className="text-xs text-destructive">{error}</p></div>}

      {/* 基本信息 */}
      <DetailSection title="基本信息">
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="agentdash-form-label">Key</label>
            <input value={draft.key} onChange={(e) => updateLifecycleDraft({ key: e.target.value })} disabled={!isNew} className="agentdash-form-input disabled:opacity-60" placeholder="task_lifecycle_v2" />
          </div>
          <div>
            <label className="agentdash-form-label">名称</label>
            <input value={draft.name} onChange={(e) => updateLifecycleDraft({ name: e.target.value })} className="agentdash-form-input" placeholder="Task Lifecycle V2" />
          </div>
        </div>
        <div>
          <label className="agentdash-form-label">描述</label>
          <textarea value={draft.description} onChange={(e) => updateLifecycleDraft({ description: e.target.value })} rows={2} className="agentdash-form-textarea" placeholder="描述该 lifecycle 如何为 agent 分配阶段性 workflow" />
        </div>
        <div className="grid gap-3 sm:grid-cols-3">
          <div>
            <label className="agentdash-form-label">目标类型</label>
            <select value={draft.target_kind} onChange={(e) => updateLifecycleDraft({ target_kind: e.target.value as WorkflowTargetKind })} disabled={!isNew} className="agentdash-form-select disabled:opacity-60">
              {Object.entries(TARGET_KIND_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
            </select>
          </div>
          <div>
            <label className="agentdash-form-label">Recommended Roles</label>
            <div className="mt-1 flex flex-wrap gap-3">
              {ROLE_ORDER.map((r) => (
                <label key={r} className="flex items-center gap-1.5 text-xs text-foreground">
                  <input
                    type="checkbox"
                    checked={draft.recommended_roles.includes(r)}
                    onChange={(e) => {
                      const next = e.target.checked
                        ? [...draft.recommended_roles, r]
                        : draft.recommended_roles.filter((v) => v !== r);
                      updateLifecycleDraft({ recommended_roles: next });
                    }}
                  />
                  {ROLE_LABEL[r]}
                </label>
              ))}
            </div>
          </div>
          <div>
            <label className="agentdash-form-label">Entry Step Key</label>
            <input value={draft.entry_step_key} onChange={(e) => updateLifecycleDraft({ entry_step_key: e.target.value })} list="entry-step-opts" className="agentdash-form-input" placeholder="start" />
            <datalist id="entry-step-opts">
              {availableStepKeys.map((k) => <option key={k} value={k} />)}
            </datalist>
          </div>
        </div>
      </DetailSection>

      {/* Steps */}
      <DetailSection
        title={`Lifecycle Steps (${draft.steps.length})`}
        description="每个 step 把一个 primary workflow 挂载为当前 agent 行为定义，并决定何时进入下一步。"
        extra={
          <button type="button" onClick={addLifecycleStep} className="agentdash-button-secondary text-sm">
            + 添加
          </button>
        }
      >
        <div className="space-y-3">
          {draft.steps.map((step, idx) => (
            <LifecycleStepCard
              key={`${step.key || "step"}-${idx}`}
              step={step}
              index={idx}
              availableWorkflows={availableWorkflows}
              availableStepKeys={availableStepKeys}
              onChange={(patch) => updateLifecycleStep(idx, patch)}
              onRemove={() => removeLifecycleStep(idx)}
            />
          ))}
          {draft.steps.length === 0 && (
            <p className="text-center text-sm text-muted-foreground py-4">至少需要一个 entry step 才能生效。</p>
          )}
        </div>
      </DetailSection>
    </div>
  );
}

function StatusPill({ status, label }: { status: string; label: string }) {
  const colors: Record<string, string> = {
    active: "border-emerald-300/40 bg-emerald-500/10 text-emerald-700",
    disabled: "border-amber-300/40 bg-amber-500/10 text-amber-700",
  };
  return (
    <span className={`rounded-full border px-2 py-0.5 text-[10px] ${colors[status] ?? "border-border bg-secondary/40 text-muted-foreground"}`}>
      {label}
    </span>
  );
}
