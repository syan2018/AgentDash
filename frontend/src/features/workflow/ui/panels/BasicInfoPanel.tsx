/**
 * 基础信息 Panel —— Workflow 的 key / name / description / target_kinds。
 *
 * 受控组件：不直接依赖 workflowStore；调用方传入当前值和 onChange，
 * 由容器串回 store action。
 */

import type { WorkflowTargetKind } from "../../../../types";
import { TARGET_KIND_LABEL, TARGET_KIND_OPTIONS } from "../../shared-labels";
import { DetailSection } from "../../../../components/ui/detail-panel";
import { toggleTargetKind } from "./shared";

export interface BasicInfoPanelProps {
  /** Workflow key —— 新建时可编辑，已有 workflow 应锁定。 */
  draftKey: string;
  /** 显示名称。 */
  name: string;
  /** 描述文本。 */
  description: string;
  /** 目标挂载类型；至少保留一个。 */
  targetKinds: WorkflowTargetKind[];
  /**
   * key 输入是否禁用。
   * 容器视 `originalId === null`（新建态）决定是否可编辑。
   */
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
    <DetailSection title="基本信息">
      <div className="grid gap-3 sm:grid-cols-2">
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
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
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
          <div className="mt-1.5 flex flex-wrap gap-2">
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
      </div>
    </DetailSection>
  );
}
