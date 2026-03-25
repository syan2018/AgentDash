import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";

import type {
  LifecycleStepDefinition,
  ValidationIssue,
  WorkflowAgentRole,
  WorkflowAttachmentSpec,
  WorkflowDefinition,
  WorkflowSessionTerminalState,
  WorkflowTargetKind,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import {
  DEFINITION_STATUS_LABEL,
  ROLE_LABEL,
  ROLE_ORDER,
  TARGET_KIND_LABEL,
  TRANSITION_POLICY_LABEL,
} from "./shared-labels";

function ValidationPanel({ issues }: { issues: ValidationIssue[] }) {
  const errors = issues.filter((item) => item.severity === "error");
  const warnings = issues.filter((item) => item.severity === "warning");

  return (
    <div className="space-y-2">
      {errors.length > 0 && (
        <div className="space-y-1.5 rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2.5">
          <p className="text-[11px] font-medium text-destructive">{errors.length} 个错误</p>
          {errors.map((issue, index) => (
            <div key={index} className="text-[11px] leading-5 text-destructive/80">
              <span className="font-mono text-[10px] text-destructive/60">{issue.field_path}</span>
              <span className="mx-1.5">·</span>
              {issue.message}
            </div>
          ))}
        </div>
      )}
      {warnings.length > 0 && (
        <div className="space-y-1.5 rounded-[10px] border border-amber-300/30 bg-amber-500/5 px-3 py-2.5">
          <p className="text-[11px] font-medium text-amber-700">{warnings.length} 个警告</p>
          {warnings.map((issue, index) => (
            <div key={index} className="text-[11px] leading-5 text-amber-700/80">
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

function StepAttachmentEditor({
  value,
  onCommit,
}: {
  value: WorkflowAttachmentSpec[];
  onCommit: (next: WorkflowAttachmentSpec[]) => void;
}) {
  const [draft, setDraft] = useState(() => JSON.stringify(value, null, 2));
  const [error, setError] = useState<string | null>(null);

  const commit = () => {
    try {
      const parsed = JSON.parse(draft) as unknown;
      if (!Array.isArray(parsed)) {
        setError("attached_workflows 必须是 JSON 数组");
        return;
      }
      onCommit(parsed as WorkflowAttachmentSpec[]);
      setError(null);
    } catch (err) {
      setError((err as Error).message);
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between gap-2">
        <label className="text-[11px] text-muted-foreground">Attached Workflows (JSON)</label>
        <button
          type="button"
          onClick={commit}
          className="rounded px-2 py-0.5 text-[11px] text-primary hover:bg-primary/10"
        >
          应用 JSON
        </button>
      </div>
      <textarea
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        rows={5}
        className="agentdash-form-textarea mt-1 font-mono text-xs"
        placeholder='[{"workflow_key":"task_review_overlay","mode":"overlay","lifetime":"until_step_exit","priority":0}]'
      />
      {error && <p className="mt-1 text-[11px] text-destructive">{error}</p>}
    </div>
  );
}

function TerminalStatePicker({
  value,
  onChange,
}: {
  value: WorkflowSessionTerminalState[];
  onChange: (next: WorkflowSessionTerminalState[]) => void;
}) {
  const options: WorkflowSessionTerminalState[] = ["completed", "failed", "interrupted"];

  return (
    <div>
      <label className="text-[11px] text-muted-foreground">Session Terminal States</label>
      <div className="mt-2 flex flex-wrap gap-3">
        {options.map((option) => {
          const checked = value.includes(option);
          return (
            <label key={option} className="flex items-center gap-2 text-[11px] text-foreground">
              <input
                type="checkbox"
                checked={checked}
                onChange={(event) => {
                  if (event.target.checked) {
                    onChange([...value, option]);
                  } else {
                    onChange(value.filter((item) => item !== option));
                  }
                }}
              />
              {option}
            </label>
          );
        })}
      </div>
    </div>
  );
}

function LifecycleStepCard({
  step,
  index,
  availableWorkflows,
  availableStepKeys,
  onChange,
  onCommitAttachments,
  onChangeTerminalStates,
  onRemove,
}: {
  step: LifecycleStepDefinition;
  index: number;
  availableWorkflows: WorkflowDefinition[];
  availableStepKeys: string[];
  onChange: (patch: Partial<LifecycleStepDefinition>) => void;
  onCommitAttachments: (next: WorkflowAttachmentSpec[]) => void;
  onChangeTerminalStates: (next: WorkflowSessionTerminalState[]) => void;
  onRemove: () => void;
}) {
  const workflowDatalistId = `workflow-key-options-${index}`;
  const nextStepDatalistId = `next-step-options-${index}`;

  return (
    <div className="space-y-4 rounded-[14px] border border-border bg-secondary/10 p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-sm font-medium text-foreground">
            Step {index + 1}: {step.title || "(未命名)"}
          </p>
          <p className="mt-1 text-[11px] text-muted-foreground">
            当前 step 负责把一个 Workflow 定义作为主行为单元挂到 agent 上，并定义何时推进到下一个 step。
          </p>
        </div>
        <button
          type="button"
          onClick={onRemove}
          className="rounded-[8px] border border-destructive/20 bg-destructive/5 px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-destructive/10"
        >
          删除 Step
        </button>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <div>
          <label className="text-[11px] text-muted-foreground">Step Key</label>
          <input
            value={step.key}
            onChange={(event) => onChange({ key: event.target.value })}
            className="agentdash-form-input mt-1 text-sm"
            placeholder="implement"
          />
        </div>
        <div>
          <label className="text-[11px] text-muted-foreground">Step 标题</label>
          <input
            value={step.title}
            onChange={(event) => onChange({ title: event.target.value })}
            className="agentdash-form-input mt-1 text-sm"
            placeholder="实现"
          />
        </div>
      </div>

      <div>
        <label className="text-[11px] text-muted-foreground">Step 描述</label>
        <textarea
          value={step.description}
          onChange={(event) => onChange({ description: event.target.value })}
          rows={2}
          className="agentdash-form-textarea mt-1"
          placeholder="说明当前 step 的职责与边界"
        />
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <div>
          <label className="text-[11px] text-muted-foreground">Primary Workflow Key</label>
          <input
            value={step.primary_workflow_key}
            onChange={(event) => onChange({ primary_workflow_key: event.target.value })}
            list={workflowDatalistId}
            className="agentdash-form-input mt-1 text-sm"
            placeholder="trellis_dev_task_implement"
          />
          <datalist id={workflowDatalistId}>
            {availableWorkflows.map((workflow) => (
              <option key={workflow.id} value={workflow.key}>
                {workflow.name}
              </option>
            ))}
          </datalist>
          <p className="mt-1 text-[10px] text-muted-foreground/70">
            可选 Contract: {availableWorkflows.map((workflow) => workflow.key).join(" / ") || "暂无"}
          </p>
        </div>
        <div>
          <label className="text-[11px] text-muted-foreground">Session Binding</label>
          <select
            value={step.session_binding}
            onChange={(event) => onChange({
              session_binding: event.target.value as LifecycleStepDefinition["session_binding"],
            })}
            className="agentdash-form-select mt-1 text-sm"
          >
            <option value="not_required">不要求 Session</option>
            <option value="optional">Session 可选</option>
            <option value="required">必须挂接 Session</option>
          </select>
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-3">
        <div>
          <label className="text-[11px] text-muted-foreground">Transition Policy</label>
          <select
            value={step.transition_policy}
            onChange={(event) => onChange({
              transition_policy: event.target.value as LifecycleStepDefinition["transition_policy"],
            })}
            className="agentdash-form-select mt-1 text-sm"
          >
            {Object.entries(TRANSITION_POLICY_LABEL).map(([key, label]) => (
              <option key={key} value={key}>{label}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="text-[11px] text-muted-foreground">Next Step Key</label>
          <input
            value={step.next_step_key ?? ""}
            onChange={(event) => onChange({ next_step_key: event.target.value || null })}
            list={nextStepDatalistId}
            className="agentdash-form-input mt-1 text-sm"
            placeholder="check"
          />
          <datalist id={nextStepDatalistId}>
            {availableStepKeys
              .filter((key) => key && key !== step.key)
              .map((key) => (
                <option key={key} value={key} />
              ))}
          </datalist>
        </div>
        <div>
          <label className="text-[11px] text-muted-foreground">Action Key</label>
          <input
            value={step.action_key ?? ""}
            onChange={(event) => onChange({ action_key: event.target.value || null })}
            className="agentdash-form-input mt-1 text-sm"
            placeholder="record_complete"
          />
        </div>
      </div>

      <TerminalStatePicker
        value={step.session_terminal_states}
        onChange={onChangeTerminalStates}
      />

      <StepAttachmentEditor
        key={`attachments:${JSON.stringify(step.attached_workflows)}`}
        value={step.attached_workflows}
        onCommit={onCommitAttachments}
      />
    </div>
  );
}

export function LifecycleEditor({ embedded }: { embedded?: boolean } = {}) {
  const navigate = useNavigate();
  const draft = useWorkflowStore((state) => state.lifecycleEditorDraft);
  const originalId = useWorkflowStore((state) => state.lifecycleEditorOriginalId);
  const validation = useWorkflowStore((state) => state.lifecycleEditorValidation);
  const isSaving = useWorkflowStore((state) => state.lifecycleEditorIsSaving);
  const isValidating = useWorkflowStore((state) => state.lifecycleEditorIsValidating);
  const isDirty = useWorkflowStore((state) => state.lifecycleEditorDirty);
  const error = useWorkflowStore((state) => state.error);
  const lifecycleDefinitions = useWorkflowStore((state) => state.lifecycleDefinitions);
  const workflowDefinitions = useWorkflowStore((state) => state.definitions);

  const closeLifecycleDraft = useWorkflowStore((state) => state.closeLifecycleDraft);
  const updateLifecycleDraft = useWorkflowStore((state) => state.updateLifecycleDraft);
  const updateLifecycleStep = useWorkflowStore((state) => state.updateLifecycleStep);
  const addLifecycleStep = useWorkflowStore((state) => state.addLifecycleStep);
  const removeLifecycleStep = useWorkflowStore((state) => state.removeLifecycleStep);
  const updateLifecycleStepAttachments = useWorkflowStore((state) => state.updateLifecycleStepAttachments);
  const updateLifecycleStepTerminalStates = useWorkflowStore((state) => state.updateLifecycleStepTerminalStates);
  const validateLifecycleDraft = useWorkflowStore((state) => state.validateLifecycleDraft);
  const saveLifecycleDraft = useWorkflowStore((state) => state.saveLifecycleDraft);
  const fetchDefinitions = useWorkflowStore((state) => state.fetchDefinitions);
  const targetKind = draft?.target_kind;

  const currentDefinition = useMemo(() => {
    if (!originalId) return null;
    return lifecycleDefinitions.find((definition) => definition.id === originalId) ?? null;
  }, [lifecycleDefinitions, originalId]);

  useEffect(() => {
    if (!targetKind) return;
    void fetchDefinitions(targetKind);
  }, [fetchDefinitions, targetKind]);

  if (!draft) return null;

  const isNew = originalId === null;
  const hasValidationErrors = validation?.issues.some((item) => item.severity === "error") ?? false;
  const availableWorkflows = workflowDefinitions
    .filter((definition) => definition.target_kind === draft.target_kind)
    .slice()
    .sort((left, right) => left.name.localeCompare(right.name, "zh-CN"));
  const availableStepKeys = draft.steps.map((step) => step.key).filter(Boolean);

  const handleSave = async () => {
    const result = await validateLifecycleDraft();
    if (result && result.issues.some((issue) => issue.severity === "error")) return;
    await saveLifecycleDraft();
  };

  return (
    <div className="rounded-[16px] border border-primary/20 bg-background shadow-lg">
      <div className="flex items-center justify-between gap-3 border-b border-border px-5 py-4">
        <div className="min-w-0">
          <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
            {isNew ? "新建 Lifecycle Definition" : "编辑 Lifecycle Definition"}
          </p>
          <p className="mt-1 truncate text-sm font-medium text-foreground">
            {draft.name || "(未命名 Lifecycle)"}
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
        <div className="flex shrink-0 items-center gap-2">
          {isDirty && (
            <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-700">
              未保存修改
            </span>
          )}
          <button
            type="button"
            onClick={() => void validateLifecycleDraft()}
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
              closeLifecycleDraft();
              if (!embedded) navigate("/dashboard/workflow");
            }}
            className="rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:bg-secondary"
          >
            关闭
          </button>
        </div>
      </div>

      {validation && (
        <div className="border-b border-border px-5 py-3">
          <ValidationPanel issues={validation.issues} />
        </div>
      )}

      {error && (
        <div className="border-b border-destructive/30 bg-destructive/5 px-5 py-2.5">
          <p className="text-[11px] text-destructive">{error}</p>
        </div>
      )}

      <div className="space-y-6 px-5 py-5">
        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">基本信息</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="text-[11px] text-muted-foreground">Key</label>
              <input
                value={draft.key}
                onChange={(event) => updateLifecycleDraft({ key: event.target.value })}
                disabled={!isNew}
                className="agentdash-form-input mt-1 text-sm disabled:opacity-60"
                placeholder="task_lifecycle_v2"
              />
              {!isNew && (
                <p className="mt-0.5 text-[10px] text-muted-foreground/60">Key 在创建后不可修改</p>
              )}
            </div>
            <div>
              <label className="text-[11px] text-muted-foreground">名称</label>
              <input
                value={draft.name}
                onChange={(event) => updateLifecycleDraft({ name: event.target.value })}
                className="agentdash-form-input mt-1 text-sm"
                placeholder="Task Lifecycle V2"
              />
            </div>
          </div>

          <div>
            <label className="text-[11px] text-muted-foreground">描述</label>
            <textarea
              value={draft.description}
              onChange={(event) => updateLifecycleDraft({ description: event.target.value })}
              rows={2}
              className="agentdash-form-textarea mt-1"
              placeholder="描述该 lifecycle 如何为 agent 分配阶段性 workflow"
            />
          </div>

          <div className="grid gap-3 sm:grid-cols-3">
            <div>
              <label className="text-[11px] text-muted-foreground">适用目标类型</label>
              <select
                value={draft.target_kind}
                onChange={(event) => updateLifecycleDraft({
                  target_kind: event.target.value as WorkflowTargetKind,
                })}
                disabled={!isNew}
                className="agentdash-form-select mt-1 text-sm disabled:opacity-60"
              >
                {Object.entries(TARGET_KIND_LABEL).map(([key, label]) => (
                  <option key={key} value={key}>{label}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-[11px] text-muted-foreground">推荐角色</label>
              <select
                value={draft.recommended_role ?? ""}
                onChange={(event) => updateLifecycleDraft({
                  recommended_role: (event.target.value || null) as WorkflowAgentRole | null,
                })}
                className="agentdash-form-select mt-1 text-sm"
              >
                <option value="">(无推荐角色)</option>
                {ROLE_ORDER.map((role) => (
                  <option key={role} value={role}>{ROLE_LABEL[role]}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-[11px] text-muted-foreground">Entry Step Key</label>
              <input
                value={draft.entry_step_key}
                onChange={(event) => updateLifecycleDraft({ entry_step_key: event.target.value })}
                list="entry-step-key-options"
                className="agentdash-form-input mt-1 text-sm"
                placeholder="start"
              />
              <datalist id="entry-step-key-options">
                {availableStepKeys.map((key) => (
                  <option key={key} value={key} />
                ))}
              </datalist>
            </div>
          </div>
        </section>

        <section className="space-y-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                Lifecycle Steps ({draft.steps.length})
              </h3>
              <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
                每个 step 都会把一个 primary workflow 挂载为当前 agent 行为定义，并决定何时进入下一步。
              </p>
            </div>
            <button
              type="button"
              onClick={addLifecycleStep}
              className="agentdash-button-secondary text-sm"
            >
              + 添加 Step
            </button>
          </div>

          <div className="space-y-3">
            {draft.steps.map((step, index) => (
              <LifecycleStepCard
                key={`${step.key || "step"}-${index}`}
                step={step}
                index={index}
                availableWorkflows={availableWorkflows}
                availableStepKeys={availableStepKeys}
                onChange={(patch) => updateLifecycleStep(index, patch)}
                onCommitAttachments={(attachments) => updateLifecycleStepAttachments(index, attachments)}
                onChangeTerminalStates={(states) => updateLifecycleStepTerminalStates(index, states)}
                onRemove={() => removeLifecycleStep(index)}
              />
            ))}
            {draft.steps.length === 0 && (
              <div className="rounded-[12px] border border-dashed border-border bg-secondary/10 px-4 py-6 text-center text-sm text-muted-foreground">
                当前 lifecycle 还没有 step。至少需要一个 entry step 才能生效。
              </div>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
