import { useEffect, useMemo, useState } from "react";
import type { CanvasDataBinding } from "../../types";

export interface CanvasBindingsEditorProps {
  value: CanvasDataBinding[];
  isSaving?: boolean;
  error?: string | null;
  onSave: (bindings: CanvasDataBinding[]) => Promise<void> | void;
  onCancel?: (bindings: CanvasDataBinding[]) => void;
}

interface BindingValidationResult {
  hasError: boolean;
  rowErrors: string[];
}

const DEFAULT_CONTENT_TYPE = "application/json";

function createEmptyBinding(): CanvasDataBinding {
  return {
    alias: "",
    source_uri: "",
    content_type: DEFAULT_CONTENT_TYPE,
  };
}

function normalizeBinding(binding: CanvasDataBinding): CanvasDataBinding {
  return {
    alias: binding.alias.trim(),
    source_uri: binding.source_uri.trim(),
    content_type: binding.content_type.trim() || DEFAULT_CONTENT_TYPE,
  };
}

function validateBindings(bindings: CanvasDataBinding[]): BindingValidationResult {
  const seen = new Set<string>();
  const rowErrors = bindings.map((binding) => {
    const alias = binding.alias.trim();
    const sourceUri = binding.source_uri.trim();

    if (!alias) {
      return "alias 不能为空";
    }
    if (alias.includes("/") || alias.includes("\\")) {
      return "alias 不能包含路径分隔符";
    }
    const aliasKey = alias.toLowerCase();
    if (seen.has(aliasKey)) {
      return "alias 不能重复";
    }
    seen.add(aliasKey);

    if (!sourceUri) {
      return "source_uri 不能为空";
    }

    return "";
  });

  return {
    hasError: rowErrors.some((item) => item.length > 0),
    rowErrors,
  };
}

export function CanvasBindingsEditor({
  value,
  isSaving = false,
  error = null,
  onSave,
  onCancel,
}: CanvasBindingsEditorProps) {
  const [draftBindings, setDraftBindings] = useState<CanvasDataBinding[]>(value);
  const [isDirty, setIsDirty] = useState(false);

  useEffect(() => {
    setDraftBindings(value);
    setIsDirty(false);
  }, [value]);

  const validation = useMemo(() => validateBindings(draftBindings), [draftBindings]);

  const canSave = !isSaving && isDirty && !validation.hasError;

  const handleBindingChange = (
    index: number,
    field: keyof CanvasDataBinding,
    nextValue: string,
  ) => {
    setDraftBindings((prev) =>
      prev.map((item, itemIndex) => {
        if (itemIndex !== index) {
          return item;
        }
        return {
          ...item,
          [field]: nextValue,
        };
      }),
    );
    setIsDirty(true);
  };

  const handleAddBinding = () => {
    setDraftBindings((prev) => [...prev, createEmptyBinding()]);
    setIsDirty(true);
  };

  const handleRemoveBinding = (index: number) => {
    setDraftBindings((prev) => prev.filter((_, itemIndex) => itemIndex !== index));
    setIsDirty(true);
  };

  const handleCancel = () => {
    setDraftBindings(value);
    setIsDirty(false);
    onCancel?.(value);
  };

  const handleSave = async () => {
    if (!canSave) {
      return;
    }
    const normalized = draftBindings.map(normalizeBinding);
    try {
      await onSave(normalized);
      setIsDirty(false);
    } catch {
      // 保存失败时保留 dirty 状态，方便用户修正后重试。
    }
  };

  return (
    <section className="space-y-3 rounded-[10px] border border-border bg-secondary/20 p-3">
      <div className="flex items-center justify-between">
        <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">
          数据绑定编辑
        </p>
        <button
          type="button"
          onClick={handleAddBinding}
          disabled={isSaving}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
        >
          新增绑定
        </button>
      </div>

      {draftBindings.length === 0 && (
        <div className="rounded-[8px] border border-dashed border-border bg-background px-3 py-2 text-xs text-muted-foreground">
          当前没有绑定，点击“新增绑定”开始配置。
        </div>
      )}

      {draftBindings.map((binding, index) => (
        <div
          key={`${index}-${binding.alias}-${binding.source_uri}`}
          className="space-y-2 rounded-[8px] border border-border bg-background p-2"
        >
          <div className="grid grid-cols-1 gap-2">
            <label className="space-y-1">
              <span className="text-[11px] text-muted-foreground">alias</span>
              <input
                value={binding.alias}
                onChange={(event) => handleBindingChange(index, "alias", event.target.value)}
                disabled={isSaving}
                className="w-full rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-foreground outline-none transition-colors focus:border-foreground/40"
                placeholder="例如：stats"
              />
            </label>
            <label className="space-y-1">
              <span className="text-[11px] text-muted-foreground">source_uri</span>
              <input
                value={binding.source_uri}
                onChange={(event) => handleBindingChange(index, "source_uri", event.target.value)}
                disabled={isSaving}
                className="w-full rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-foreground outline-none transition-colors focus:border-foreground/40"
                placeholder="例如：lifecycle://active/artifacts/1"
              />
            </label>
            <label className="space-y-1">
              <span className="text-[11px] text-muted-foreground">content_type</span>
              <input
                value={binding.content_type}
                onChange={(event) => handleBindingChange(index, "content_type", event.target.value)}
                disabled={isSaving}
                className="w-full rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-foreground outline-none transition-colors focus:border-foreground/40"
                placeholder={DEFAULT_CONTENT_TYPE}
              />
            </label>
          </div>
          {validation.rowErrors[index] && (
            <p className="text-xs text-destructive">{validation.rowErrors[index]}</p>
          )}
          <div className="flex justify-end">
            <button
              type="button"
              onClick={() => handleRemoveBinding(index)}
              disabled={isSaving}
              className="rounded-[8px] border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs text-destructive transition-colors hover:bg-destructive/20 disabled:cursor-not-allowed disabled:opacity-50"
            >
              删除
            </button>
          </div>
        </div>
      ))}

      {error && (
        <div className="rounded-[8px] border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs text-destructive">
          {error}
        </div>
      )}

      <div className="flex items-center justify-end gap-2 pt-1">
        <button
          type="button"
          onClick={handleCancel}
          disabled={isSaving || !isDirty}
          className="rounded-[8px] border border-border bg-background px-3 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
        >
          取消
        </button>
        <button
          type="button"
          onClick={() => void handleSave()}
          disabled={!canSave}
          className="rounded-[8px] border border-border bg-foreground px-3 py-1 text-xs text-background transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isSaving ? "保存中..." : "保存绑定"}
        </button>
      </div>
    </section>
  );
}

export default CanvasBindingsEditor;
