/**
 * Injection Panel —— guidance + context_bindings。
 *
 * 受控组件：展示 Session 启动时注入给 Agent 的目标文本，
 * 以及挂载到 Session 的外部上下文资源。
 */

import type {
  WorkflowContextBinding,
  WorkflowInjectionSpec,
} from "../../../../types";
import { BindingEditor } from "../../binding-editor";
import { DetailSection } from "../../../../components/ui/detail-panel";

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
    <>
      {/* Session 指引 */}
      <DetailSection
        title="Session 指引"
        description="Workflow 激活时注入给 Agent 的目标、行为边界和完成要求。"
      >
        <textarea
          value={injection.guidance ?? ""}
          onChange={(e) => onGuidanceChange(e.target.value || null)}
          rows={7}
          className="agentdash-form-textarea"
          placeholder={
            "描述这个 Workflow 下 Agent 应完成什么、遵守什么边界、如何结束。\n\n例如：\n当前处于 Review 阶段，检查实现质量与风险。\n- 先阅读相关 diff 与测试结果\n- 输出明确问题和建议\n- 完成后调用 complete_lifecycle_node"
          }
        />
      </DetailSection>

      {/* Context Bindings */}
      <DetailSection
        title={`上下文挂载 (${bindings.length})`}
        description="Session 启动时自动挂载的外部上下文资源。"
        extra={
          <button
            type="button"
            onClick={onBindingAdd}
            className="agentdash-button-secondary text-sm"
          >
            + 添加
          </button>
        }
      >
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
            <p className="py-4 text-center text-sm text-muted-foreground">暂无上下文挂载</p>
          )}
        </div>
      </DetailSection>
    </>
  );
}
