import { useCallback, useEffect, useMemo, useState } from "react";

import type {
  WorkflowCheckKind,
  WorkflowCheckSpec,
  WorkflowCompletionSpec,
  WorkflowInjectionSpec,
  WorkflowTargetKind,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import {
  DEFINITION_STATUS_LABEL,
  TARGET_KIND_LABEL,
} from "./shared-labels";
import { BindingEditor } from "./binding-editor";
import { ValidationPanel } from "./ui/validation-panel";
import { DetailSection } from "../../components/ui/detail-panel";

const CHECK_KIND_LABEL: Record<WorkflowCheckKind, string> = {
  artifact_exists: "产物已提交",
  artifact_count_gte: "产物数量 ≥",
  session_terminal_in: "会话终态匹配",
  checklist_evidence_present: "检查清单已完成",
  explicit_action_received: "显式确认操作",
  custom: "自定义",
};

function InstructionListEditor({
  values,
  onChange,
}: {
  values: string[];
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
      <label className="agentdash-form-label">注入指令 ({values.length})</label>
      <p className="mb-1.5 text-[11px] text-muted-foreground">
        Session 启动时注入给 Agent 的行为指令，按数组顺序拼接到 system prompt。
      </p>
      <div className="space-y-1.5">
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
          placeholder="添加一条注入指令…"
        />
        <button type="button" onClick={addItem} className="agentdash-button-secondary shrink-0 text-sm">
          添加
        </button>
      </div>
    </div>
  );
}

function CheckItemEditor({
  spec,
  onChange,
  onRemove,
}: {
  spec: WorkflowCheckSpec;
  index: number;
  onChange: (patch: Partial<WorkflowCheckSpec>) => void;
  onRemove: () => void;
}) {
  return (
    <div className="flex items-center gap-2 rounded-[10px] border border-border bg-background p-2.5">
      <div className="min-w-0 flex-1 grid gap-2 sm:grid-cols-3">
        <input
          value={spec.key}
          onChange={(e) => onChange({ key: e.target.value })}
          className="agentdash-form-input text-xs"
          placeholder="check_key"
        />
        <select
          value={spec.kind}
          onChange={(e) => onChange({ kind: e.target.value as WorkflowCheckKind })}
          className="agentdash-form-select text-xs"
        >
          {Object.entries(CHECK_KIND_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
        <input
          value={spec.description}
          onChange={(e) => onChange({ description: e.target.value })}
          className="agentdash-form-input text-xs"
          placeholder="检查说明"
        />
      </div>
      <button
        type="button"
        onClick={onRemove}
        className="shrink-0 rounded-[6px] px-1.5 py-0.5 text-xs text-destructive hover:bg-destructive/10"
      >
        ×
      </button>
    </div>
  );
}

export function WorkflowEditor() {
  const draft = useWorkflowStore((s) => s.wfEditor.draft);
  const originalId = useWorkflowStore((s) => s.wfEditor.originalId);
  const validation = useWorkflowStore((s) => s.wfEditor.validation);
  const isSaving = useWorkflowStore((s) => s.wfEditor.isSaving);
  const isValidating = useWorkflowStore((s) => s.wfEditor.isValidating);
  const isDirty = useWorkflowStore((s) => s.wfEditor.dirty);
  const error = useWorkflowStore((s) => s.wfEditor.error);

  const updateDraft = useWorkflowStore((s) => s.updateDraft);
  const updateDraftBinding = useWorkflowStore((s) => s.updateDraftBinding);
  const addDraftBinding = useWorkflowStore((s) => s.addDraftBinding);
  const removeDraftBinding = useWorkflowStore((s) => s.removeDraftBinding);
  const validateDraft = useWorkflowStore((s) => s.validateDraft);
  const saveDraft = useWorkflowStore((s) => s.saveDraft);

  const definitions = useWorkflowStore((s) => s.definitions);
  const currentDefinition = useMemo(() => {
    if (!originalId) return null;
    return definitions.find((d) => d.id === originalId) ?? null;
  }, [originalId, definitions]);

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
  const updateCompletion = (patch: Partial<WorkflowCompletionSpec>) => {
    updateContract({ completion: { ...draft.contract.completion, ...patch } });
  };

  const updateCheck = (idx: number, patch: Partial<WorkflowCheckSpec>) => {
    const next = [...draft.contract.completion.checks];
    next[idx] = { ...next[idx], ...patch };
    updateCompletion({ checks: next });
  };
  const addCheck = () => {
    updateCompletion({
      checks: [...draft.contract.completion.checks, { key: "", kind: "checklist_evidence_present", description: "", payload: null }],
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
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="agentdash-form-label">描述</label>
            <textarea value={draft.description} onChange={(e) => updateDraft({ description: e.target.value })} rows={2} className="agentdash-form-textarea" placeholder="这个 Workflow 做什么" />
          </div>
          <div>
            <label className="agentdash-form-label">挂载类型</label>
            <select value={draft.target_kind} onChange={(e) => updateDraft({ target_kind: e.target.value as WorkflowTargetKind })} disabled={!isNew} className="agentdash-form-select disabled:opacity-60">
              {Object.entries(TARGET_KIND_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
            </select>
            <p className="mt-1 text-[11px] text-muted-foreground">决定此 Workflow 挂载到哪类实体（Project/Story/Task）。</p>
          </div>
        </div>
      </DetailSection>

      {/* Session 注入 */}
      <DetailSection title="Session 注入" description="Session 启动或 Workflow 切换时，hook 向 Agent 上下文注入的内容。">
        <InstructionListEditor
          values={draft.contract.injection.instructions}
          onChange={(instructions) => updateInjection({ instructions })}
        />
      </DetailSection>

      {/* Context Bindings */}
      <DetailSection
        title={`上下文挂载 (${draft.contract.injection.context_bindings.length})`}
        description="Session 启动时自动挂载的外部上下文资源。"
        extra={<button type="button" onClick={addDraftBinding} className="agentdash-button-secondary text-sm">+ 添加</button>}
      >
        <div className="space-y-2">
          {draft.contract.injection.context_bindings.map((binding, idx) => (
            <BindingEditor
              key={`${binding.locator}:${idx}`}
              binding={binding}
              index={idx}
              onChange={(patch) => updateDraftBinding(idx, patch)}
              onRemove={() => removeDraftBinding(idx)}
            />
          ))}
          {draft.contract.injection.context_bindings.length === 0 && (
            <p className="py-4 text-center text-sm text-muted-foreground">暂无上下文挂载</p>
          )}
        </div>
      </DetailSection>

      {/* 完成条件 */}
      <DetailSection
        title={`完成条件 (${draft.contract.completion.checks.length})`}
        description="BeforeStop hook 评估的条件，全部满足后 step 才可推进。"
        extra={<button type="button" onClick={addCheck} className="agentdash-button-secondary text-sm">+ 添加</button>}
      >
        <div className="space-y-2">
          {draft.contract.completion.checks.map((c, idx) => (
            <CheckItemEditor key={`ck-${idx}`} spec={c} index={idx} onChange={(p) => updateCheck(idx, p)} onRemove={() => removeCheck(idx)} />
          ))}
          {draft.contract.completion.checks.length === 0 && (
            <p className="py-4 text-center text-sm text-muted-foreground">无完成条件（手动推进）</p>
          )}
        </div>
      </DetailSection>

      {/* 既有 Constraints（只读，存在时才展示） */}
      {draft.contract.constraints.length > 0 && (
        <DetailSection title={`行为约束 (${draft.contract.constraints.length})`} description="由 hook 规则引擎管理，不支持手动编辑。">
          <div className="space-y-1.5">
            {draft.contract.constraints.map((c, idx) => (
              <div key={`cs-${idx}`} className="flex items-center gap-2 rounded-[8px] border border-border/60 bg-secondary/20 px-3 py-2">
                <span className="rounded bg-secondary px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground">{c.kind}</span>
                <span className="text-xs text-foreground/80">{c.description || c.key}</span>
              </div>
            ))}
          </div>
        </DetailSection>
      )}
    </div>
  );
}
