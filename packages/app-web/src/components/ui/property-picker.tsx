import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";

export interface PropertyPickerOption<T extends string> {
  value: T;
  label: string;
  preview?: ReactNode;
  description?: string;
}

interface PropertyPickerProps<T extends string> {
  trigger: ReactNode;
  triggerLabel?: string;
  value: T;
  options: PropertyPickerOption<T>[];
  onChange: (next: T) => void;
  align?: "start" | "end";
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  searchable?: boolean;
  disabled?: boolean;
  triggerClassName?: string;
}

export function PropertyPicker<T extends string>({
  trigger,
  triggerLabel,
  value,
  options,
  onChange,
  align = "start",
  open: controlledOpen,
  onOpenChange,
  searchable = false,
  disabled = false,
  triggerClassName = "",
}: PropertyPickerProps<T>) {
  const [internalOpen, setInternalOpen] = useState(false);
  const open = controlledOpen ?? internalOpen;
  const setOpen = useCallback(
    (next: boolean) => {
      if (controlledOpen === undefined) setInternalOpen(next);
      onOpenChange?.(next);
    },
    [controlledOpen, onOpenChange],
  );
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const composingRef = useRef(false);
  const [search, setSearch] = useState("");
  const [highlightIdx, setHighlightIdx] = useState(0);
  const [openSnapshot, setOpenSnapshot] = useState(open);
  const id = useId();

  if (open !== openSnapshot) {
    setOpenSnapshot(open);
    if (open) {
      setSearch("");
      const idx = options.findIndex((opt) => opt.value === value);
      setHighlightIdx(idx >= 0 ? idx : 0);
    }
  }

  const trimmed = search.trim().toLowerCase();
  const filtered = options.filter(
    (opt) => !trimmed || opt.label.toLowerCase().includes(trimmed),
  );

  const effectiveHighlight =
    filtered.length === 0 ? 0 : Math.min(highlightIdx, filtered.length - 1);

  useEffect(() => {
    if (!open) return;
    const handleClick = (event: MouseEvent) => {
      if (!containerRef.current) return;
      if (!containerRef.current.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", handleClick);
    return () => window.removeEventListener("mousedown", handleClick);
  }, [open, setOpen]);

  useEffect(() => {
    if (open && searchable) {
      const t = window.setTimeout(() => inputRef.current?.focus(), 0);
      return () => window.clearTimeout(t);
    }
  }, [open, searchable]);

  const handleKeyDown = (event: ReactKeyboardEvent) => {
    if (composingRef.current) return;
    if (event.key === "Escape") {
      event.preventDefault();
      setOpen(false);
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      const len = filtered.length;
      if (len === 0) return;
      setHighlightIdx((effectiveHighlight + 1) % len);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      const len = filtered.length;
      if (len === 0) return;
      setHighlightIdx((effectiveHighlight - 1 + len) % len);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const opt = filtered[effectiveHighlight];
      if (opt) {
        onChange(opt.value);
        setOpen(false);
      }
    }
  };

  const triggerToggle = (event: ReactMouseEvent | ReactKeyboardEvent) => {
    if (disabled) return;
    event.preventDefault();
    event.stopPropagation();
    setOpen(!open);
  };

  return (
    <div ref={containerRef} className="relative inline-flex">
      <span
        role="button"
        tabIndex={disabled ? -1 : 0}
        aria-label={triggerLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={id}
        onClick={triggerToggle}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") triggerToggle(event);
        }}
        className={`inline-flex cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:ring-offset-1 focus-visible:ring-offset-background ${
          disabled ? "pointer-events-none opacity-60" : ""
        } ${triggerClassName}`}
      >
        {trigger}
      </span>
      {open && (
        <div
          role="listbox"
          id={id}
          onKeyDown={handleKeyDown}
          tabIndex={-1}
          className={`absolute top-full z-50 mt-1 min-w-[10rem] max-w-[18rem] rounded-[8px] border border-border bg-card p-1 shadow-lg shadow-foreground/10 outline-none ${
            align === "end" ? "right-0" : "left-0"
          }`}
        >
          {searchable && (
            <input
              ref={inputRef}
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              onCompositionStart={() => {
                composingRef.current = true;
              }}
              onCompositionEnd={() => {
                composingRef.current = false;
              }}
              placeholder="搜索"
              className="mb-1 h-7 w-full rounded-[6px] border border-border bg-background px-2 text-xs text-foreground outline-none focus:border-primary/30"
            />
          )}
          <ul className="max-h-60 overflow-y-auto">
            {filtered.map((opt, idx) => (
              <li
                key={opt.value}
                role="option"
                aria-selected={opt.value === value}
                data-highlighted={idx === effectiveHighlight ? "true" : undefined}
                onMouseEnter={() => setHighlightIdx(idx)}
                onClick={(event) => {
                  event.stopPropagation();
                  onChange(opt.value);
                  setOpen(false);
                }}
                className={`flex cursor-pointer items-center justify-between gap-2 rounded-[6px] px-2 py-1.5 text-xs transition-colors ${
                  idx === effectiveHighlight ? "bg-secondary/60" : ""
                } ${opt.value === value ? "text-foreground" : "text-muted-foreground"}`}
              >
                <span className="flex min-w-0 items-center gap-2">
                  {opt.preview}
                  <span className="truncate">{opt.label}</span>
                </span>
                {opt.value === value && (
                  <svg
                    className="h-3 w-3 shrink-0 text-primary"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                  >
                    <path d="M20 6 9 17l-5-5" />
                  </svg>
                )}
              </li>
            ))}
            {filtered.length === 0 && (
              <li className="px-2 py-2 text-center text-xs text-muted-foreground">无匹配</li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
