/**
 * "+" 新建 Tab 下拉菜单
 *
 * 列出所有可创建的（非钉选）Tab 类型，点击后触发新建。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { useTabTypeRegistrySnapshot } from "./tab-type-registry";

interface AddTabMenuProps {
  onAddTab: (typeId: string) => void;
}

export function AddTabMenu({ onAddTab }: AddTabMenuProps) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  const creatableTypes = useTabTypeRegistrySnapshot().filter((type) => !type.pinned);

  const handleSelect = useCallback(
    (typeId: string) => {
      onAddTab(typeId);
      setOpen(false);
    },
    [onAddTab],
  );

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  return (
    <div ref={menuRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="flex h-7 w-7 shrink-0 items-center justify-center rounded-[6px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        title="新建 Tab"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M5 12h14" />
          <path d="M12 5v14" />
        </svg>
      </button>

      {open && creatableTypes.length > 0 && (
        <div className="absolute right-0 top-full z-50 mt-1 min-w-[160px] rounded-[8px] border border-border bg-background py-1 shadow-lg">
          <p className="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
            打开面板
          </p>
          {creatableTypes.map((type) => {
            const Icon = type.icon;
            return (
              <button
                key={type.typeId}
                type="button"
                onClick={() => handleSelect(type.typeId)}
                className="flex w-full items-center gap-2.5 px-3 py-2 text-left text-xs text-foreground transition-colors hover:bg-secondary"
              >
                <Icon className="h-3.5 w-3.5 text-muted-foreground" />
                <span>{type.label}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
