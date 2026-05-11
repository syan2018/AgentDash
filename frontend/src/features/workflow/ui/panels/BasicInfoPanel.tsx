/**
 * 基础信息 Panel —— Workflow 的 key / name / description / target_kinds。
 *
 * 视觉语言对齐 Overview：线性字段（`agentdash-form-label` + 控件），
 * 不包 DetailSection、不加平铺 description 注释。
 */

import type { WorkflowTargetKind } from "../../../../types";
import { TARGET_KIND_LABEL, TARGET_KIND_OPTIONS } from "../../shared-labels";
import { toggleTargetKind } from "./shared";

export interface BasicInfoPanelProps {
  draftKey: string;
  name: string;
  description: string;
  targetKinds: WorkflowTargetKind[];
  keyDisabled: boolean;
  onKeyChange: (next: string) => void;
  onNameChange: (next: string) => void;
  onDescriptionChange: (next: string) => void;
  onTargetKindsChange: (next: WorkflowTargetKind[]) => void;
}

export function BasicInfoPanel({
  draftKey,
  name,
  description,
  targetKinds,
  keyDisabled,
  onKeyChange,
  onNameChange,
  onDescriptionChange,
  onTargetKindsChange,
}: BasicInfoPanelProps) {
  return (
    <section className="space-y-3">
      <div>
        <label className="agentdash-form-label">Key</label>
        <input
          value={draftKey}
          onChange={(e) => onKeyChange(e.target.value)}
          disabled={keyDisabled}
          className="agentdash-form-input disabled:opacity-60"
          placeholder="unique_workflow_key"
        />
      </div>

      <div>
        <label className="agentdash-form-label">名称</label>
        <input
          value={name}
          onChange={(e) => onNameChange(e.target.value)}
          className="agentdash-form-input"
          placeholder="Workflow 显示名"
        />
      </div>

      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={description}
          onChange={(e) => onDescriptionChange(e.target.value)}
          rows={2}
          className="agentdash-form-textarea"
          placeholder="这个 Workflow 做什么"
        />
      </div>

      <div>
        <label className="agentdash-form-label">挂载类型</label>
        <div className="flex flex-wrap gap-2">
          {TARGET_KIND_OPTIONS.map((kind) => {
            const checked = targetKinds.includes(kind);
            return (
              <label
                key={kind}
                className={`flex cursor-pointer items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-xs transition-colors ${
                  checked
                    ? "border-primary/40 bg-primary/5 text-foreground"
                    : "border-border bg-background text-muted-foreground hover:border-primary/20"
                }`}
              >
                <input
                  type="checkbox"
                  checked={checked}
                  onChange={() => onTargetKindsChange(toggleTargetKind(targetKinds, kind))}
                  className="sr-only"
                />
                {TARGET_KIND_LABEL[kind]}
              </label>
            );
          })}
        </div>
      </div>
    </section>
  );
}
