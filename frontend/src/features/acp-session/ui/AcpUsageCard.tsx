/**
 * ACP 用量卡片
 *
 * 渲染 usage_update 事件，显示 token 使用量。
 * 支持 ACP 标准字段（size/used）和 AgentDash 扩展字段（inputTokens/outputTokens 等）。
 */

import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { extractAgentDashMetaFromUpdate } from "../model/agentdashMeta";

export interface AcpUsageCardProps {
  update: SessionUpdate;
}

function formatTokenCount(n: number | undefined): string {
  if (n == null) return "-";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

export function AcpUsageCard({ update }: AcpUsageCardProps) {
  if (update.sessionUpdate !== "usage_update") return null;

  const u = update as Record<string, unknown>;

  // ACP 标准字段
  const size = typeof u.size === "number" ? u.size : undefined;
  const used = typeof u.used === "number" ? u.used : undefined;

  // AgentDash 扩展字段（可能在 _meta 或直接在 update 上）
  const meta = extractAgentDashMetaFromUpdate(update);
  const metaData = meta?.event?.data as Record<string, unknown> | undefined;

  const inputTokens = (typeof u.inputTokens === "number" ? u.inputTokens : undefined)
    ?? (typeof metaData?.inputTokens === "number" ? metaData.inputTokens : undefined);
  const outputTokens = (typeof u.outputTokens === "number" ? u.outputTokens : undefined)
    ?? (typeof metaData?.outputTokens === "number" ? metaData.outputTokens : undefined);
  const cacheReadTokens = (typeof u.cacheReadTokens === "number" ? u.cacheReadTokens : undefined)
    ?? (typeof metaData?.cacheReadTokens === "number" ? metaData.cacheReadTokens : undefined);

  const hasStandard = size != null || used != null;
  const hasExtended = inputTokens != null || outputTokens != null;

  if (!hasStandard && !hasExtended) return null;

  const usedPercent = (size != null && used != null && size > 0)
    ? Math.round((used / size) * 100)
    : undefined;

  return (
    <div className="flex flex-wrap items-center gap-3 rounded-[10px] border border-border bg-secondary/60 px-3.5 py-2 text-xs text-muted-foreground">
      <span className="inline-flex rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
        TOKENS
      </span>
      {hasStandard && (
        <>
          <span>
            上下文: <span className="font-medium text-foreground/70">{formatTokenCount(used)}</span>
            {size != null && <span className="text-muted-foreground/60">/{formatTokenCount(size)}</span>}
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
        </>
      )}
      {inputTokens != null && (
        <span>输入: <span className="font-medium text-foreground/70">{formatTokenCount(inputTokens)}</span></span>
      )}
      {outputTokens != null && (
        <span>输出: <span className="font-medium text-foreground/70">{formatTokenCount(outputTokens)}</span></span>
      )}
      {cacheReadTokens != null && cacheReadTokens > 0 && (
        <span>缓存: <span className="font-medium text-foreground/70">{formatTokenCount(cacheReadTokens)}</span></span>
      )}
    </div>
  );
}

export default AcpUsageCard;
