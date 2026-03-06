import { useEffect, useRef, useState, type ReactNode } from "react";

export interface DetailPanelProps {
  open: boolean;
  title: string;
  subtitle?: string;
  onClose: () => void;
  children: ReactNode;
  headerExtra?: ReactNode;
  widthClassName?: string;
  overlayClassName?: string;
  panelClassName?: string;
}

export function DetailPanel({
  open,
  title,
  subtitle,
  onClose,
  children,
  headerExtra,
  widthClassName = "max-w-3xl",
  overlayClassName = "z-40",
  panelClassName = "z-50",
}: DetailPanelProps) {
  if (!open) return null;

  return (
    <>
      <div
        className={`fixed inset-0 ${overlayClassName} bg-foreground/18 backdrop-blur-[2px]`}
        onClick={onClose}
      />
      <aside
        className={`fixed inset-y-0 right-0 ${panelClassName} flex w-full ${widthClassName} flex-col border-l border-border bg-background shadow-2xl`}
      >
        <header className="flex items-start justify-between gap-3 border-b border-border px-5 py-4">
          <div className="min-w-0">
            <span className="agentdash-panel-header-tag">Panel</span>
            <h3 className="truncate text-base font-semibold text-foreground">{title}</h3>
            {subtitle && (
              <p className="mt-1.5 text-xs leading-5 text-muted-foreground">{subtitle}</p>
            )}
          </div>
          <div className="flex items-center gap-2">
            {headerExtra}
            <button
              type="button"
              onClick={onClose}
              className="inline-flex h-8 w-8 items-center justify-center rounded-[10px] border border-border bg-background text-base leading-none text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
              aria-label="关闭"
              title="关闭"
            >
              ×
            </button>
          </div>
        </header>
        <div className="flex-1 overflow-y-auto">{children}</div>
      </aside>
    </>
  );
}

export interface DetailSectionProps {
  title: string;
  description?: string;
  children: ReactNode;
  extra?: ReactNode;
}

export function DetailSection({ title, description, children, extra }: DetailSectionProps) {
  return (
    <section className="space-y-3 rounded-[12px] border border-border bg-secondary/35 p-4">
      <div className="flex items-start justify-between">
        <div>
          <h4 className="text-sm font-medium text-foreground">{title}</h4>
          {description && <p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p>}
        </div>
        {extra}
      </div>
      {children}
    </section>
  );
}

export interface DetailMenuItem {
  key: string;
  label: string;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
}

export function DetailMenu({ items }: { items: DetailMenuItem[] }) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;

    const handlePointerDown = (event: PointerEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };

    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  return (
    <div ref={containerRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="inline-flex h-8 w-8 items-center justify-center rounded-[10px] border border-border bg-background text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        title="详情菜单"
      >
        ⋯
      </button>
      {open && (
        <div className="absolute right-0 top-10 z-[80] min-w-[10rem] rounded-[12px] border border-border bg-background p-1.5 shadow-xl">
          {items.map((item) => (
            <button
              key={item.key}
              type="button"
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              disabled={item.disabled}
              className={`w-full rounded-[8px] px-2.5 py-2 text-left text-sm transition-colors ${
                item.danger
                  ? "text-destructive hover:bg-destructive/10"
                  : "text-foreground hover:bg-secondary"
              } disabled:opacity-50`}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export interface DangerConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  expectedValue?: string;
  inputValue?: string;
  onInputValueChange?: (value: string) => void;
  confirmLabel: string;
  onClose: () => void;
  onConfirm: () => void;
  isConfirming?: boolean;
}

export function DangerConfirmDialog({
  open,
  title,
  description,
  expectedValue,
  inputValue = "",
  onInputValueChange,
  confirmLabel,
  onClose,
  onConfirm,
  isConfirming = false,
}: DangerConfirmDialogProps) {
  if (!open) return null;

  const needInputMatch = Boolean(expectedValue);
  const canConfirm = needInputMatch ? inputValue.trim() === expectedValue : true;

  return (
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/24 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-lg rounded-[16px] border border-border bg-background shadow-2xl">
          <div className="border-b border-border px-5 py-4">
            <span className="agentdash-panel-header-tag">Danger</span>
            <h4 className="text-base font-semibold text-foreground">{title}</h4>
            <p className="mt-1.5 text-sm leading-6 text-muted-foreground">{description}</p>
          </div>
          <div className="space-y-3 p-5">
            {needInputMatch && (
              <>
                <p className="text-xs text-muted-foreground">
                  请输入 <span className="font-mono text-foreground">{expectedValue}</span>{" "}
                  进行确认
                </p>
                <input
                  value={inputValue}
                  onChange={(event) => onInputValueChange?.(event.target.value)}
                  placeholder={`请输入 ${expectedValue}`}
                  className="agentdash-form-input border-destructive/30 focus:border-destructive/30 focus:ring-destructive/40"
                />
              </>
            )}
          </div>
          <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
            <button
              type="button"
              onClick={onClose}
              className="agentdash-button-secondary"
            >
              取消
            </button>
            <button
              type="button"
              onClick={onConfirm}
              disabled={!canConfirm || isConfirming}
              className="agentdash-button-danger"
            >
              {isConfirming ? "处理中..." : confirmLabel}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
