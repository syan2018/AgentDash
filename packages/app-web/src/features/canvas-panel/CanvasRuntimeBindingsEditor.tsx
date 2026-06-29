import { useMemo, useState } from "react";
import type { CanvasRuntimeBinding } from "../../types";

export interface CanvasRuntimeBindingDraft {
  alias: string;
  source_uri: string;
  content_type?: string;
}

export interface CanvasRuntimeBindingsEditorProps {
  value: CanvasRuntimeBinding[];
  isSaving?: boolean;
  error?: string | null;
  readOnly?: boolean;
  onUpsert: (binding: CanvasRuntimeBindingDraft) => Promise<void> | void;
}

function draftFromBinding(binding: CanvasRuntimeBinding): CanvasRuntimeBindingDraft {
  return {
    alias: binding.alias,
    source_uri: binding.source_uri,
    content_type: binding.content_type,
  };
}

function emptyDraft(): CanvasRuntimeBindingDraft {
  return {
    alias: "",
    source_uri: "",
    content_type: "application/json",
  };
}

function normalizeDraft(draft: CanvasRuntimeBindingDraft): CanvasRuntimeBindingDraft {
  return {
    alias: draft.alias.trim(),
    source_uri: draft.source_uri.trim(),
    content_type: draft.content_type?.trim() || undefined,
  };
}

export function CanvasRuntimeBindingsEditor({
  value,
  isSaving = false,
  error = null,
  readOnly = false,
  onUpsert,
}: CanvasRuntimeBindingsEditorProps) {
  const [draft, setDraft] = useState<CanvasRuntimeBindingDraft>(() => emptyDraft());
  const [localError, setLocalError] = useState<string | null>(null);

  const bindingsByAlias = useMemo(() => {
    return new Map(value.map((binding) => [binding.alias, binding]));
  }, [value]);

  const selectedBinding = bindingsByAlias.get(draft.alias);

  const handleSelectAlias = (alias: string) => {
    const binding = bindingsByAlias.get(alias);
    setDraft(binding ? draftFromBinding(binding) : emptyDraft());
    setLocalError(null);
  };

  const handleDraftChange = (patch: Partial<CanvasRuntimeBindingDraft>) => {
    setDraft((current) => ({ ...current, ...patch }));
    setLocalError(null);
  };

  const handleSubmit = async () => {
    const next = normalizeDraft(draft);
    if (!next.alias) {
      setLocalError("alias 不能为空");
      return;
    }
    if (!next.source_uri) {
      setLocalError("source_uri 不能为空");
      return;
    }
    setLocalError(null);
    await onUpsert(next);
    setDraft(emptyDraft());
  };

  return (
    <section className="space-y-3 rounded-[8px] border border-border bg-secondary/20 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">运行期绑定编辑</p>
        {value.length > 0 && (
          <select
            value={selectedBinding ? draft.alias : ""}
            onChange={(event) => handleSelectAlias(event.target.value)}
            className="rounded-[6px] border border-border bg-background px-2 py-1 text-xs text-foreground outline-none focus:border-primary"
          >
            <option value="">新增绑定</option>
            {value.map((binding) => (
              <option key={binding.alias} value={binding.alias}>
                {binding.alias}
              </option>
            ))}
          </select>
        )}
      </div>

      {(error || localError) && (
        <p className="rounded-[6px] border border-destructive/30 bg-destructive/10 px-2 py-1 text-xs text-destructive">
          {error || localError}
        </p>
      )}

      {readOnly && (
        <p className="rounded-[6px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground">
          当前视图没有 AgentRun runtime bridge，绑定只能随快照读取。
        </p>
      )}

      <div className="grid gap-2 md:grid-cols-[minmax(120px,0.8fr)_minmax(220px,1.7fr)_minmax(160px,1fr)]">
        <label className="space-y-1 text-xs">
          <span className="text-muted-foreground">alias</span>
          <input
            value={draft.alias}
            onChange={(event) => handleDraftChange({ alias: event.target.value })}
            disabled={readOnly || isSaving}
            className="w-full rounded-[6px] border border-border bg-background px-2 py-1 text-foreground outline-none focus:border-primary disabled:cursor-not-allowed disabled:opacity-60"
            placeholder="stats"
          />
        </label>
        <label className="space-y-1 text-xs">
          <span className="text-muted-foreground">source_uri</span>
          <input
            value={draft.source_uri}
            onChange={(event) => handleDraftChange({ source_uri: event.target.value })}
            disabled={readOnly || isSaving}
            className="w-full rounded-[6px] border border-border bg-background px-2 py-1 text-foreground outline-none focus:border-primary disabled:cursor-not-allowed disabled:opacity-60"
            placeholder="main://data/stats.json"
          />
        </label>
        <label className="space-y-1 text-xs">
          <span className="text-muted-foreground">content_type</span>
          <input
            value={draft.content_type ?? ""}
            onChange={(event) => handleDraftChange({ content_type: event.target.value })}
            disabled={readOnly || isSaving}
            className="w-full rounded-[6px] border border-border bg-background px-2 py-1 text-foreground outline-none focus:border-primary disabled:cursor-not-allowed disabled:opacity-60"
            placeholder="application/json"
          />
        </label>
      </div>

      <div className="flex justify-end">
        <button
          type="button"
          onClick={() => void handleSubmit()}
          disabled={readOnly || isSaving}
          className="rounded-[6px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
        >
          {isSaving ? "保存中..." : selectedBinding ? "更新绑定" : "添加绑定"}
        </button>
      </div>
    </section>
  );
}

export default CanvasRuntimeBindingsEditor;
