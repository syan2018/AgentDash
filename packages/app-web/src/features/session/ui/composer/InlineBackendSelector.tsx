import { useEffect, useMemo, useRef, useState } from "react";

export interface InlineBackendOption {
  accessId: string;
  backendId: string;
  label: string;
  statusLabel?: string;
  online?: boolean;
}

export interface InlineBackendSelectorProps {
  value: string;
  options: InlineBackendOption[];
  loading?: boolean;
  statusText?: string | null;
  readonly?: boolean;
  onChange: (backendId: string) => void;
  onRefresh?: () => void;
  onInspectBackend?: (backendId: string) => void;
}

export function InlineBackendSelector({
  value,
  options,
  loading = false,
  statusText,
  readonly: isReadonly = false,
  onChange,
  onRefresh,
  onInspectBackend,
}: InlineBackendSelectorProps) {
  const [open, setOpen] = useState(false);
  const popoverRef = useRef<HTMLDivElement>(null);
  const chipRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    function handleClick(event: MouseEvent) {
      if (
        popoverRef.current
        && !popoverRef.current.contains(event.target as Node)
        && chipRef.current
        && !chipRef.current.contains(event.target as Node)
      ) {
        setOpen(false);
      }
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  const selectedOption = useMemo(
    () => options.find((option) => option.backendId === value) ?? null,
    [options, value],
  );
  const chipLabel = selectedOption?.label ?? "默认";
  const chipStatus = selectedOption?.statusLabel ?? statusText ?? null;

  const handleSelect = (backendId: string) => {
    onChange(backendId);
    setOpen(false);
  };

  return (
    <div className="relative">
      <button
        ref={chipRef}
        type="button"
        onClick={() => {
          if (!isReadonly) setOpen((current) => !current);
        }}
        disabled={isReadonly}
        className={`flex items-center gap-1 rounded-[8px] px-2.5 py-1.5 text-xs transition-colors ${
          isReadonly
            ? "cursor-default text-muted-foreground opacity-60"
            : open
              ? "bg-secondary text-foreground"
              : "text-muted-foreground hover:bg-secondary hover:text-foreground"
        }`}
        title={chipStatus ?? undefined}
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        {loading ? (
          <span className="inline-block h-3 w-3 animate-spin rounded-[8px] border border-muted-foreground border-t-transparent" />
        ) : (
          <BackendDot online={selectedOption?.online} selected={Boolean(value)} />
        )}
        <span className="max-w-[180px] truncate">{chipLabel}</span>
        {!isReadonly && (
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`}>
            <path d="M4 6L8 10L12 6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        )}
      </button>

      {open && (
        <div
          ref={popoverRef}
          className="absolute bottom-full right-0 z-50 mb-2 flex max-w-[calc(100vw-2rem)] overflow-hidden rounded-[12px] border border-border bg-popover shadow-lg"
        >
          <div className="w-[160px] max-w-[42vw] shrink-0 border-r border-border p-2">
            <div className="px-2 pb-1 pt-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              Backend
            </div>
            <button
              type="button"
              onClick={() => handleSelect("")}
              className={`flex w-full items-center justify-between rounded-[6px] px-2 py-1.5 text-xs transition-colors ${
                value
                  ? "text-foreground hover:bg-secondary"
                  : "bg-primary/10 text-primary"
              }`}
              role="option"
              aria-selected={!value}
            >
              <span>默认</span>
              {!value && <CheckIcon />}
            </button>

            {chipStatus && (
              <div className="mt-2 rounded-[8px] bg-secondary/40 px-2 py-1.5 text-[11px] leading-relaxed text-muted-foreground">
                {chipStatus}
              </div>
            )}

            {(onRefresh || (onInspectBackend && value)) && (
              <div className="mt-2 flex items-center gap-1 border-t border-border pt-2">
                {onRefresh && (
                  <button
                    type="button"
                    onClick={onRefresh}
                    className="rounded-[6px] px-2 py-1 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground"
                  >
                    刷新
                  </button>
                )}
                {onInspectBackend && value && (
                  <button
                    type="button"
                    onClick={() => onInspectBackend(value)}
                    className="rounded-[6px] px-2 py-1 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground"
                  >
                    状态
                  </button>
                )}
              </div>
            )}
          </div>

          <div className="w-[220px] max-w-[52vw] p-2">
            <div className="px-2 pb-1 pt-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              授权 Backend
            </div>
            <div className="max-h-[300px] space-y-0.5 overflow-y-auto">
              {options.map((option) => {
                const isSelected = option.backendId === value;
                return (
                  <button
                    key={option.accessId}
                    type="button"
                    onClick={() => handleSelect(option.backendId)}
                    className={`flex w-full items-center justify-between gap-2 rounded-[6px] px-2 py-1.5 text-xs transition-colors ${
                      isSelected
                        ? "bg-primary/10 text-primary"
                        : "text-foreground hover:bg-secondary"
                    }`}
                    role="option"
                    aria-selected={isSelected}
                  >
                    <span className="flex min-w-0 items-center gap-2">
                      <BackendDot online={option.online} selected={isSelected} />
                      <span className="min-w-0 truncate">{option.label}</span>
                    </span>
                    {isSelected && <CheckIcon />}
                  </button>
                );
              })}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function BackendDot({ online, selected }: { online?: boolean; selected?: boolean }) {
  const tone = online === true
    ? "bg-success"
    : online === false
      ? "bg-muted-foreground/40"
      : selected
        ? "bg-primary"
        : "bg-muted-foreground/40";
  return <span className={`inline-block h-1.5 w-1.5 shrink-0 rounded-[8px] ${tone}`} />;
}

function CheckIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" className="shrink-0 text-primary">
      <path d="M3.5 8.5L6.5 11.5L12.5 4.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
