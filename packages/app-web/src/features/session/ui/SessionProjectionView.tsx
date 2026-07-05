import { useCallback, useEffect, useState } from "react";
import type {
  SessionProjectionSegmentViewResponse,
  SessionProjectionViewResponse,
} from "../../../generated/session-contracts";
import {
  fetchAgentRunRuntimeContextProjection,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";
import type { TokenUsageInfo } from "../model/types";

export interface SessionProjectionViewProps {
  agentRunTarget?: AgentRunRuntimeTarget | null;
  refreshKey?: number;
  tokenUsage?: TokenUsageInfo | null;
  /** 浮层模式：去掉整页内联的外层留白/边框，适配 popover 容器 */
  embedded?: boolean;
}

export interface SessionProjectionViewPanelProps {
  projection: SessionProjectionViewResponse | null;
  tokenUsage?: TokenUsageInfo | null;
  isLoading?: boolean;
  error?: string | null;
  onRefresh?: () => void;
  /** 浮层模式：去掉整页内联的外层留白/边框，适配 popover 容器 */
  embedded?: boolean;
}

async function fetchSessionProjectionForTarget({
  agentRunTarget,
}: {
  agentRunTarget?: AgentRunRuntimeTarget | null;
}): Promise<SessionProjectionViewResponse | null> {
  if (agentRunTarget) {
    return fetchAgentRunRuntimeContextProjection(agentRunTarget);
  }
  return null;
}

interface ContextCategoryRow {
  id: string;
  label: string;
  tokens: number;
  source: string;
  deferred?: boolean;
}

function formatNumber(value: number | undefined): string {
  if (value == null) return "-";
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return String(value);
}

function formatRange(segment: SessionProjectionSegmentViewResponse): string {
  if (segment.source_range) {
    return `#${segment.source_range.start_event_seq}-#${segment.source_range.end_event_seq}`;
  }
  if (segment.source_event_seq != null) {
    return `#${segment.source_event_seq}`;
  }
  return "unbound";
}

function originLabel(value: string): string {
  switch (value) {
    case "projection":
      return "projection";
    case "event":
      return "event";
    default:
      return value;
  }
}

function roleLabel(value: string): string {
  switch (value) {
    case "compaction_summary":
      return "summary";
    case "tool_result":
      return "tool";
    default:
      return value;
  }
}

function buildContextCategories(
  projection: SessionProjectionViewResponse | null,
  tokenUsage: TokenUsageInfo | null | undefined,
): ContextCategoryRow[] {
  const rows: ContextCategoryRow[] = (projection?.context_usage.categories ?? []).map((category) => ({
    id: category.kind,
    label: category.label,
    tokens: category.token_estimate,
    source: category.deferred ? `${category.source} · deferred` : category.source,
    deferred: category.deferred,
  }));
  if (tokenUsage && tokenUsage.pendingEstimateTokens > 0) {
    rows.push({
      id: "pending_estimate",
      label: "待确认估算",
      tokens: tokenUsage.pendingEstimateTokens,
      source: "local_estimate",
    });
  }
  if (tokenUsage && tokenUsage.reserveTokens > 0) {
    rows.push({
      id: "reserve",
      label: "预留缓冲",
      tokens: tokenUsage.reserveTokens,
      source: "policy",
    });
  }
  const contextWindow = tokenUsage?.effectiveContextWindow ?? tokenUsage?.modelContextWindow;
  if (tokenUsage && contextWindow) {
    const reserveToSubtract = tokenUsage.effectiveContextWindow == null ? tokenUsage.reserveTokens : 0;
    const freeTokens = Math.max(
      0,
      contextWindow - tokenUsage.currentContextTokens - reserveToSubtract,
    );
    rows.push({
      id: "free_space",
      label: "剩余空间",
      tokens: freeTokens,
      source: "derived",
    });
  }
  return rows;
}

function SegmentRow({ segment }: { segment: SessionProjectionSegmentViewResponse }) {
  const provenance = segment.provenance;
  return (
    <div className="grid gap-2 border-t border-border/70 px-3 py-2.5 text-xs md:grid-cols-[160px_1fr]">
      <div className="min-w-0 space-y-1">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-medium text-foreground">
            {originLabel(segment.origin)}
          </span>
          <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
            {roleLabel(segment.role)}
          </span>
          {segment.synthetic && (
            <span className="rounded-[6px] border border-primary/20 bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
              synthetic
            </span>
          )}
        </div>
        <div className="truncate font-mono text-[11px] text-muted-foreground" title={segment.id}>
          {segment.segment_type}
        </div>
        <div className="font-mono text-[11px] text-muted-foreground/70">
          {formatRange(segment)}
        </div>
      </div>
      <div className="min-w-0 space-y-1.5">
        <p className="line-clamp-3 whitespace-pre-wrap text-foreground/85">
          {segment.preview || "(empty)"}
        </p>
        <div className="flex flex-wrap gap-1.5 text-[10px] text-muted-foreground">
          {provenance.compaction_id && (
            <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 font-mono">
              {provenance.compaction_id}
            </span>
          )}
          {provenance.strategy && (
            <span className="rounded-[6px] bg-secondary px-1.5 py-0.5">
              {provenance.strategy}
            </span>
          )}
          {provenance.trigger && (
            <span className="rounded-[6px] bg-secondary px-1.5 py-0.5">
              {provenance.trigger}
            </span>
          )}
          {provenance.phase && (
            <span className="rounded-[6px] bg-secondary px-1.5 py-0.5">
              {provenance.phase}
            </span>
          )}
          <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 font-mono">
            {segment.message_ref.turn_id}:{segment.message_ref.entry_index}
          </span>
          {segment.token_estimate != null && (
            <span className="rounded-[6px] bg-secondary px-1.5 py-0.5">
              {formatNumber(segment.token_estimate)} tokens
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

export function SessionProjectionViewPanel({
  projection,
  tokenUsage,
  isLoading = false,
  error = null,
  onRefresh,
  embedded = false,
}: SessionProjectionViewPanelProps) {
  const categories = buildContextCategories(projection, tokenUsage);
  const messageBreakdown = projection?.context_usage.messages;
  const topTools = projection?.context_usage.top_tools ?? [];
  const topAttachments = projection?.context_usage.top_attachments ?? [];
  const card = (
      <div
        className={
          embedded
            ? "w-full overflow-hidden rounded-[10px] border border-border bg-popover shadow-lg"
            : "mx-auto w-full max-w-4xl rounded-[8px] border border-border bg-secondary/20"
        }
      >
        <div className="flex flex-wrap items-center gap-2 px-3 py-2">
          <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-semibold uppercase text-muted-foreground">
            CONTEXT
          </span>
          {projection ? (
            <>
              <span className="text-xs text-muted-foreground">
                v{projection.projection_version} · head #{projection.head_event_seq}
              </span>
              <span className="text-xs text-muted-foreground">
                {formatNumber(projection.message_count)} segments
              </span>
              {projection.token_estimate != null && (
                <span className="text-xs text-muted-foreground">
                  {formatNumber(projection.token_estimate)} tokens
                </span>
              )}
              {tokenUsage && (
                <span className="text-xs text-muted-foreground">
                  当前 {formatNumber(tokenUsage.currentContextTokens)}
                  {tokenUsage.effectiveContextWindow != null && ` / ${formatNumber(tokenUsage.effectiveContextWindow)}`}
                </span>
              )}
              {projection.active_compaction_id && (
                <span className="truncate rounded-[6px] bg-background px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
                  {projection.active_compaction_id}
                </span>
              )}
            </>
          ) : (
            <span className="text-xs text-muted-foreground">
              {isLoading ? "加载中" : "暂无投影"}
            </span>
          )}
          <button
            type="button"
            onClick={onRefresh}
            disabled={isLoading}
            className="ml-auto rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
          >
            {isLoading ? "刷新中" : "刷新"}
          </button>
        </div>
        {error && (
          <div className="border-t border-border px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        )}
        {(categories.length > 0 || projection) && (
          <div className="grid gap-3 border-t border-border px-3 py-3 text-xs md:grid-cols-[1fr_1fr]">
            <div className="space-y-2">
              <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                构成
              </div>
              <div className="space-y-1">
                {categories.map((category) => (
                  <div key={category.id} className="flex items-center gap-2">
                    <span className="min-w-0 flex-1 truncate text-foreground/80">{category.label}</span>
                    <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
                      {category.source}
                    </span>
                    <span className="w-14 text-right font-mono text-muted-foreground">
                      {formatNumber(category.tokens)}
                    </span>
                  </div>
                ))}
              </div>
            </div>
            <div className="space-y-2">
              <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                消息明细
              </div>
              <div className="grid grid-cols-2 gap-1 text-muted-foreground">
                <span>用户 {formatNumber(messageBreakdown?.user_message_tokens)}</span>
                <span>助手 {formatNumber(messageBreakdown?.assistant_message_tokens)}</span>
                <span>工具调用 {formatNumber(messageBreakdown?.tool_call_tokens)}</span>
                <span>工具结果 {formatNumber(messageBreakdown?.tool_result_tokens)}</span>
                <span>附件 {formatNumber(messageBreakdown?.attachment_tokens)}</span>
              </div>
              {topTools.length > 0 && (
                <div className="space-y-1">
                  <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                    Top Tools
                  </div>
                  {topTools.map((tool) => (
                    <div key={tool.name} className="flex items-center gap-2">
                      <span className="min-w-0 flex-1 truncate text-foreground/80">{tool.name}</span>
                      <span className="font-mono text-muted-foreground">
                        {formatNumber(tool.call_tokens)} / {formatNumber(tool.result_tokens)}
                      </span>
                    </div>
                  ))}
                </div>
              )}
              {topAttachments.length > 0 && (
                <div className="space-y-1">
                  <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                    Top Attachments
                  </div>
                  {topAttachments.map((attachment) => (
                    <div key={attachment.name} className="flex items-center gap-2">
                      <span className="min-w-0 flex-1 truncate text-foreground/80">{attachment.name}</span>
                      <span className="font-mono text-muted-foreground">
                        {formatNumber(attachment.tokens)}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        )}
        {projection && projection.segments.length > 0 && (
          <div className="max-h-72 overflow-y-auto">
            {projection.segments.map((segment) => (
              <SegmentRow key={segment.id} segment={segment} />
            ))}
          </div>
        )}
        {projection && projection.segments.length === 0 && (
          <div className="border-t border-border px-3 py-4 text-xs text-muted-foreground">
            当前 projection 没有可展示 segment。
          </div>
        )}
      </div>
  );
  if (embedded) return card;
  return <div className="border-b border-border bg-background px-5 py-3">{card}</div>;
}

export function SessionProjectionView({
  agentRunTarget = null,
  refreshKey = 0,
  tokenUsage = null,
  embedded = false,
}: SessionProjectionViewProps) {
  const [projection, setProjection] = useState<SessionProjectionViewResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!agentRunTarget) {
      setProjection(null);
      return;
    }
    setIsLoading(true);
    setError(null);
    try {
      const next = await fetchSessionProjectionForTarget({ agentRunTarget });
      setProjection(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : "加载模型上下文失败");
    } finally {
      setIsLoading(false);
    }
  }, [agentRunTarget]);

  useEffect(() => {
    void refresh();
  }, [refresh, refreshKey]);

  return (
    <SessionProjectionViewPanel
      projection={projection}
      tokenUsage={tokenUsage}
      isLoading={isLoading}
      error={error}
      onRefresh={() => void refresh()}
      embedded={embedded}
    />
  );
}

export default SessionProjectionView;
