import { useState, type ReactNode } from "react";

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
        className={`fixed inset-0 ${overlayClassName} bg-foreground/20 backdrop-blur-[1px]`}
        onClick={onClose}
      />
      <aside
        className={`fixed inset-y-0 right-0 ${panelClassName} flex w-full ${widthClassName} flex-col border-l border-border bg-background shadow-xl`}
      >
        <header className="flex items-start justify-between gap-3 border-b border-border px-5 py-4">
          <div className="min-w-0">
            <h3 className="truncate text-base font-semibold text-foreground">{title}</h3>
            {subtitle && (
              <p className="mt-1 text-xs text-muted-foreground">{subtitle}</p>
            )}
          </div>
          <div className="flex items-center gap-2">
            {headerExtra}
            <button
              type="button"
              onClick={onClose}
              className="h-7 w-7 rounded-md text-base leading-none text-muted-foreground hover:bg-secondary"
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
}

export function DetailSection({ title, description, children }: DetailSectionProps) {
  return (
    <section className="space-y-3 rounded-md border border-border bg-card p-4">
      <div>
        <h4 className="text-sm font-medium text-foreground">{title}</h4>
        {description && <p className="mt-1 text-xs text-muted-foreground">{description}</p>}
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

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-secondary"
        title="详情菜单"
      >
        ⋯
      </button>
      {open && (
        <div className="absolute right-0 top-9 z-[80] min-w-[10rem] rounded-md border border-border bg-background p-1 shadow-lg">
          {items.map((item) => (
            <button
              key={item.key}
              type="button"
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              disabled={item.disabled}
              className={`w-full rounded px-2 py-1.5 text-left text-sm ${
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
      <div className="fixed inset-0 z-[90] bg-foreground/30" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-lg rounded-lg border border-border bg-background shadow-xl">
          <div className="border-b border-border px-4 py-3">
            <h4 className="text-base font-semibold text-foreground">{title}</h4>
            <p className="mt-1 text-sm text-muted-foreground">{description}</p>
          </div>
          <div className="space-y-3 p-4">
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
                  className="w-full rounded border border-destructive/30 bg-background px-2 py-1.5 text-sm outline-none ring-destructive/40 focus:ring-1"
                />
              </>
            )}
          </div>
          <div className="flex items-center justify-end gap-2 border-t border-border px-4 py-3">
            <button
              type="button"
              onClick={onClose}
              className="rounded border border-border bg-secondary px-3 py-1.5 text-sm text-foreground hover:bg-secondary/70"
            >
              取消
            </button>
            <button
              type="button"
              onClick={onConfirm}
              disabled={!canConfirm || isConfirming}
              className="rounded bg-destructive px-3 py-1.5 text-sm text-destructive-foreground disabled:opacity-50"
            >
              {isConfirming ? "处理中..." : confirmLabel}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
