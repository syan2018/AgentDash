import type { WorkflowTargetKind } from "../../../types";
import {
  TARGET_KIND_LABEL,
  TARGET_KIND_OPTIONS,
} from "../shared-labels";

function toggleTargetKind(current: WorkflowTargetKind[], value: WorkflowTargetKind): WorkflowTargetKind[] {
  if (current.includes(value)) {
    const next = current.filter((kind) => kind !== value);
    return next.length > 0 ? next : current;
  }
  return [...current, value];
}

export interface DagLifecyclePanelProps {
  /** lifecycle 顶层 key */
  lifecycleKey: string;
  name: string;
  description: string;
  targetKinds: WorkflowTargetKind[];
  entryStepKey: string;
  /** 当前所有 step key，用于 entry_step_key datalist */
  stepKeys: string[];
  /** 是否为新建模式（key 创建后不可改） */
  isNew: boolean;
  onChange: (patch: {
    key?: string;
    name?: string;
    description?: string;
    target_kinds?: WorkflowTargetKind[];
    entry_step_key?: string;
  }) => void;
}

/**
 * DAG 编辑器中的 Lifecycle 元信息配置面板。
 * 当未选中任何节点时显示在右侧，用于编辑 lifecycle 顶层字段。
 */
export function DagLifecyclePanel({
  lifecycleKey,
  name,
  description,
  targetKinds,
  entryStepKey,
  stepKeys,
  isNew,
  onChange,
}: DagLifecyclePanelProps) {
  return (
    <div className="flex h-full flex-col border-l border-border bg-background">
      {/* Header */}
      <div className="border-b border-border px-4 py-3">
        <p className="text-sm font-semibold text-foreground">Lifecycle 配置</p>
        <p className="mt-0.5 text-[10px] text-muted-foreground">
          点击节点可编辑节点配置
        </p>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="space-y-4">
          <div>
            <label className="agentdash-form-label">Key</label>
            <input
              value={lifecycleKey}
              onChange={(e) => onChange({ key: e.target.value })}
              disabled={!isNew}
              className="agentdash-form-input disabled:opacity-60"
              placeholder="task_lifecycle_v2"
            />
            <p className="mt-1 text-[10px] text-muted-foreground">
              全局唯一标识{!isNew && "（创建后不可修改）"}
            </p>
          </div>

          <div>
            <label className="agentdash-form-label">名称</label>
            <input
              value={name}
              onChange={(e) => onChange({ name: e.target.value })}
              className="agentdash-form-input"
              placeholder="Task Lifecycle V2"
            />
          </div>

          <div>
            <label className="agentdash-form-label">描述</label>
            <textarea
              value={description}
              onChange={(e) => onChange({ description: e.target.value })}
              rows={3}
              className="agentdash-form-textarea"
              placeholder="描述该 lifecycle 如何编排 agent 的阶段性工作"
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
                      onChange={() => onChange({ target_kinds: toggleTargetKind(targetKinds, kind) })}
                      className="sr-only"
                    />
                    {TARGET_KIND_LABEL[kind]}
                  </label>
                );
              })}
            </div>
          </div>

          <div>
            <label className="agentdash-form-label">入口节点</label>
            <input
              value={entryStepKey}
              onChange={(e) => onChange({ entry_step_key: e.target.value })}
              list="lifecycle-entry-step-opts"
              className="agentdash-form-input"
              placeholder="start"
            />
            <datalist id="lifecycle-entry-step-opts">
              {stepKeys.filter(Boolean).map((k) => (
                <option key={k} value={k} />
              ))}
            </datalist>
            <p className="mt-1 text-[10px] text-muted-foreground">
              DAG 执行的起始节点
            </p>
          </div>

        </div>
      </div>
    </div>
  );
}
