/**
 * "+" 新建 Tab 下拉菜单
 *
 * 列出所有可创建的（非钉选）Tab 类型，点击后触发新建。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { TabTypeDescriptor } from "./tab-type-registry";
import type { CanvasModuleOpenOption } from "./model/canvasModuleOpen";
import type { ProjectWorkspaceModulesStatus } from "../workspace-module/model/types";

interface AddTabMenuProps {
  tabTypes: TabTypeDescriptor[];
  onAddTab: (typeId: string) => void;
  canvasOptions: CanvasModuleOpenOption[];
  canvasOptionsStatus: ProjectWorkspaceModulesStatus;
  canvasOpenBusyKey: string | null;
  canvasOpenError: string | null;
  onOpenCanvasModule: (option: CanvasModuleOpenOption) => Promise<boolean>;
}

export function AddTabMenu({
  tabTypes,
  onAddTab,
  canvasOptions,
  canvasOptionsStatus,
  canvasOpenBusyKey,
  canvasOpenError,
  onOpenCanvasModule,
}: AddTabMenuProps) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  const creatableTypes = tabTypes.filter((type) => !type.pinned);

  const handleSelect = useCallback(
    (typeId: string) => {
      onAddTab(typeId);
      setOpen(false);
    },
    [onAddTab],
  );

  const handleCanvasSelect = useCallback(
    (option: CanvasModuleOpenOption) => {
      void onOpenCanvasModule(option).then((opened) => {
        if (opened) setOpen(false);
      });
    },
    [onOpenCanvasModule],
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
            if (type.typeId === "canvas") {
              return (
                <CanvasSelectorGroup
                  key={type.typeId}
                  icon={Icon}
                  label={type.label}
                  options={canvasOptions}
                  status={canvasOptionsStatus}
                  busyKey={canvasOpenBusyKey}
                  error={canvasOpenError}
                  onSelect={handleCanvasSelect}
                />
              );
            }
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

function CanvasSelectorGroup({
  icon: Icon,
  label,
  options,
  status,
  busyKey,
  error,
  onSelect,
}: {
  icon: TabTypeDescriptor["icon"];
  label: string;
  options: CanvasModuleOpenOption[];
  status: ProjectWorkspaceModulesStatus;
  busyKey: string | null;
  error: string | null;
  onSelect: (option: CanvasModuleOpenOption) => void;
}) {
  const loading = status === "loading" || status === "refreshing";
  const disabledMessage = loading
    ? "加载中"
    : status === "error"
      ? "加载失败"
      : "无可打开 Canvas";

  return (
    <div className="border-t border-border/60 first:border-t-0">
      <div className="flex w-full items-center gap-2.5 px-3 py-2 text-left text-xs text-foreground">
        <Icon className="h-3.5 w-3.5 text-muted-foreground" />
        <span>{label}</span>
      </div>
      <div className="pb-1">
        {options.length > 0 ? (
          options.map((option) => {
            const key = `${option.module_id}:${option.view_key}`;
            const busy = busyKey === key;
            return (
              <button
                key={key}
                type="button"
                disabled={busy}
                onClick={() => onSelect(option)}
                className="flex w-full min-w-0 items-center justify-between gap-2 px-6 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-secondary disabled:cursor-wait disabled:text-muted-foreground"
                title={option.presentation_uri}
              >
                <span className="min-w-0 truncate">{option.title}</span>
                {busy && (
                  <span className="shrink-0 text-[10px] text-muted-foreground">打开中</span>
                )}
              </button>
            );
          })
        ) : (
          <div className="px-6 py-1.5 text-xs text-muted-foreground">{disabledMessage}</div>
        )}
        {error && (
          <div className="mx-3 mt-1 rounded-[6px] border border-destructive/30 bg-destructive/10 px-2 py-1 text-[11px] text-destructive">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}
