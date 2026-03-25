import { useCallback, useEffect, useMemo, useState } from "react";

import type {
  WorkflowAgentRole,
  WorkflowCheckKind,
  WorkflowCheckSpec,
  WorkflowCompletionSpec,
  WorkflowConstraintKind,
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
import { ValidationPanel } from "./ui/validation-panel";
import { DetailSection } from "../../components/ui/detail-panel";

const CONSTRAINT_KIND_LABEL: Record<WorkflowConstraintKind, string> = {
  deny_tool: "禁止工具",
  rewrite_tool_arg: "改写工具参数",
  require_approval: "需要审批",
  deny_task_status_transition: "禁止状态转换",
  require_artifact_before_exit: "退出前要求产物",
  block_stop_until_checks_pass: "检查通过前阻止停止",
  require_output_section: "要求输出段",
  require_output_artifact: "要求输出产物",
  enforce_response_style: "强制回复风格",
  custom: "自定义",
};

const CHECK_KIND_LABEL: Record<WorkflowCheckKind, string> = {
  task_status_in: "Task 状态匹配",
  artifact_exists: "产物存在",
  artifact_count_gte: "产物数量 >=",
  session_terminal_in: "Session 终态匹配",
  checklist_evidence_present: "检查清单证据",
  custom: "自定义",
};

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
      <label className="agentdash-form-label">{label} ({values.length})</label>
      <div className="mt-1.5 space-y-1.5">
        {values.map((value, index) => (
          <div key={`${value}-${index}`} className="flex items-start gap-2">
            <p className="flex-1 rounded-[8px] border border-border bg-secondary/20 px-2 py-1.5 text-xs text-foreground/80 leading-5">
              {value}
            </p>
            <button
              type="button"
              onClick={() => onChange(values.filter((_, i) => i !== index))}
              className="shrink-0 rounded-[6px] px-1.5 py-0.5 text-xs text-destructive hover:bg-destructive/10"
            >
              ×
            </button>
          </div>
        ))}
      </div>
      <div className="mt-2 flex gap-2">
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); addItem(); } }}
          className="agentdash-form-input flex-1 text-sm"
          placeholder={placeholder}
        />
        <button type="button" onClick={addItem} className="agentdash-button-secondary shrink-0 text-sm">
          添加
        </button>
      </div>
    </div>
  );
}

function ConstraintItemEditor({
  spec,
  index,
  onChange,
  onRemove,
}: {
  spec: WorkflowConstraintSpec;
  index: number;
  onChange: (patch: Partial<WorkflowConstraintSpec>) => void;
  onRemove: () => void;
}) {
  const [payloadDraft, setPayloadDraft] = useState(() => JSON.stringify(spec.payload ?? {}, null, 2));

  return (
    <div className="space-y-2 rounded-[10px] border border-border bg-background p-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-foreground">Constraint #{index + 1}</span>
        <button type="button" onClick={onRemove} className="text-xs text-destructive hover:underline">删除</button>
      </div>
      <div className="grid gap-2 sm:grid-cols-3">
        <div>
          <label className="agentdash-form-label">Key</label>
          <input value={spec.key} onChange={(e) => onChange({ key: e.target.value })} className="agentdash-form-input" placeholder="deny_complete" />
        </div>
        <div>
          <label className="agentdash-form-label">类型</label>
          <select value={spec.kind} onChange={(e) => onChange({ kind: e.target.value as WorkflowConstraintKind })} className="agentdash-form-select">
            {Object.entries(CONSTRAINT_KIND_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
          </select>
        </div>
        <div>
          <label className="agentdash-form-label">描述</label>
          <input value={spec.description} onChange={(e) => onChange({ description: e.target.value })} className="agentdash-form-input" />
        </div>
      </div>
      <div>
        <label className="agentdash-form-label">Payload (JSON)</label>
        <textarea
          value={payloadDraft}
          onChange={(e) => setPayloadDraft(e.target.value)}
          onBlur={() => {
            try { onChange({ payload: JSON.parse(payloadDraft) as Record<string, unknown> }); } catch { /* keep draft */ }
          }}
          rows={2}
          className="agentdash-form-textarea font-mono text-xs"
        />
      </div>
    </div>
  );
}

function CheckItemEditor({
  spec,
  index,
  onChange,
  onRemove,
}: {
  spec: WorkflowCheckSpec;
  index: number;
  onChange: (patch: Partial<WorkflowCheckSpec>) => void;
  onRemove: () => void;
}) {
  const [payloadDraft, setPayloadDraft] = useState(() => JSON.stringify(spec.payload ?? {}, null, 2));

  return (
    <div className="space-y-2 rounded-[10px] border border-border bg-background p-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-foreground">Check #{index + 1}</span>
        <button type="button" onClick={onRemove} className="text-xs text-destructive hover:underline">删除</button>
      </div>
      <div className="grid gap-2 sm:grid-cols-3">
        <div>
          <label className="agentdash-form-label">Key</label>
          <input value={spec.key} onChange={(e) => onChange({ key: e.target.value })} className="agentdash-form-input" placeholder="task_ready" />
        </div>
        <div>
          <label className="agentdash-form-label">类型</label>
          <select value={spec.kind} onChange={(e) => onChange({ kind: e.target.value as WorkflowCheckKind })} className="agentdash-form-select">
            {Object.entries(CHECK_KIND_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
          </select>
        </div>
        <div>
          <label className="agentdash-form-label">描述</label>
          <input value={spec.description} onChange={(e) => onChange({ description: e.target.value })} className="agentdash-form-input" />
        </div>
      </div>
      <div>
        <label className="agentdash-form-label">Payload (JSON)</label>
        <textarea
          value={payloadDraft}
          onChange={(e) => setPayloadDraft(e.target.value)}
          onBlur={() => {
            try { onChange({ payload: JSON.parse(payloadDraft) as Record<string, unknown> }); } catch { /* keep draft */ }
          }}
          rows={2}
          className="agentdash-form-textarea font-mono text-xs"
        />
      </div>
    </div>
  );
}

export function WorkflowEditor({ embedded }: { embedded?: boolean } = {}) {
  const draft = useWorkflowStore((s) => s.editorDraft);
  const originalId = useWorkflowStore((s) => s.editorOriginalId);
  const validation = useWorkflowStore((s) => s.editorValidation);
  const isSaving = useWorkflowStore((s) => s.editorIsSaving);
  const isValidating = useWorkflowStore((s) => s.editorIsValidating);
  const isDirty = useWorkflowStore((s) => s.editorDirty);
  const error = useWorkflowStore((s) => s.editorError);
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
    return definitions.find((d) => d.id === originalId) ?? null;
  }, [originalId, definitions]);

  useEffect(() => { void loadBindingMetadata(); }, [loadBindingMetadata]);

  const handleSave = useCallback(async () => {
    const result = await validateDraft();
    if (result && result.issues.some((i) => i.severity === "error")) return;
    await saveDraft();
  }, [validateDraft, saveDraft]);

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

  const updateContract = (patch: Partial<typeof draft.contract>) => {
    updateDraft({ contract: { ...draft.contract, ...patch } });
  };
  const updateInjection = (patch: Partial<WorkflowInjectionSpec>) => {
    updateContract({ injection: { ...draft.contract.injection, ...patch } });
  };
  const updateHookPolicy = (patch: Partial<WorkflowHookPolicySpec>) => {
    updateContract({ hook_policy: { ...draft.contract.hook_policy, ...patch } });
  };
  const updateCompletion = (patch: Partial<WorkflowCompletionSpec>) => {
    updateContract({ completion: { ...draft.contract.completion, ...patch } });
  };

  const updateConstraint = (idx: number, patch: Partial<WorkflowConstraintSpec>) => {
    const next = [...draft.contract.hook_policy.constraints];
    next[idx] = { ...next[idx], ...patch };
    updateHookPolicy({ constraints: next });
  };
  const addConstraint = () => {
    updateHookPolicy({
      constraints: [...draft.contract.hook_policy.constraints, { key: "", kind: "deny_tool", description: "", payload: null }],
    });
  };
  const removeConstraint = (idx: number) => {
    updateHookPolicy({ constraints: draft.contract.hook_policy.constraints.filter((_, i) => i !== idx) });
  };

  const updateCheck = (idx: number, patch: Partial<WorkflowCheckSpec>) => {
    const next = [...draft.contract.completion.checks];
    next[idx] = { ...next[idx], ...patch };
    updateCompletion({ checks: next });
  };
  const addCheck = () => {
    updateCompletion({
      checks: [...draft.contract.completion.checks, { key: "", kind: "task_status_in", description: "", payload: null }],
    });
  };
  const removeCheck = (idx: number) => {
    updateCompletion({ checks: draft.contract.completion.checks.filter((_, i) => i !== idx) });
  };

  return (
    <div className="space-y-4 p-5">
      {/* 操作栏 */}
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          {isDirty && <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-700">未保存</span>}
          {currentDefinition && (
            <>
              <span className={`rounded-full border px-2 py-0.5 text-[10px] ${currentDefinition.status === "active" ? "border-emerald-300/40 bg-emerald-500/10 text-emerald-700" : "border-border bg-secondary/40 text-muted-foreground"}`}>
                {DEFINITION_STATUS_LABEL[currentDefinition.status]}
              </span>
              <span className="text-[10px] text-muted-foreground">v{currentDefinition.version}</span>
            </>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button type="button" onClick={() => void validateDraft()} disabled={isValidating} className="agentdash-button-secondary text-sm">
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
            <input value={draft.key} onChange={(e) => updateDraft({ key: e.target.value })} disabled={!isNew} className="agentdash-form-input disabled:opacity-60" placeholder="unique_workflow_key" />
          </div>
          <div>
            <label className="agentdash-form-label">名称</label>
            <input value={draft.name} onChange={(e) => updateDraft({ name: e.target.value })} className="agentdash-form-input" placeholder="Workflow 显示名" />
          </div>
        </div>
        <div>
          <label className="agentdash-form-label">描述</label>
          <textarea value={draft.description} onChange={(e) => updateDraft({ description: e.target.value })} rows={2} className="agentdash-form-textarea" placeholder="Workflow 描述" />
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="agentdash-form-label">目标类型</label>
            <select value={draft.target_kind} onChange={(e) => updateDraft({ target_kind: e.target.value as WorkflowTargetKind })} disabled={!isNew} className="agentdash-form-select disabled:opacity-60">
              {Object.entries(TARGET_KIND_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
            </select>
          </div>
          <div>
            <label className="agentdash-form-label">推荐角色</label>
            <select value={draft.recommended_role ?? ""} onChange={(e) => updateDraft({ recommended_role: (e.target.value || null) as WorkflowAgentRole | null })} className="agentdash-form-select">
              <option value="">(无)</option>
              {ROLE_ORDER.map((r) => <option key={r} value={r}>{ROLE_LABEL[r]}</option>)}
            </select>
          </div>
        </div>
      </DetailSection>

      {/* 注入配置 */}
      <DetailSection title="输入注入" description="定义 workflow 注入到 agent 的 goal / instructions / context bindings。">
        <div>
          <label className="agentdash-form-label">Goal</label>
          <textarea value={draft.contract.injection.goal ?? ""} onChange={(e) => updateInjection({ goal: e.target.value || null })} rows={2} className="agentdash-form-textarea" placeholder="当前 workflow 的目标" />
        </div>
        <div>
          <label className="agentdash-form-label">Session Binding</label>
          <select value={draft.contract.injection.session_binding} onChange={(e) => updateInjection({ session_binding: e.target.value as WorkflowInjectionSpec["session_binding"] })} className="agentdash-form-select">
            <option value="not_required">不要求</option>
            <option value="optional">可选</option>
            <option value="required">必须</option>
          </select>
        </div>
        <StringListEditor label="注入指令" values={draft.contract.injection.instructions} placeholder="新增一条 workflow 注入指令…" onChange={(instructions) => updateInjection({ instructions })} />
      </DetailSection>

      {/* Context Bindings */}
      <DetailSection
        title={`Context Bindings (${draft.contract.injection.context_bindings.length})`}
        extra={<button type="button" onClick={addDraftBinding} className="agentdash-button-secondary text-sm">+ 添加</button>}
      >
        <div className="space-y-2">
          {draft.contract.injection.context_bindings.map((binding, idx) => (
            <BindingEditor
              key={`${binding.kind}:${binding.locator}:${idx}`}
              binding={binding}
              index={idx}
              targetKind={draft.target_kind}
              bindingMetadata={bindingMetadata}
              onChange={(patch) => updateDraftBinding(idx, patch)}
              onRemove={() => removeDraftBinding(idx)}
            />
          ))}
          {draft.contract.injection.context_bindings.length === 0 && (
            <p className="py-4 text-center text-sm text-muted-foreground">暂无 Context Binding</p>
          )}
        </div>
      </DetailSection>

      {/* Hook 策略 Constraints */}
      <DetailSection
        title={`Hook Constraints (${draft.contract.hook_policy.constraints.length})`}
        description="约束 agent 在此 workflow 下的行为边界。"
        extra={<button type="button" onClick={addConstraint} className="agentdash-button-secondary text-sm">+ 添加</button>}
      >
        <div className="space-y-2">
          {draft.contract.hook_policy.constraints.map((c, idx) => (
            <ConstraintItemEditor key={`c-${idx}`} spec={c} index={idx} onChange={(p) => updateConstraint(idx, p)} onRemove={() => removeConstraint(idx)} />
          ))}
          {draft.contract.hook_policy.constraints.length === 0 && (
            <p className="py-4 text-center text-sm text-muted-foreground">暂无约束规则</p>
          )}
        </div>
      </DetailSection>

      {/* 完成检查 */}
      <DetailSection
        title={`完成检查 (${draft.contract.completion.checks.length})`}
        description="workflow 完成的条件检查列表。"
        extra={<button type="button" onClick={addCheck} className="agentdash-button-secondary text-sm">+ 添加</button>}
      >
        <div className="space-y-2">
          {draft.contract.completion.checks.map((c, idx) => (
            <CheckItemEditor key={`ck-${idx}`} spec={c} index={idx} onChange={(p) => updateCheck(idx, p)} onRemove={() => removeCheck(idx)} />
          ))}
          {draft.contract.completion.checks.length === 0 && (
            <p className="py-4 text-center text-sm text-muted-foreground">暂无完成检查</p>
          )}
        </div>
        <div className="grid gap-3 sm:grid-cols-2 mt-3">
          <div>
            <label className="agentdash-form-label">默认 Artifact 类型</label>
            <select value={draft.contract.completion.default_artifact_type ?? ""} onChange={(e) => updateCompletion({ default_artifact_type: (e.target.value || null) as WorkflowRecordArtifactType | null })} className="agentdash-form-select">
              <option value="">(无)</option>
              {Object.entries(ARTIFACT_TYPE_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
            </select>
          </div>
          <div>
            <label className="agentdash-form-label">默认 Artifact 标题</label>
            <input value={draft.contract.completion.default_artifact_title ?? ""} onChange={(e) => updateCompletion({ default_artifact_title: e.target.value || null })} className="agentdash-form-input" placeholder="可选标题" />
          </div>
        </div>
      </DetailSection>

      {/* Record Policy */}
      <DetailSection title="Record Policy">
        <div className="flex flex-wrap gap-x-6 gap-y-2">
          <label className="flex items-center gap-2 text-xs text-foreground">
            <input type="checkbox" checked={draft.record_policy.emit_summary} onChange={(e) => updateDraft({ record_policy: { ...draft.record_policy, emit_summary: e.target.checked } })} />
            产出摘要
          </label>
          <label className="flex items-center gap-2 text-xs text-foreground">
            <input type="checkbox" checked={draft.record_policy.emit_journal_update} onChange={(e) => updateDraft({ record_policy: { ...draft.record_policy, emit_journal_update: e.target.checked } })} />
            更新日志
          </label>
          <label className="flex items-center gap-2 text-xs text-foreground">
            <input type="checkbox" checked={draft.record_policy.emit_archive_suggestion} onChange={(e) => updateDraft({ record_policy: { ...draft.record_policy, emit_archive_suggestion: e.target.checked } })} />
            归档建议
          </label>
        </div>
      </DetailSection>
    </div>
  );
}
