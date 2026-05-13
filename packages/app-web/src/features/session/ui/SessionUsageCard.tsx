/**
 * 用量卡片
 *
 * 渲染 token_usage_updated 事件。
 */

import type { BackboneEvent } from "../../../generated/backbone-protocol";

export interface AcpUsageCardProps {
  event: BackboneEvent;
}

function formatTokenCount(n: number | undefined): string {
  if (n == null) return "-";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

export function AcpUsageCard({ event }: AcpUsageCardProps) {
  if (event.type !== "token_usage_updated") return null;

  const usage = event.payload.tokenUsage;
  const total = usage.total;
  const maxTokens = usage.modelContextWindow ?? undefined;
  const used = total.totalTokens;

  const usedPercent = (maxTokens != null && used > 0 && maxTokens > 0)
    ? Math.round((used / maxTokens) * 100)
    : undefined;

  return (
    <div className="flex flex-wrap items-center gap-3 rounded-[10px] border border-border bg-secondary/60 px-3.5 py-2 text-xs text-muted-foreground">
      <span className="inline-flex rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
        TOKENS
      </span>
      <span>
        上下文: <span className="font-medium text-foreground/70">{formatTokenCount(used)}</span>
        {maxTokens != null && <span className="text-muted-foreground/60">/{formatTokenCount(maxTokens)}</span>}
      </span>
      {usedPercent != null && (
        <div className="flex items-center gap-1">
          <div className="h-1 w-12 overflow-hidden rounded-full bg-background">
            <div
              className={`h-full rounded-full transition-all ${usedPercent > 80 ? "bg-warning" : "bg-primary/60"}`}
              style={{ width: `${Math.min(usedPercent, 100)}%` }}
            />
          </div>
          <span className="text-muted-foreground/60 tabular-nums">{usedPercent}%</span>
        </div>
      )}
      <span>输入: <span className="font-medium text-foreground/70">{formatTokenCount(total.inputTokens)}</span></span>
      <span>输出: <span className="font-medium text-foreground/70">{formatTokenCount(total.outputTokens)}</span></span>
      {total.cachedInputTokens > 0 && (
        <span>缓存: <span className="font-medium text-foreground/70">{formatTokenCount(total.cachedInputTokens)}</span></span>
      )}
    </div>
  );
}

export default AcpUsageCard;
