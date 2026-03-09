import { useEffect, useRef } from "react";
import type { AddressEntry } from "../../../services/addressSpaces";

function getEntryIcon(entry: AddressEntry): string {
  if (entry.entry_type === "directory") return "📁";
  const ext = entry.address.split(".").pop()?.toLowerCase() ?? "";
  const icons: Record<string, string> = {
    rs: "🦀", ts: "📘", tsx: "📘", js: "📒", jsx: "📒",
    json: "📋", toml: "⚙️", yaml: "⚙️", yml: "⚙️",
    md: "📝", py: "🐍", html: "🌐", css: "🎨", scss: "🎨",
    sql: "🗃️", sh: "🖥️", bash: "🖥️",
  };
  return icons[ext] ?? "📄";
}

export interface AddressEntryPickerProps {
  open: boolean;
  query: string;
  entries: AddressEntry[];
  loading: boolean;
  error: string | null;
  selectedIndex: number;
  placeholder?: string;
  emptyText?: string;
  onQueryChange: (q: string) => void;
  onSelect: (entry: AddressEntry) => void;
  onClose: () => void;
  onMoveSelection: (delta: number) => void;
  onConfirmSelection: () => void;
}

/**
 * 内联模式的寻址条目选择器。
 * 渲染为正常文档流元素（非浮层），适合嵌入 overflow 容器中。
 */
export function AddressEntryPickerInline({
  open,
  query,
  entries,
  loading,
  error,
  selectedIndex,
  placeholder = "搜索文件…",
  emptyText,
  onQueryChange,
  onSelect,
  onClose,
  onMoveSelection,
  onConfirmSelection,
}: AddressEntryPickerProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) {
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  useEffect(() => {
    if (!open || !listRef.current) return;
    const active = listRef.current.querySelector("[data-selected='true']");
    if (active) {
      active.scrollIntoView({ block: "nearest" });
    }
  }, [open, selectedIndex]);

  if (!open) return null;

  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        onMoveSelection(1);
        break;
      case "ArrowUp":
        e.preventDefault();
        onMoveSelection(-1);
        break;
      case "Enter":
        e.preventDefault();
        onConfirmSelection();
        break;
      case "Escape":
        e.preventDefault();
        onClose();
        break;
    }
  };

  return (
    <div className="rounded-[8px] border border-border bg-background/80">
      <div className="flex items-center gap-2 px-2.5 py-1.5">
        <span className="text-xs text-muted-foreground">@</span>
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          className="flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground/60"
        />
        {loading && (
          <span className="h-3 w-3 animate-spin rounded-full border border-muted-foreground/40 border-t-primary" />
        )}
        <button
          type="button"
          onClick={onClose}
          className="rounded-[4px] p-0.5 text-muted-foreground/50 transition-colors hover:text-foreground"
        >
          <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div ref={listRef} className="max-h-40 overflow-y-auto border-t border-border py-0.5">
        {error && (
          <div className="px-2.5 py-1.5 text-xs text-destructive">{error}</div>
        )}
        {!error && entries.length === 0 && !loading && (
          <div className="px-2.5 py-3 text-center text-xs text-muted-foreground">
            {emptyText ?? (query ? "没有匹配的条目" : "暂无条目")}
          </div>
        )}
        {entries.map((entry, i) => (
          <button
            key={entry.address}
            type="button"
            data-selected={i === selectedIndex}
            aria-selected={i === selectedIndex}
            onClick={() => onSelect(entry)}
            className={`flex w-full items-center gap-1.5 px-2.5 py-1 text-left text-xs transition-colors ${
              i === selectedIndex
                ? "bg-primary/10 text-foreground"
                : "text-foreground hover:bg-accent/50"
            }`}
          >
            <span className="shrink-0 text-xs">{getEntryIcon(entry)}</span>
            <span className="min-w-0 flex-1 truncate font-mono">{entry.label}</span>
          </button>
        ))}
      </div>

      <div className="border-t border-border px-2.5 py-1 text-[10px] text-muted-foreground/50">
        ↑↓ 选择 · Enter 确认 · Esc 关闭
      </div>
    </div>
  );
}

/**
 * 浮层模式的寻址条目选择器（保留作为 absolute popup 用途）。
 */
export function AddressEntryPickerPopup(props: AddressEntryPickerProps) {
  const { open, onClose } = props;

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (!target.closest("[data-entry-picker]")) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      data-entry-picker
      className="absolute bottom-full left-0 z-50 mb-1 w-full max-w-lg rounded-lg border border-border bg-popover shadow-lg"
    >
      <AddressEntryPickerInline {...props} />
    </div>
  );
}
