/**
 * TransitionInspector —— 选中 lifecycle transition 时右侧栏的编辑面板。
 *
 * 受控组件：接收 transition + lifecycle.activities 上下文 + 一组细粒度 callback。
 * 内部不直接调 store，shell 在装配时把 store actions 包装传入。
 *
 * 结构：
 *   - sticky header: from → to + 关闭按钮
 *   - kind switch（flow / artifact）—— artifact→flow 时弹 confirm 防误删 bindings
 *   - max_traversals（数字 + 无限复选）
 *   - ConditionEditor
 *   - ArtifactBindingsEditor（仅 kind=artifact 显示）
 */

import type {
  ActivityDefinition,
  ActivityTransition,
  ArtifactBinding,
  TransitionCondition,
} from "../../../types";
import { ArtifactBindingsEditor } from "./ArtifactBindingsEditor";
import { ConditionEditor } from "./conditions/ConditionEditor";

export interface TransitionInspectorProps {
  transition: ActivityTransition;
  /** 全部 lifecycle activity，供 condition / binding 选择 */
  activities: ActivityDefinition[];
  onClose: () => void;

  // 编辑回调（细粒度对应 workflowStore actions）
  onSetKind: (kind: ActivityTransition["kind"]) => void;
  onConditionChange: (next: TransitionCondition) => void;
  onMaxTraversalsChange: (value: number | null) => void;
  onAddBinding: (binding: ArtifactBinding) => void;
  onUpdateBinding: (idx: number, patch: Partial<ArtifactBinding>) => void;
  onRemoveBinding: (idx: number) => void;
}

export function TransitionInspector(props: TransitionInspectorProps) {
  const {
    transition,
    activities,
    onClose,
    onSetKind,
    onConditionChange,
    onMaxTraversalsChange,
    onAddBinding,
    onUpdateBinding,
    onRemoveBinding,
  } = props;

  const toActivity = activities.find((a) => a.key === transition.to) ?? null;
  const maxAttempts = transition.max_traversals;
  const isInfinite = maxAttempts === null || maxAttempts === undefined;

  const handleKindSwitch = (next: ActivityTransition["kind"]) => {
    if (next === transition.kind) return;
    if (
      transition.kind === "artifact" &&
      next === "flow" &&
      transition.artifact_bindings.length > 0 &&
      typeof window !== "undefined" &&
      typeof window.confirm === "function"
    ) {
      const ok = window.confirm("切换到 flow 将清空当前所有 artifact_bindings，是否继续？");
      if (!ok) return;
    }
    onSetKind(next);
  };

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="sticky top-0 z-10 flex shrink-0 items-center justify-between border-b border-border bg-background px-4 py-3">
        <div className="overflow-hidden">
          <p className="truncate font-mono text-sm text-foreground">
            {transition.from} → {transition.to}
          </p>
          <p className="text-[10px] text-muted-foreground">Transition</p>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="rounded-[8px] p-1 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          title="关闭面板"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
          </svg>
        </button>
      </header>

      <div className="flex-1 overflow-y-auto">
        <div className="space-y-4 p-4">
          <section className="space-y-2">
            <label className="agentdash-form-label">Kind</label>
            <select
              value={transition.kind}
              onChange={(e) => handleKindSwitch(e.target.value as ActivityTransition["kind"])}
              className="agentdash-form-select"
            >
              <option value="flow">flow</option>
              <option value="artifact">artifact</option>
            </select>
          </section>

          <section className="space-y-2">
            <label className="agentdash-form-label">Condition</label>
            <ConditionEditor
              condition={transition.condition}
              activities={activities}
              onChange={onConditionChange}
            />
          </section>

          <section className="space-y-2">
            <label className="agentdash-form-label">Max Traversals</label>
            <div className="flex items-center gap-2">
              <input
                type="number"
                min={1}
                value={isInfinite ? "" : maxAttempts}
                disabled={isInfinite}
                onChange={(e) => {
                  const n = Number.parseInt(e.target.value, 10);
                  onMaxTraversalsChange(Number.isFinite(n) && n > 0 ? n : 1);
                }}
                className="agentdash-form-input flex-1 disabled:opacity-50"
                placeholder="1"
              />
              <label className="flex items-center gap-1 text-xs text-muted-foreground">
                <input
                  type="checkbox"
                  checked={isInfinite}
                  onChange={(e) => onMaxTraversalsChange(e.target.checked ? null : 1)}
                />
                无限
              </label>
            </div>
          </section>

          {transition.kind === "artifact" && (
            <section>
              <ArtifactBindingsEditor
                bindings={transition.artifact_bindings}
                activities={activities}
                toActivity={toActivity}
                defaultFromActivity={transition.from}
                onAdd={onAddBinding}
                onUpdate={onUpdateBinding}
                onRemove={onRemoveBinding}
              />
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
