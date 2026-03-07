import { useEffect, useRef } from "react";
import type { FileEntry } from "../../services/workspaceFiles";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function getFileIcon(relPath: string): string {
  const ext = relPath.split(".").pop()?.toLowerCase() ?? "";
  const icons: Record<string, string> = {
    rs: "🦀", ts: "📘", tsx: "📘", js: "📒", jsx: "📒",
    json: "📋", toml: "⚙️", yaml: "⚙️", yml: "⚙️",
    md: "📝", py: "🐍", html: "🌐", css: "🎨", scss: "🎨",
    sql: "🗃️", sh: "🖥️", bash: "🖥️",
  };
  return icons[ext] ?? "📄";
}

interface FilePickerPopupProps {
  open: boolean;
  query: string;
  files: FileEntry[];
  loading: boolean;
  error: string | null;
  selectedIndex: number;
  placeholder?: string;
  emptyText?: string;
  onQueryChange: (q: string) => void;
  onSelect: (file: FileEntry) => void;
  onClose: () => void;
  onMoveSelection: (delta: number) => void;
  onConfirmSelection: () => void;
}

export function FilePickerPopup({
  open,
  query,
  files,
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
}: FilePickerPopupProps) {
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
      if (!target.closest("[data-file-picker]")) {
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
      data-file-picker
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
        {!error && files.length === 0 && !loading && (
          <div className="px-3 py-4 text-center text-xs text-muted-foreground">
            {emptyText ?? (query ? "没有匹配的文件" : "暂无文件")}
          </div>
        )}
        {files.map((file, i) => (
          <button
            key={file.relPath}
            type="button"
            data-selected={i === selectedIndex}
            onClick={() => onSelect(file)}
            className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors ${
              i === selectedIndex
                ? "bg-accent text-accent-foreground"
                : "text-foreground hover:bg-accent/50"
            }`}
          >
            <span className="shrink-0 text-sm">{getFileIcon(file.relPath)}</span>
            <span className="min-w-0 flex-1 truncate font-mono">{file.relPath}</span>
            <span className="shrink-0 text-muted-foreground">{formatSize(file.size)}</span>
          </button>
        ))}
      </div>

      <div className="border-t border-border px-3 py-1.5 text-xs text-muted-foreground/60">
        ↑↓ 选择 · Enter 确认 · Esc 关闭
      </div>
    </div>
  );
}
