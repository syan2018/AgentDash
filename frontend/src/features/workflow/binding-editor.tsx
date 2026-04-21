import type {
  WorkflowContextBinding,
} from "../../types";

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
    <div className="rounded-[10px] border border-border bg-secondary/10 p-3 space-y-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] font-medium text-muted-foreground">Binding #{index + 1}</span>
        <button
          type="button"
          onClick={onRemove}
          className="rounded-md px-2 py-0.5 text-[11px] text-destructive transition-colors hover:bg-destructive/10"
        >
          移除
        </button>
      </div>

      <div className="grid gap-2 sm:grid-cols-2">
        <div>
          <label className="text-[11px] text-muted-foreground">Locator</label>
          <input
            value={binding.locator}
            onChange={(e) => onChange({ locator: e.target.value })}
            className="agentdash-form-input mt-1 text-sm"
            placeholder="e.g. main/docs/workflow.md"
          />
        </div>

        <div>
          <label className="text-[11px] text-muted-foreground">标题（可选）</label>
          <input
            value={binding.title ?? ""}
            onChange={(e) => onChange({ title: e.target.value || null })}
            className="agentdash-form-input mt-1 text-sm"
            placeholder="可选显示名"
          />
        </div>
      </div>

      <div>
        <label className="text-[11px] text-muted-foreground">用途说明</label>
        <input
          value={binding.reason}
          onChange={(e) => onChange({ reason: e.target.value })}
          className="agentdash-form-input mt-1 text-sm w-full"
          placeholder="为什么需要此 binding"
        />
      </div>

      <label className="flex items-center gap-2 text-[11px] text-foreground">
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
