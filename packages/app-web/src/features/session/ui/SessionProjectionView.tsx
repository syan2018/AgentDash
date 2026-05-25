import { useCallback, useEffect, useState } from "react";
import type {
  SessionProjectionSegmentViewResponse,
  SessionProjectionViewResponse,
} from "../../../generated/session-contracts";
import { fetchSessionContextProjection } from "../../../services/session";

export interface SessionProjectionViewProps {
  sessionId: string | null;
  refreshKey?: number;
}

export interface SessionProjectionViewPanelProps {
  projection: SessionProjectionViewResponse | null;
  isLoading?: boolean;
  error?: string | null;
  onRefresh?: () => void;
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
        </div>
      </div>
    </div>
  );
}

export function SessionProjectionViewPanel({
  projection,
  isLoading = false,
  error = null,
  onRefresh,
}: SessionProjectionViewPanelProps) {
  return (
    <div className="border-b border-border bg-background px-5 py-3">
      <div className="mx-auto w-full max-w-4xl rounded-[8px] border border-border bg-secondary/20">
        <div className="flex flex-wrap items-center gap-2 px-3 py-2">
          <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-semibold uppercase text-muted-foreground">
            MODEL CONTEXT
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
    </div>
  );
}

export function SessionProjectionView({
  sessionId,
  refreshKey = 0,
}: SessionProjectionViewProps) {
  const [projection, setProjection] = useState<SessionProjectionViewResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!sessionId) {
      setProjection(null);
      return;
    }
    setIsLoading(true);
    setError(null);
    try {
      const next = await fetchSessionContextProjection(sessionId);
      setProjection(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : "加载模型上下文失败");
    } finally {
      setIsLoading(false);
    }
  }, [sessionId]);

  useEffect(() => {
    void refresh();
  }, [refresh, refreshKey]);

  return (
    <SessionProjectionViewPanel
      projection={projection}
      isLoading={isLoading}
      error={error}
      onRefresh={() => void refresh()}
    />
  );
}

export default SessionProjectionView;
