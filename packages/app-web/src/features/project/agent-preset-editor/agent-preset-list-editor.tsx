import { useState } from "react";
import type { AgentPreset } from "../../../types";
import {
  formToPreset,
  formatPresetSummary,
  presetToForm,
  validateForm,
} from "./form-state";
import type { PresetFormState } from "./form-state";
import { PresetFormFields, useAgentTypeOptions } from "./preset-form-fields";

export interface AgentPresetEditorProps {
  presets: AgentPreset[];
  onSave: (presets: AgentPreset[]) => Promise<void>;
  isSaving?: boolean;
}

export function AgentPresetEditor({ presets, onSave, isSaving = false }: AgentPresetEditorProps) {
  const { agentTypeOptions, isDiscoveryLoading } = useAgentTypeOptions();
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [form, setForm] = useState<PresetFormState>(presetToForm());
  const [validationError, setValidationError] = useState<string | null>(null);

  const existingNames = presets.map((p) => p.name);
  const isFormOpen = isCreating || editingIndex !== null;

  const startCreate = () => {
    setForm(presetToForm());
    setEditingIndex(null);
    setIsCreating(true);
    setValidationError(null);
  };

  const startEdit = (index: number) => {
    setForm(presetToForm(presets[index]));
    setEditingIndex(index);
    setIsCreating(false);
    setValidationError(null);
  };

  const cancel = () => {
    setEditingIndex(null);
    setIsCreating(false);
    setValidationError(null);
  };

  const handleSave = async () => {
    const editingName = editingIndex != null ? presets[editingIndex]?.name : undefined;
    const err = validateForm(form, existingNames, editingName);
    if (err) { setValidationError(err); return; }
    const preset = formToPreset(form);
    const next = isCreating
      ? [...presets, preset]
      : presets.map((p, i) => (i === editingIndex ? preset : p));
    await onSave(next);
    cancel();
  };

  const handleDelete = async (index: number) => {
    await onSave(presets.filter((_, i) => i !== index));
  };

  const patchForm = (patch: Partial<PresetFormState>) => {
    setForm((prev) => ({ ...prev, ...patch }));
    setValidationError(null);
  };

  return (
    <div className="space-y-2.5">
      {presets.length === 0 && !isFormOpen && (
        <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-center text-xs text-muted-foreground">
          暂无 Agent 预设，点击下方按钮添加
        </p>
      )}

      {presets.map((preset, index) => (
        <div
          key={`${preset.name}-${index}`}
          className="flex items-center justify-between rounded-[12px] border border-border bg-secondary/30 px-4 py-3"
        >
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-foreground">{preset.name}</p>
            <p className="mt-0.5 truncate text-xs text-muted-foreground">
              {formatPresetSummary(preset)}
            </p>
          </div>
          <div className="ml-3 flex gap-1.5">
            <button
              type="button"
              onClick={() => startEdit(index)}
              disabled={isSaving || isFormOpen}
              className="rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground disabled:opacity-40"
            >
              编辑
            </button>
            <button
              type="button"
              onClick={() => void handleDelete(index)}
              disabled={isSaving || isFormOpen}
              className="rounded-[8px] border border-destructive/30 bg-background px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-40"
            >
              删除
            </button>
          </div>
        </div>
      ))}

      {isFormOpen && (
        <div className="space-y-3 rounded-[12px] border border-primary/30 bg-background p-4">
          <p className="text-sm font-medium text-foreground">
            {isCreating ? "新建 Agent 预设" : `编辑预设: ${presets[editingIndex!]?.name}`}
          </p>

          <PresetFormFields
            form={form}
            patchForm={patchForm}
            agentTypeOptions={agentTypeOptions}
            isDiscoveryLoading={isDiscoveryLoading}
          />

          {validationError && (
            <p className="mt-2 text-xs text-destructive">{validationError}</p>
          )}

          <div className="flex justify-end gap-2 border-t border-border pt-3">
            <button type="button" onClick={cancel} disabled={isSaving} className="agentdash-button-secondary">
              取消
            </button>
            <button type="button" onClick={() => void handleSave()} disabled={isSaving} className="agentdash-button-primary">
              {isSaving ? "保存中..." : "保存"}
            </button>
          </div>
        </div>
      )}

      {!isFormOpen && (
        <button
          type="button"
          onClick={startCreate}
          disabled={isSaving}
          className="w-full rounded-[12px] border border-dashed border-border py-2.5 text-sm text-muted-foreground transition-colors hover:border-primary/50 hover:text-foreground disabled:opacity-40"
        >
          + 添加 Agent 预设
        </button>
      )}
    </div>
  );
}
