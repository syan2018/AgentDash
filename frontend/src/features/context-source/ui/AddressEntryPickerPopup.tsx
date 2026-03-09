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

interface AddressEntryPickerPopupProps {
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

export function AddressEntryPickerPopup({
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
}: AddressEntryPickerPopupProps) {
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
    <div
      data-entry-picker
      className="absolute bottom-full left-0 z-50 mb-1 w-full max-w-lg rounded-lg border border-border bg-popover shadow-lg"
    >
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <span className="text-xs text-muted-foreground">@</span>
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          className="flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground/60"
        />
        {loading && (
          <span className="h-3.5 w-3.5 animate-spin rounded-full border border-muted-foreground/40 border-t-primary" />
        )}
      </div>

      <div ref={listRef} className="max-h-60 overflow-y-auto py-1">
        {error && (
          <div className="px-3 py-2 text-xs text-destructive">{error}</div>
        )}
        {!error && entries.length === 0 && !loading && (
          <div className="px-3 py-4 text-center text-xs text-muted-foreground">
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
            className={`relative mx-1 flex w-[calc(100%-0.5rem)] items-center gap-2 rounded-[10px] border px-3 py-2 text-left text-xs transition-colors ${
              i === selectedIndex
                ? "border-primary/50 bg-primary/10 text-foreground shadow-sm ring-1 ring-primary/25"
                : "border-transparent text-foreground hover:border-border hover:bg-accent/50"
            }`}
          >
            {i === selectedIndex && (
              <span className="absolute left-0 top-1/2 h-6 w-1 -translate-y-1/2 rounded-r-full bg-primary" />
            )}
            <span className="shrink-0 text-sm">{getEntryIcon(entry)}</span>
            <span className="min-w-0 flex-1 truncate font-mono">{entry.label}</span>
            <span className={`shrink-0 text-[10px] ${i === selectedIndex ? "text-foreground/60" : "text-muted-foreground/60"}`}>
              {entry.entry_type}
            </span>
          </button>
        ))}
      </div>

      <div className="border-t border-border px-3 py-1.5 text-xs text-muted-foreground/60">
        ↑↓ 选择 · Enter 确认 · Esc 关闭
      </div>
    </div>
  );
}
