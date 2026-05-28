import type { ReactNode } from "react";

export const inputCls =
  "w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring";

export const btnPrimaryCls =
  "rounded-[8px] border border-border bg-primary text-primary-foreground px-4 py-2 text-sm font-medium transition-colors hover:opacity-90 disabled:opacity-50";

export function SectionCard({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-[12px] border border-border bg-secondary/35 p-5">
      <h2 className="text-base font-semibold text-foreground">{title}</h2>
      <div className="mt-4 space-y-4">{children}</div>
    </section>
  );
}

export function Field({
  label,
  desc,
  children,
}: {
  label: string;
  desc?: string;
  children: ReactNode;
}) {
  return (
    <label className="block space-y-1.5">
      <span className="text-sm font-medium text-foreground">{label}</span>
      {desc && <p className="text-xs text-muted-foreground">{desc}</p>}
      {children}
    </label>
  );
}
