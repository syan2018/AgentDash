import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";

import type {
  ValidationIssue,
  WorkflowAgentRole,
  WorkflowCheckSpec,
  WorkflowCompletionSpec,
  WorkflowConstraintSpec,
  WorkflowHookPolicySpec,
  WorkflowInjectionSpec,
  WorkflowRecordArtifactType,
  WorkflowTargetKind,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import {
  ARTIFACT_TYPE_LABEL,
  DEFINITION_STATUS_LABEL,
  ROLE_LABEL,
  ROLE_ORDER,
  TARGET_KIND_LABEL,
} from "./shared-labels";
import { BindingEditor } from "./binding-editor";

function ValidationPanel({ issues }: { issues: ValidationIssue[] }) {
  const errors = issues.filter((i) => i.severity === "error");
  const warnings = issues.filter((i) => i.severity === "warning");

  return (
    <div className="space-y-2">
      {errors.length > 0 && (
        <div className="rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2.5 space-y-1.5">
          <p className="text-[11px] font-medium text-destructive">{errors.length} 个错误</p>
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
          <p className="text-[11px] font-medium text-amber-700">{warnings.length} 个警告</p>
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

function JsonArrayEditor<T>({
  label,
  value,
  placeholder,
  onCommit,
}: {
  label: string;
  value: T[];
  placeholder: string;
  onCommit: (next: T[]) => void;
}) {
  const [draft, setDraft] = useState(() => JSON.stringify(value, null, 2));
  const [error, setError] = useState<string | null>(null);

  const commit = () => {
    try {
      const parsed = JSON.parse(draft) as unknown;
      if (!Array.isArray(parsed)) {
        setError("必须是 JSON 数组");
        return;
      }
      onCommit(parsed as T[]);
      setError(null);
    } catch (err) {
      setError((err as Error).message);
    }
  };

  return (
    <div>
      <div className="flex items-center justify-between gap-2">
        <label className="text-[11px] text-muted-foreground">{label}</label>
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
        rows={8}
        placeholder={placeholder}
        className="agentdash-form-textarea mt-1 font-mono text-xs"
      />
      {error && <p className="mt-1 text-[11px] text-destructive">{error}</p>}
    </div>
  );
}

function StringListEditor({
  label,
  values,
  placeholder,
  onChange,
}: {
  label: string;
  values: string[];
  placeholder: string;
  onChange: (next: string[]) => void;
}) {
  const [draft, setDraft] = useState("");

  const addItem = () => {
    const trimmed = draft.trim();
    if (!trimmed) return;
    onChange([...values, trimmed]);
    setDraft("");
  };

  return (
    <div>
      <label className="text-[11px] text-muted-foreground">
        {label} ({values.length})
      </label>
      <div className="mt-2 space-y-1.5">
        {values.map((value, index) => (
          <div key={`${value}-${index}`} className="flex items-start gap-2">
            <p className="flex-1 rounded-md border border-border bg-secondary/20 px-2 py-1.5 text-[11px] text-foreground/80 leading-5">
              {value}
            </p>
            <button
              type="button"
              onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))}
              className="shrink-0 rounded px-1.5 py-0.5 text-[11px] text-destructive hover:bg-destructive/10"
            >
              ×
            </button>
          </div>
        ))}
      </div>
      <div className="mt-2 flex gap-2">
        <input
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              addItem();
            }
          }}
          className="agentdash-form-input flex-1 text-sm"
          placeholder={placeholder}
        />
        <button
          type="button"
          onClick={addItem}
          className="agentdash-button-secondary shrink-0 text-sm"
        >
          添加
        </button>
      </div>
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
  const updateDraftBinding = useWorkflowStore((s) => s.updateDraftBinding);
  const addDraftBinding = useWorkflowStore((s) => s.addDraftBinding);
  const removeDraftBinding = useWorkflowStore((s) => s.removeDraftBinding);
  const validateDraft = useWorkflowStore((s) => s.validateDraft);
  const saveDraft = useWorkflowStore((s) => s.saveDraft);
  const loadBindingMetadata = useWorkflowStore((s) => s.loadBindingMetadata);

  const definitions = useWorkflowStore((s) => s.definitions);
  const currentDefinition = useMemo(() => {
    if (!originalId) return null;
    return definitions.find((definition) => definition.id === originalId) ?? null;
  }, [originalId, definitions]);

  useEffect(() => {
    void loadBindingMetadata();
  }, [loadBindingMetadata]);

  if (!draft) return null;

  const isNew = originalId === null;
  const hasValidationErrors = validation?.issues.some((issue) => issue.severity === "error") ?? false;

  const updateContract = (patch: Partial<typeof draft.contract>) => {
    updateDraft({
      contract: {
        ...draft.contract,
        ...patch,
      },
    });
  };

  const updateInjection = (patch: Partial<WorkflowInjectionSpec>) => {
    updateContract({
      injection: {
        ...draft.contract.injection,
        ...patch,
      },
    });
  };

  const updateHookPolicy = (patch: Partial<WorkflowHookPolicySpec>) => {
    updateContract({
      hook_policy: {
        ...draft.contract.hook_policy,
        ...patch,
      },
    });
  };

  const updateCompletion = (patch: Partial<WorkflowCompletionSpec>) => {
    updateContract({
      completion: {
        ...draft.contract.completion,
        ...patch,
      },
    });
  };

  const handleSave = async () => {
    const result = await validateDraft();
    if (result && result.issues.some((issue) => issue.severity === "error")) return;
    await saveDraft();
  };

  return (
    <div className="rounded-[16px] border border-primary/20 bg-background shadow-lg">
      <div className="flex items-center justify-between gap-3 border-b border-border px-5 py-4">
        <div className="min-w-0">
          <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
            {isNew ? "新建 Workflow 定义" : "编辑 Workflow 定义"}
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

      <div className="px-5 py-5 space-y-6">
        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">基本信息</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="text-[11px] text-muted-foreground">Key</label>
              <input
                value={draft.key}
                onChange={(event) => updateDraft({ key: event.target.value })}
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
                onChange={(event) => updateDraft({ name: event.target.value })}
                className="agentdash-form-input mt-1 text-sm"
                placeholder="Workflow 显示名"
              />
            </div>
          </div>

          <div>
            <label className="text-[11px] text-muted-foreground">描述</label>
            <textarea
              value={draft.description}
              onChange={(event) => updateDraft({ description: event.target.value })}
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
                onChange={(event) => updateDraft({ target_kind: event.target.value as WorkflowTargetKind })}
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
                onChange={(event) => updateDraft({
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
          </div>
        </section>

        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">输入注入</h3>

          <div>
            <label className="text-[11px] text-muted-foreground">Goal</label>
            <textarea
              value={draft.contract.injection.goal ?? ""}
              onChange={(event) => updateInjection({ goal: event.target.value || null })}
              rows={3}
              placeholder="当前 workflow 的目标"
              className="agentdash-form-textarea mt-1"
            />
          </div>

          <div>
            <label className="text-[11px] text-muted-foreground">Session Binding</label>
            <select
              value={draft.contract.injection.session_binding}
              onChange={(event) => updateInjection({
                session_binding: event.target.value as typeof draft.contract.injection.session_binding,
              })}
              className="agentdash-form-select mt-1 text-sm"
            >
              <option value="not_required">不要求 Session</option>
              <option value="optional">Session 可选</option>
              <option value="required">必须挂接 Session</option>
            </select>
          </div>
        </section>

        <section className="space-y-4">
          <StringListEditor
            label="注入指令"
            values={draft.contract.injection.instructions}
            placeholder="新增一条 workflow 注入指令…"
            onChange={(instructions) => updateInjection({ instructions })}
          />
        </section>

        <section className="space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">
              Context Bindings ({draft.contract.injection.context_bindings.length})
            </h3>
            <button
              type="button"
              onClick={addDraftBinding}
              className="agentdash-button-secondary text-sm"
            >
              + 添加 Binding
            </button>
          </div>
          <div className="space-y-2">
            {draft.contract.injection.context_bindings.map((binding, index) => (
              <BindingEditor
                key={`${binding.kind}:${binding.locator}:${index}`}
                binding={binding}
                index={index}
                targetKind={draft.target_kind}
                bindingMetadata={bindingMetadata}
                onChange={(patch) => updateDraftBinding(index, patch)}
                onRemove={() => removeDraftBinding(index)}
              />
            ))}
            {draft.contract.injection.context_bindings.length === 0 && (
              <div className="rounded-[12px] border border-dashed border-border bg-secondary/10 px-4 py-6 text-center text-sm text-muted-foreground">
                当前 workflow 还没有绑定任何注入上下文。
              </div>
            )}
          </div>
        </section>

        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">完成检查</h3>

          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="text-[11px] text-muted-foreground">默认 Artifact 类型</label>
              <select
                value={draft.contract.completion.default_artifact_type ?? ""}
                onChange={(event) => updateCompletion({
                  default_artifact_type: (event.target.value || null) as WorkflowRecordArtifactType | null,
                })}
                className="agentdash-form-select mt-1 text-sm"
              >
                <option value="">(无)</option>
                {Object.entries(ARTIFACT_TYPE_LABEL).map(([key, label]) => (
                  <option key={key} value={key}>{label}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-[11px] text-muted-foreground">默认 Artifact 标题</label>
              <input
                value={draft.contract.completion.default_artifact_title ?? ""}
                onChange={(event) => updateCompletion({ default_artifact_title: event.target.value || null })}
                className="agentdash-form-input mt-1 text-sm"
                placeholder="可选标题"
              />
            </div>
          </div>
        </section>

        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">Hook 策略</h3>
          <JsonArrayEditor<WorkflowConstraintSpec>
            key={`constraints:${JSON.stringify(draft.contract.hook_policy.constraints)}`}
            label="Constraints"
            value={draft.contract.hook_policy.constraints}
            placeholder='[{"key":"deny_complete","kind":"deny_task_status_transition","description":"...","payload":{"to":["completed"]}}]'
            onCommit={(constraints) => updateHookPolicy({ constraints })}
          />
        </section>

        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">完成规则</h3>
          <JsonArrayEditor<WorkflowCheckSpec>
            key={`checks:${JSON.stringify(draft.contract.completion.checks)}`}
            label="Checks"
            value={draft.contract.completion.checks}
            placeholder='[{"key":"task_ready","kind":"task_status_in","description":"...","payload":{"statuses":["awaiting_verification","completed"]}}]'
            onCommit={(checks) => updateCompletion({ checks })}
          />
        </section>

        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-[0.12em] text-muted-foreground">Record Policy</h3>
          <div className="flex flex-wrap gap-x-6 gap-y-2">
            <label className="flex items-center gap-2 text-[11px] text-foreground">
              <input
                type="checkbox"
                checked={draft.record_policy.emit_summary}
                onChange={(event) => updateDraft({
                  record_policy: { ...draft.record_policy, emit_summary: event.target.checked },
                })}
              />
              产出摘要（emit_summary）
            </label>
            <label className="flex items-center gap-2 text-[11px] text-foreground">
              <input
                type="checkbox"
                checked={draft.record_policy.emit_journal_update}
                onChange={(event) => updateDraft({
                  record_policy: { ...draft.record_policy, emit_journal_update: event.target.checked },
                })}
              />
              更新日志（emit_journal_update）
            </label>
            <label className="flex items-center gap-2 text-[11px] text-foreground">
              <input
                type="checkbox"
                checked={draft.record_policy.emit_archive_suggestion}
                onChange={(event) => updateDraft({
                  record_policy: { ...draft.record_policy, emit_archive_suggestion: event.target.checked },
                })}
              />
              归档建议（emit_archive_suggestion）
            </label>
          </div>
        </section>
      </div>
    </div>
  );
}
