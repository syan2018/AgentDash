import { useCallback, useEffect, useState } from "react";
import type {
  SessionLineageRecordResponse,
  SessionLineageViewResponse,
} from "../../../generated/session-contracts";
import { fetchSessionLineage } from "../../../services/session";

export interface SessionLineageViewProps {
  sessionId: string | null;
  refreshKey?: number;
}

export interface SessionLineageViewPanelProps {
  lineage: SessionLineageViewResponse | null;
  isLoading?: boolean;
  error?: string | null;
  onRefresh?: () => void;
}

function relationLabel(value: SessionLineageRecordResponse["relation_kind"]): string {
  switch (value) {
    case "fork":
      return "fork";
    case "companion":
      return "companion";
    case "spawned_agent":
      return "spawned agent";
    case "rollback_branch":
      return "rollback branch";
    default:
      return value;
  }
}

function statusLabel(value: SessionLineageRecordResponse["status"]): string {
  switch (value) {
    case "open":
      return "open";
    case "closed":
      return "closed";
    case "archived":
      return "archived";
    default:
      return value;
  }
}

function eventSeqLabel(value: number | undefined): string {
  return value == null ? "unbound" : `#${value}`;
}

function compactId(value: string): string {
  if (value.length <= 18) return value;
  return `${value.slice(0, 8)}...${value.slice(-6)}`;
}

function ParentSummary({ record }: { record: SessionLineageRecordResponse }) {
  return (
    <div className="min-w-0 rounded-[8px] border border-border bg-background px-3 py-2">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="rounded-[6px] border border-primary/20 bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary">
          {relationLabel(record.relation_kind)}
        </span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
          {statusLabel(record.status)}
        </span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
          fork {eventSeqLabel(record.fork_point_event_seq)}
        </span>
      </div>
      <div className="mt-1 truncate font-mono text-[11px] text-foreground" title={record.parent_session_id}>
        parent {record.parent_session_id}
      </div>
      {record.fork_point_compaction_id && (
        <div className="mt-1 truncate font-mono text-[10px] text-muted-foreground" title={record.fork_point_compaction_id}>
          compaction {compactId(record.fork_point_compaction_id)}
        </div>
      )}
    </div>
  );
}

function ChildRow({ record }: { record: SessionLineageRecordResponse }) {
  return (
    <div className="grid gap-2 border-t border-border/70 px-3 py-2.5 text-xs md:grid-cols-[150px_1fr]">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] text-foreground">
          {relationLabel(record.relation_kind)}
        </span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
          {statusLabel(record.status)}
        </span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
          {eventSeqLabel(record.fork_point_event_seq)}
        </span>
      </div>
      <div className="min-w-0 truncate font-mono text-[11px] text-foreground" title={record.child_session_id}>
        {record.child_session_id}
      </div>
    </div>
  );
}

export function SessionLineageViewPanel({
  lineage,
  isLoading = false,
  error = null,
  onRefresh,
}: SessionLineageViewPanelProps) {
  const parent = lineage?.lineage;
  const childCount = lineage?.children.length ?? 0;
  const ancestorCount = lineage?.ancestors.length ?? 0;

  return (
    <div className="border-b border-border bg-background px-5 py-3">
      <div className="mx-auto w-full max-w-4xl rounded-[8px] border border-border bg-secondary/20">
        <div className="flex flex-wrap items-center gap-2 px-3 py-2">
          <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-semibold uppercase text-muted-foreground">
            BRANCH
          </span>
          {lineage ? (
            <>
              <span className="text-xs text-muted-foreground">
                {parent ? "child branch" : "root session"}
              </span>
              <span className="text-xs text-muted-foreground">
                {ancestorCount} ancestors
              </span>
              <span className="text-xs text-muted-foreground">
                {childCount} children
              </span>
            </>
          ) : (
            <span className="text-xs text-muted-foreground">
              {isLoading ? "加载中" : "暂无 lineage"}
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
        {lineage && (
          <div className="border-t border-border px-3 py-3">
            {parent ? (
              <ParentSummary record={parent} />
            ) : (
              <div className="rounded-[8px] border border-border bg-background px-3 py-2 text-xs text-muted-foreground">
                当前会话没有 parent lineage，是当前树的 root。
              </div>
            )}
          </div>
        )}
        {lineage && lineage.children.length > 0 && (
          <div className="max-h-56 overflow-y-auto">
            {lineage.children.map((child) => (
              <ChildRow key={child.child_session_id} record={child} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export function SessionLineageView({
  sessionId,
  refreshKey = 0,
}: SessionLineageViewProps) {
  const [lineage, setLineage] = useState<SessionLineageViewResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!sessionId) {
      setLineage(null);
      return;
    }
    setIsLoading(true);
    setError(null);
    try {
      const next = await fetchSessionLineage(sessionId);
      setLineage(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : "加载分支关系失败");
    } finally {
      setIsLoading(false);
    }
  }, [sessionId]);

  useEffect(() => {
    void refresh();
  }, [refresh, refreshKey]);

  return (
    <SessionLineageViewPanel
      lineage={lineage}
      isLoading={isLoading}
      error={error}
      onRefresh={() => void refresh()}
    />
  );
}

export default SessionLineageView;
