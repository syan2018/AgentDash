/**
 * 统一的资产来源徽章。
 *
 * 各 panel 自行把 (source, installed) 映射成 variant，避免本组件耦合具体 panel 的 source 枚举。
 */

export type AssetSourceVariant = "marketplace" | "builtin" | "cloned" | "user";

const VARIANT_STYLE: Record<AssetSourceVariant, { label: string; className: string }> = {
  marketplace: {
    label: "marketplace",
    className:
      "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
  },
  builtin: {
    label: "builtin",
    className:
      "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300",
  },
  cloned: {
    label: "cloned",
    className:
      "border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300",
  },
  user: {
    label: "user",
    className: "border-border bg-secondary/70 text-muted-foreground",
  },
};

export function SourceBadge({ variant }: { variant: AssetSourceVariant }) {
  const style = VARIANT_STYLE[variant];
  return (
    <span
      className={`shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-medium ${style.className}`}
    >
      {style.label}
    </span>
  );
}
