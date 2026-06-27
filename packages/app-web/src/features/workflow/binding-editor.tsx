import type { WorkflowContextBinding } from "../../types";

interface BindingEditorProps {
  binding: WorkflowContextBinding;
  index: number;
  onChange: (patch: Partial<WorkflowContextBinding>) => void;
  onRemove: () => void;
}

export function BindingEditor({
  binding,
  index,
  onChange,
  onRemove,
}: BindingEditorProps) {
  return (
    <div className="space-y-2.5 rounded-[8px] border border-border bg-background p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
          #{index + 1}
        </span>
        <button
          type="button"
          onClick={onRemove}
          className="rounded-md px-2 py-0.5 text-[11px] text-destructive transition-colors hover:bg-destructive/10"
        >
          移除
        </button>
      </div>

      <div>
        <label className="agentdash-form-label">Locator</label>
        <input
          value={binding.locator}
          onChange={(e) => onChange({ locator: e.target.value })}
          className="agentdash-form-input font-mono text-xs"
          placeholder="main/docs/workflow.md"
        />
      </div>

      <div>
        <label className="agentdash-form-label">标题</label>
        <input
          value={binding.title ?? ""}
          onChange={(e) => onChange({ title: e.target.value || undefined })}
          className="agentdash-form-input text-xs"
          placeholder="可选显示名"
        />
      </div>

      <div>
        <label className="agentdash-form-label">用途</label>
        <input
          value={binding.reason}
          onChange={(e) => onChange({ reason: e.target.value })}
          className="agentdash-form-input text-xs"
          placeholder="为什么需要此 binding"
        />
      </div>

      <label className="flex cursor-pointer items-center gap-2 text-[11px] text-foreground">
        <input
          type="checkbox"
          checked={binding.required}
          onChange={(e) => onChange({ required: e.target.checked })}
        />
        必须（required）
      </label>
    </div>
  );
}
