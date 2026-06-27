/**
 * Tab 地址栏
 *
 * 展示当前激活 Tab 的 URI scheme，点击可复制。
 */

import { useCallback, useState } from "react";
import type { TabInstance, TabTypeDescriptor } from "./tab-type-registry";

interface AddressBarProps {
  tab: TabInstance | null;
  tabTypes: TabTypeDescriptor[];
}

export function AddressBar({ tab, tabTypes }: AddressBarProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    if (!tab) return;
    try {
      await navigator.clipboard.writeText(tab.uri);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* noop */ }
  }, [tab]);

  if (!tab) return null;

  const type = tabTypes.find((descriptor) => descriptor.typeId === tab.typeId);
  const Icon = type?.icon;

  return (
    <div className="flex shrink-0 items-center gap-1.5 border-b border-border/50 bg-secondary/10 px-3 py-1">
      {Icon && <Icon className="h-3 w-3 shrink-0 text-muted-foreground/60" />}
      <button
        type="button"
        onClick={() => void handleCopy()}
        className="min-w-0 flex-1 truncate text-left font-mono text-[11px] text-muted-foreground/80 transition-colors hover:text-foreground"
        title="点击复制 URI"
      >
        {tab.uri}
      </button>
      {copied && (
        <span className="shrink-0 text-[10px] text-success">已复制</span>
      )}
    </div>
  );
}
