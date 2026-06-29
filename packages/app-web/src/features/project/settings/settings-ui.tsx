import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import type { SettingsTab, SettingsTabItem } from "./settings-tabs";

export function SectionCard({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="space-y-5">
      <div className="space-y-1.5">
        <h2 className="text-xl font-semibold tracking-[-0.025em] text-foreground">{title}</h2>
        {description && <p className="max-w-3xl text-sm leading-6 text-muted-foreground">{description}</p>}
      </div>
      {children}
    </section>
  );
}

export function ContentGroup({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="space-y-4 border-t border-border/70 pt-6 first:border-t-0 first:pt-0">
      <div className="space-y-1">
        <h3 className="text-sm font-semibold uppercase tracking-[0.14em] text-foreground">{title}</h3>
        {description && <p className="text-sm leading-6 text-muted-foreground">{description}</p>}
      </div>
      {children}
    </section>
  );
}

export function CollapsibleGroup({
  title,
  hint,
  children,
}: {
  title: string;
  hint?: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <section className="space-y-3 border-t border-border/70 pt-6 first:border-t-0 first:pt-0">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        className="flex w-full items-center gap-2 text-left"
      >
        <span className="text-[10px] text-muted-foreground">{open ? "▾" : "▸"}</span>
        <span className="text-sm font-semibold uppercase tracking-[0.14em] text-foreground">{title}</span>
      </button>
      {hint && <p className="text-xs leading-6 text-muted-foreground">{hint}</p>}
      {open && <div className="space-y-4">{children}</div>}
    </section>
  );
}

export function SettingsTabs({
  tabs,
  activeTab,
  onChange,
}: {
  tabs: SettingsTabItem[];
  activeTab: SettingsTab;
  onChange: (tab: SettingsTab) => void;
}) {
  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    event.preventDefault();
    const currentIndex = tabs.findIndex((tab) => tab.key === activeTab);
    const delta = event.key === "ArrowRight" ? 1 : -1;
    const nextIndex = (currentIndex + delta + tabs.length) % tabs.length;
    onChange(tabs[nextIndex].key);
  };

  return (
    <div
      role="tablist"
      aria-label="Project Settings 分区"
      onKeyDown={handleKeyDown}
      className="grid gap-2 sm:grid-cols-2 xl:grid-cols-4"
    >
      {tabs.map((tab) => {
        const isActive = activeTab === tab.key;
        return (
          <button
            key={tab.key}
            type="button"
            role="tab"
            aria-selected={isActive}
            tabIndex={isActive ? 0 : -1}
            onClick={() => onChange(tab.key)}
            className={`flex h-full min-w-0 flex-col justify-between rounded-[14px] border px-4 py-3 text-left transition-colors ${
              isActive
                ? "border-foreground/10 bg-background text-foreground shadow-sm"
                : "border-transparent bg-transparent text-muted-foreground hover:border-border/80 hover:bg-background/70 hover:text-foreground"
            }`}
          >
            <p className="truncate text-sm font-medium">{tab.label}</p>
            <p className="mt-1 text-xs leading-5 opacity-80">{tab.description}</p>
          </button>
        );
      })}
    </div>
  );
}

export function Toast({ message, onDone }: { message: string; onDone: () => void }) {
  useEffect(() => {
    const timer = setTimeout(onDone, 2400);
    return () => clearTimeout(timer);
  }, [message, onDone]);

  return (
    <div className="fixed bottom-6 right-6 z-50 animate-fade-in rounded-[8px] border border-success/30 bg-background px-4 py-2.5 text-sm text-foreground shadow-lg">
      {message}
    </div>
  );
}
