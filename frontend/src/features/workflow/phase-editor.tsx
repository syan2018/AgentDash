import { useState } from "react";

import type {
  BindingKindMetadata,
  WorkflowContextBinding,
  WorkflowPhaseCompletionMode,
  WorkflowPhaseDefinition,
  WorkflowRecordArtifactType,
  WorkflowTargetKind,
} from "../../types";
import { COMPLETION_MODE_LABEL, ARTIFACT_TYPE_LABEL } from "./shared-labels";
import { BindingEditor } from "./binding-editor";

interface PhaseEditorProps {
  phase: WorkflowPhaseDefinition;
  index: number;
  totalPhases: number;
  targetKind: WorkflowTargetKind;
  bindingMetadata: BindingKindMetadata[];
  onUpdate: (patch: Partial<WorkflowPhaseDefinition>) => void;
  onUpdateBinding: (bindingIndex: number, patch: Partial<WorkflowContextBinding>) => void;
  onAddBinding: () => void;
  onRemoveBinding: (bindingIndex: number) => void;
  onRemove: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
}

export function PhaseEditor({
  phase,
  index,
  totalPhases,
  targetKind,
  bindingMetadata,
  onUpdate,
  onUpdateBinding,
  onAddBinding,
  onRemoveBinding,
  onRemove,
  onMoveUp,
  onMoveDown,
}: PhaseEditorProps) {
  const [expanded, setExpanded] = useState(true);
  const [instructionDraft, setInstructionDraft] = useState("");

  const addInstruction = () => {
    const trimmed = instructionDraft.trim();
    if (!trimmed) return;
    onUpdate({ agent_instructions: [...phase.agent_instructions, trimmed] });
    setInstructionDraft("");
  };

  const removeInstruction = (instrIndex: number) => {
    onUpdate({
      agent_instructions: phase.agent_instructions.filter((_, i) => i !== instrIndex),
    });
  };

  return (
    <div className="rounded-[12px] border border-border bg-background">
      <div
        className="flex items-center justify-between gap-2 px-4 py-3 cursor-pointer select-none"
        onClick={() => setExpanded((v) => !v)}
        onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") setExpanded((v) => !v); }}
        role="button"
        tabIndex={0}
      >
        <div className="flex items-center gap-2 min-w-0">
          <span className="text-[11px] text-muted-foreground shrink-0">#{index + 1}</span>
          <span className="text-sm font-medium text-foreground truncate">
            {phase.title || "(未命名阶段)"}
          </span>
          <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground shrink-0">
            {COMPLETION_MODE_LABEL[phase.completion_mode]}
          </span>
          {phase.context_bindings.length > 0 && (
            <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground shrink-0">
              {phase.context_bindings.length} binding
            </span>
          )}
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); onMoveUp(); }}
            disabled={index === 0}
            className="rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-secondary disabled:opacity-30"
            title="上移"
          >
            ↑
          </button>
          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); onMoveDown(); }}
            disabled={index === totalPhases - 1}
            className="rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-secondary disabled:opacity-30"
            title="下移"
          >
            ↓
          </button>
          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); onRemove(); }}
            className="rounded px-1.5 py-0.5 text-[11px] text-destructive hover:bg-destructive/10"
          >
            删除
          </button>
          <span className="text-muted-foreground text-[11px]">{expanded ? "▲" : "▼"}</span>
        </div>
      </div>

      {expanded && (
        <div className="border-t border-border px-4 py-4 space-y-4">
          {/* Basic fields */}
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label className="text-[11px] text-muted-foreground">Key</label>
              <input
                value={phase.key}
                onChange={(e) => onUpdate({ key: e.target.value })}
                className="agentdash-form-input mt-1 text-sm"
                placeholder="phase_key"
              />
            </div>
            <div>
              <label className="text-[11px] text-muted-foreground">标题</label>
              <input
                value={phase.title}
                onChange={(e) => onUpdate({ title: e.target.value })}
                className="agentdash-form-input mt-1 text-sm"
                placeholder="阶段显示名"
              />
            </div>
          </div>

          <div>
            <label className="text-[11px] text-muted-foreground">描述</label>
            <textarea
              value={phase.description}
              onChange={(e) => onUpdate({ description: e.target.value })}
              className="agentdash-form-input mt-1 min-h-[60px] text-sm"
              placeholder="阶段描述"
              rows={2}
            />
          </div>

          {/* Completion & session */}
          <div className="grid gap-3 sm:grid-cols-3">
            <div>
              <label className="text-[11px] text-muted-foreground">完成模式</label>
              <select
                value={phase.completion_mode}
                onChange={(e) => onUpdate({ completion_mode: e.target.value as WorkflowPhaseCompletionMode })}
                className="agentdash-form-select mt-1 text-sm"
              >
                {Object.entries(COMPLETION_MODE_LABEL).map(([k, v]) => (
                  <option key={k} value={k}>{v}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-[11px] text-muted-foreground">默认 Artifact 类型</label>
              <select
                value={phase.default_artifact_type ?? ""}
                onChange={(e) => onUpdate({
                  default_artifact_type: (e.target.value || null) as WorkflowRecordArtifactType | null,
                })}
                className="agentdash-form-select mt-1 text-sm"
              >
                <option value="">(无)</option>
                {Object.entries(ARTIFACT_TYPE_LABEL).map(([k, v]) => (
                  <option key={k} value={k}>{v}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-[11px] text-muted-foreground">默认 Artifact 标题</label>
              <input
                value={phase.default_artifact_title ?? ""}
                onChange={(e) => onUpdate({ default_artifact_title: e.target.value || null })}
                className="agentdash-form-input mt-1 text-sm"
                placeholder="可选标题"
              />
            </div>
          </div>

          <label className="flex items-center gap-2 text-[11px] text-foreground">
            <input
              type="checkbox"
              checked={phase.requires_session}
              onChange={(e) => onUpdate({ requires_session: e.target.checked })}
            />
            需要会话（requires_session）
          </label>

          {/* Agent instructions */}
          <div>
            <label className="text-[11px] text-muted-foreground">
              Agent 约束指令 ({phase.agent_instructions.length})
            </label>
            <div className="mt-2 space-y-1.5">
              {phase.agent_instructions.map((instr, instrIndex) => (
                <div key={instrIndex} className="flex items-start gap-2">
                  <p className="flex-1 rounded-md border border-border bg-secondary/20 px-2 py-1.5 text-[11px] text-foreground/80 leading-5">
                    {instr}
                  </p>
                  <button
                    type="button"
                    onClick={() => removeInstruction(instrIndex)}
                    className="shrink-0 rounded px-1.5 py-0.5 text-[11px] text-destructive hover:bg-destructive/10"
                  >
                    ×
                  </button>
                </div>
              ))}
            </div>
            <div className="mt-2 flex gap-2">
              <input
                value={instructionDraft}
                onChange={(e) => setInstructionDraft(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); addInstruction(); } }}
                className="agentdash-form-input flex-1 text-sm"
                placeholder="新增一条 agent 约束…"
              />
              <button
                type="button"
                onClick={addInstruction}
                className="agentdash-button-secondary shrink-0 text-sm"
              >
                添加
              </button>
            </div>
          </div>

          {/* Context bindings */}
          <div>
            <div className="flex items-center justify-between">
              <label className="text-[11px] text-muted-foreground">
                Context Bindings ({phase.context_bindings.length})
              </label>
              <button
                type="button"
                onClick={onAddBinding}
                className="rounded-md px-2 py-0.5 text-[11px] text-primary hover:bg-primary/10"
              >
                + 添加 Binding
              </button>
            </div>
            <div className="mt-2 space-y-2">
              {phase.context_bindings.map((binding, bindingIndex) => (
                <BindingEditor
                  key={bindingIndex}
                  binding={binding}
                  index={bindingIndex}
                  targetKind={targetKind}
                  bindingMetadata={bindingMetadata}
                  onChange={(patch) => onUpdateBinding(bindingIndex, patch)}
                  onRemove={() => onRemoveBinding(bindingIndex)}
                />
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
