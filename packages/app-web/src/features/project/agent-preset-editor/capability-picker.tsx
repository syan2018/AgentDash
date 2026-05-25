import type { ReactNode } from "react";

export type CapabilityChipVariant = 'default' | 'warning';

export interface CapabilityChip {
  label: string;
  variant?: CapabilityChipVariant;
}

export function CapabilityCard({
  selected,
  title,
  subtitle,
  description,
  chips,
  footer,
  onClick,
}: {
  selected: boolean;
  title: string;
  subtitle?: string;
  description?: string;
  chips?: CapabilityChip[];
  /** 卡片底部插槽：用于嵌入二级控件（例如 VFS 的 cap 按钮组）。 */
  footer?: ReactNode;
  onClick: () => void;
}) {
  // 用 div + role=button 而非 <button>：footer 里通常会嵌入交互按钮，原生 button 嵌套不合法。
  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onClick();
    }
  };
  return (
    <div
      className={`group relative flex flex-col gap-2 rounded-[12px] border px-3 py-2.5 text-left transition-all duration-160 ${
        selected
          ? "border-primary/40 bg-primary/8 shadow-sm"
          : "border-border bg-secondary/15 hover:border-primary/25 hover:bg-secondary/30"
      }`}
    >
      {/* 主体可点击区——切换整卡的选中态。footer 区域单独渲染，自带交互不会冒泡到这里。 */}
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        onClick={onClick}
        onKeyDown={handleKeyDown}
        className="flex flex-col gap-2 cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-primary/40 rounded-[8px]"
      >
        <div className="flex items-start gap-2">
          <span
            className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-[6px] border transition-colors ${
              selected
                ? "border-primary bg-primary text-background"
                : "border-border bg-background text-muted-foreground/50 group-hover:border-primary/40 group-hover:text-primary"
            }`}
            aria-hidden
          >
            {selected ? (
              <svg viewBox="0 0 12 12" className="h-2.5 w-2.5" fill="none" stroke="currentColor" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round">
                <path d="M2.5 6.5l2.5 2.5 4.5-5" />
              </svg>
            ) : (
              <svg viewBox="0 0 12 12" className="h-2 w-2" fill="none" stroke="currentColor" strokeWidth={2.5} strokeLinecap="round">
                <path d="M6 2v8M2 6h8" />
              </svg>
            )}
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-xs font-medium text-foreground truncate">{title}</div>
            {subtitle && (
              <div className="mt-0.5 font-mono text-[10px] text-muted-foreground/80 truncate">{subtitle}</div>
            )}
          </div>
        </div>
        {description && (
          <p className="line-clamp-2 text-[11px] leading-snug text-muted-foreground/85">{description}</p>
        )}
        {chips && chips.length > 0 && (
          <div className="flex flex-wrap items-center gap-1">
            {chips.map((chip, i) => (
              <span
                key={`${chip.label}-${i}`}
                className={`rounded-[6px] px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-wide ${
                  chip.variant === 'warning'
                    ? "border border-warning/30 bg-warning/10 text-warning"
                    : "bg-secondary/60 text-muted-foreground"
                }`}
              >
                {chip.label}
              </span>
            ))}
          </div>
        )}
      </div>
      {footer && <div className="border-t border-border/40 pt-2">{footer}</div>}
    </div>
  );
}

function CapabilityZone({
  title,
  count,
  hint,
  emptyText,
  children,
  isEmpty,
  trailing,
}: {
  title: string;
  count: number;
  hint?: string;
  emptyText: string;
  children: ReactNode;
  isEmpty: boolean;
  /** 末尾追加的占位卡（如"+ 创建"） */
  trailing?: ReactNode;
}) {
  return (
    <section className="space-y-2">
      <div className="flex items-baseline justify-between gap-3">
        <h5 className="text-[10px] font-semibold uppercase tracking-[0.1em] text-foreground/70">
          {title}
          <span className="ml-2 font-normal normal-case tracking-normal text-muted-foreground">{count}</span>
        </h5>
        {hint && <span className="text-[10px] text-muted-foreground/70">{hint}</span>}
      </div>
      {isEmpty && !trailing ? (
        <p className="rounded-[12px] border border-dashed border-border px-3 py-3 text-center text-[11px] text-muted-foreground">
          {emptyText}
        </p>
      ) : (
        <div className="grid grid-cols-1 gap-2 md:grid-cols-2 xl:grid-cols-3">
          {children}
          {trailing}
        </div>
      )}
    </section>
  );
}

export interface CapabilityPickerProps<T> {
  /** 顶部说明文字 */
  hint?: ReactNode;
  isLoading: boolean;
  error: string | null;
  items: T[];
  selectedKeys: string[];
  /** 从 item 提取 selectedKeys 中使用的字符串 */
  itemKey: (item: T) => string;
  /** 把 item 映射为 CapabilityCard 的展示 props（排除 selected/onClick） */
  itemToCardProps: (item: T) => {
    title: string;
    subtitle?: string;
    description?: string;
    chips?: CapabilityChip[];
    footer?: ReactNode;
  } & { reactKey: string };
  onToggle: (key: string) => void;
  loadingText: string;
  emptyAllText: string;
  enabledEmptyText: string;
  availableEmptyText: string;
  /** 可添加区末尾追加（如"+ 创建"虚线占位卡） */
  trailingAvailable?: ReactNode;
}

export function CapabilityPicker<T>({
  hint,
  isLoading,
  error,
  items,
  selectedKeys,
  itemKey,
  itemToCardProps,
  onToggle,
  loadingText,
  emptyAllText,
  enabledEmptyText,
  availableEmptyText,
  trailingAvailable,
}: CapabilityPickerProps<T>) {
  const enabled = items.filter((it) => selectedKeys.includes(itemKey(it)));
  const available = items.filter((it) => !selectedKeys.includes(itemKey(it)));

  const renderCard = (item: T, selected: boolean) => {
    const { reactKey, ...rest } = itemToCardProps(item);
    return (
      <CapabilityCard
        key={reactKey}
        selected={selected}
        {...rest}
        onClick={() => onToggle(itemKey(item))}
      />
    );
  };

  return (
    <div className="space-y-4">
      {hint && <div className="text-[11px] text-muted-foreground/80">{hint}</div>}
      {error && (
        <p className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-2.5 py-2 text-[11px] text-destructive">
          {error}
        </p>
      )}
      {isLoading ? (
        <p className="rounded-[12px] border border-dashed border-border px-3 py-4 text-center text-[11px] text-muted-foreground">
          {loadingText}
        </p>
      ) : items.length === 0 && !trailingAvailable ? (
        <div className="rounded-[12px] border border-dashed border-border px-4 py-6 text-center text-[11px] text-muted-foreground">
          {emptyAllText}
        </div>
      ) : (
        <>
          <CapabilityZone
            title="已启用"
            count={enabled.length}
            isEmpty={enabled.length === 0}
            emptyText={enabledEmptyText}
            hint={enabled.length > 0 ? "点击卡片可移除" : undefined}
          >
            {enabled.map((it) => renderCard(it, true))}
          </CapabilityZone>

          <CapabilityZone
            title="可添加"
            count={available.length}
            isEmpty={available.length === 0 && !trailingAvailable}
            emptyText={availableEmptyText}
            hint={available.length > 0 ? "点击卡片启用" : undefined}
            trailing={trailingAvailable}
          >
            {available.map((it) => renderCard(it, false))}
          </CapabilityZone>
        </>
      )}
    </div>
  );
}
