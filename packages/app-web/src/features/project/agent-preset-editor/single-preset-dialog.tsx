import { useEffect, useState } from "react";
import type { AgentPreset } from "../../../types";
import { formToPreset, presetToForm, validateForm } from "./form-state";
import type { PresetFormState } from "./form-state";
import { PresetFormFields, useAgentTypeOptions } from "./preset-form-fields";

export interface SinglePresetDialogProps {
  open: boolean;
  initialPreset?: AgentPreset;
  existingNames: string[];
  onSave: (preset: AgentPreset) => Promise<void>;
  onClose: () => void;
  isSaving?: boolean;
  siblingAgents?: Array<{ name: string; display_name: string }>;
  /** 是否启用 Agent 知识库 */
  knowledgeEnabled?: boolean;
  /** 切换知识库开关 */
  onToggleKnowledge?: (enabled: boolean) => void;
  /** 用于加载知识库文件的 project/agent ID */
  knowledgeProjectId?: string;
  knowledgeAgentId?: string;
}

export function SinglePresetDialog({
  open,
  initialPreset,
  existingNames,
  onSave,
  onClose,
  isSaving = false,
  siblingAgents,
  knowledgeEnabled,
  onToggleKnowledge,
  knowledgeProjectId,
  knowledgeAgentId,
}: SinglePresetDialogProps) {
  const { agentTypeOptions, isDiscoveryLoading } = useAgentTypeOptions();
  const [form, setForm] = useState<PresetFormState>(presetToForm(initialPreset));
  const [validationError, setValidationError] = useState<string | null>(null);
  const isEditing = Boolean(initialPreset);

  // 当 initialPreset 变化时（打开不同的编辑目标），重新填充表单。
  // 合法的 derived-state reset 模式；用 key 重建对话框会丢掉未保存输入及关闭动画。
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setForm(presetToForm(initialPreset));
    setValidationError(null);
  }, [initialPreset]);

  if (!open) return null;

  const handleSave = async () => {
    const err = validateForm(form, existingNames, isEditing ? initialPreset?.name : undefined);
    if (err) { setValidationError(err); return; }
    await onSave(formToPreset(form));
  };

  const patchForm = (patch: Partial<PresetFormState>) => {
    setForm((prev) => ({ ...prev, ...patch }));
    setValidationError(null);
  };

  return (
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/18 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-[min(1080px,75vw)] rounded-[12px] border border-border bg-background shadow-2xl">
          <div className="border-b border-border px-5 py-4">
            <span className="agentdash-panel-header-tag">Agent</span>
            <h4 className="text-base font-semibold text-foreground">
              {isEditing ? `编辑 Agent 预设: ${initialPreset?.name}` : "新建 Agent 预设"}
            </h4>
            <p className="mt-1 text-xs text-muted-foreground">
              配置后将出现在 Agent Hub 卡片列表中
            </p>
          </div>

          <div className="max-h-[70vh] overflow-y-auto p-5">
            <PresetFormFields
              form={form}
              patchForm={patchForm}
              agentTypeOptions={agentTypeOptions}
              isDiscoveryLoading={isDiscoveryLoading}
              siblingAgents={siblingAgents}
              projectId={knowledgeProjectId}
              knowledgeEnabled={knowledgeEnabled}
              onToggleKnowledge={onToggleKnowledge}
              knowledgeAgentId={knowledgeAgentId}
            />

            {validationError && (
              <p className="mt-2 text-xs text-destructive">{validationError}</p>
            )}
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
            <button type="button" onClick={onClose} disabled={isSaving} className="agentdash-button-secondary">取消</button>
            <button type="button" onClick={() => void handleSave()} disabled={isSaving} className="agentdash-button-primary">
              {isSaving ? "保存中..." : isEditing ? "保存修改" : "创建预设"}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
