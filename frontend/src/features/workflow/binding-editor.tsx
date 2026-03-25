import type {
  BindingKindMetadata,
  WorkflowContextBinding,
  WorkflowContextBindingKind,
  WorkflowTargetKind,
} from "../../types";
import { BINDING_KIND_LABEL } from "./shared-labels";

interface BindingEditorProps {
  binding: WorkflowContextBinding;
  index: number;
  targetKind: WorkflowTargetKind;
  bindingMetadata: BindingKindMetadata[];
  onChange: (patch: Partial<WorkflowContextBinding>) => void;
  onRemove: () => void;
}

export function BindingEditor({
  binding,
  index,
  targetKind,
  bindingMetadata,
  onChange,
  onRemove,
}: BindingEditorProps) {
  const kindMeta = bindingMetadata.find((m) => m.kind === binding.kind);
  const locatorOptions = kindMeta?.locator_options.filter(
    (opt) => opt.applicable_target_kinds.length === 0 || opt.applicable_target_kinds.includes(targetKind),
  ) ?? [];

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
          <label className="text-[11px] text-muted-foreground">类型</label>
          <select
            value={binding.kind}
            onChange={(e) => onChange({ kind: e.target.value as WorkflowContextBindingKind, locator: "" })}
            className="agentdash-form-select mt-1 text-sm"
          >
            {bindingMetadata.length > 0
              ? bindingMetadata.map((m) => (
                  <option key={m.kind} value={m.kind}>{m.label}</option>
                ))
              : Object.entries(BINDING_KIND_LABEL).map(([k, v]) => (
                  <option key={k} value={k}>{v}</option>
                ))
            }
          </select>
          {kindMeta?.description && (
            <p className="mt-1 text-[10px] text-muted-foreground/70">{kindMeta.description}</p>
          )}
        </div>

        <div>
          <label className="text-[11px] text-muted-foreground">Locator</label>
          {locatorOptions.length > 0 ? (
            <select
              value={binding.locator}
              onChange={(e) => onChange({ locator: e.target.value })}
              className="agentdash-form-select mt-1 text-sm"
            >
              <option value="">选择 locator…</option>
              {locatorOptions.map((opt) => (
                <option key={opt.locator} value={opt.locator}>{opt.label}</option>
              ))}
            </select>
          ) : (
            <input
              value={binding.locator}
              onChange={(e) => onChange({ locator: e.target.value })}
              className="agentdash-form-input mt-1 text-sm"
              placeholder="e.g. project_journal"
            />
          )}
        </div>
      </div>

      <div className="grid gap-2 sm:grid-cols-2">
        <div>
          <label className="text-[11px] text-muted-foreground">标题（可选）</label>
          <input
            value={binding.title ?? ""}
            onChange={(e) => onChange({ title: e.target.value || null })}
            className="agentdash-form-input mt-1 text-sm"
            placeholder="可选显示名"
          />
        </div>
        <div>
          <label className="text-[11px] text-muted-foreground">用途说明</label>
          <input
            value={binding.reason}
            onChange={(e) => onChange({ reason: e.target.value })}
            className="agentdash-form-input mt-1 text-sm"
            placeholder="为什么需要此 binding"
          />
        </div>
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
