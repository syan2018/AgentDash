import { useEffect, useRef, useState } from "react";

export interface CardMenuItem {
  key: string;
  label: string;
  danger?: boolean;
  badge?: string;
  onSelect: () => void;
}

export function CardMenu({ items }: { items: CardMenuItem[] }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div ref={ref} className="relative" onClick={(e) => e.stopPropagation()}>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
        className="inline-flex h-7 w-7 items-center justify-center rounded-[8px] text-base text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        title="操作菜单"
      >
        &#x22EF;
      </button>
      {open && (
        <div className="absolute right-0 top-9 z-[80] min-w-[9rem] rounded-[10px] border border-border bg-background p-1 shadow-xl">
          {items.map((item) =>
            item.key === "---" ? (
              <div key={item.key} className="my-1 border-t border-border/60" />
            ) : (
              <button
                key={item.key}
                type="button"
                onClick={() => {
                  setOpen(false);
                  item.onSelect();
                }}
                className={`flex w-full items-center gap-2 rounded-[6px] px-2.5 py-1.5 text-left text-xs transition-colors ${
                  item.danger
                    ? "text-destructive hover:bg-destructive/10"
                    : "text-foreground hover:bg-secondary"
                }`}
              >
                {item.label}
                {item.badge && (
                  <span className="ml-auto rounded-full bg-amber-500/15 px-1.5 py-0.5 text-[9px] text-amber-600 dark:text-amber-400">
                    {item.badge}
                  </span>
                )}
              </button>
            ),
          )}
        </div>
      )}
    </div>
  );
}
