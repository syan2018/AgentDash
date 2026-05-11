/**
 * Injection Panel —— guidance + context_bindings。
 *
 * 视觉语言对齐 Overview：线性 section heading + 控件；不包 DetailSection，
 * 不加平铺 description 注释。
 */

import type {
  WorkflowContextBinding,
  WorkflowInjectionSpec,
} from "../../../../types";
import { BindingEditor } from "../../binding-editor";

export interface InjectionPanelProps {
  injection: WorkflowInjectionSpec;
  onGuidanceChange: (guidance: string | null) => void;
  onBindingChange: (index: number, patch: Partial<WorkflowContextBinding>) => void;
  onBindingAdd: () => void;
  onBindingRemove: (index: number) => void;
}

export function InjectionPanel({
  injection,
  onGuidanceChange,
  onBindingChange,
  onBindingAdd,
  onBindingRemove,
}: InjectionPanelProps) {
  const bindings = injection.context_bindings;

  return (
    <section className="space-y-4">
      <div>
        <label className="agentdash-form-label">Session 指引</label>
        <textarea
          value={injection.guidance ?? ""}
          onChange={(e) => onGuidanceChange(e.target.value || null)}
          rows={6}
          className="agentdash-form-textarea"
          placeholder="描述 Agent 该做什么、遵守什么边界、如何结束"
        />
      </div>

      <div>
        <div className="mb-1.5 flex items-center justify-between gap-2">
          <label className="agentdash-form-label m-0">
            上下文挂载 ({bindings.length})
          </label>
          <button
            type="button"
            onClick={onBindingAdd}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-foreground transition-colors hover:bg-secondary"
          >
            + 添加
          </button>
        </div>
        <div className="space-y-2">
          {bindings.map((binding, idx) => (
            <BindingEditor
              key={`${binding.locator}:${idx}`}
              binding={binding}
              index={idx}
              onChange={(patch) => onBindingChange(idx, patch)}
              onRemove={() => onBindingRemove(idx)}
            />
          ))}
          {bindings.length === 0 && (
            <p className="py-2 text-center text-xs text-muted-foreground">暂无</p>
          )}
        </div>
      </div>
    </section>
  );
}
