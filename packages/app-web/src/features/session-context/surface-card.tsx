import type { ReactNode } from "react";

/**
 * 通用的 "Surface Card" 布局原语，用于 Session 上下文展示面板中的信息卡片。
 */
export function SurfaceCard({
  eyebrow,
  title,
  children,
}: {
  eyebrow: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="rounded-[14px] border border-border bg-background/75 px-4 py-3">
      <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground/70">
        {eyebrow}
      </p>
      <h3 className="mt-1 text-sm font-semibold text-foreground">{title}</h3>
      <div className="mt-2">{children}</div>
    </section>
  );
}
