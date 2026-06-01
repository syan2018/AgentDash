import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import type { RuntimeSessionTraceView } from "../types";
import { SessionEntry, SessionLineageView, SessionProjectionView } from "../features/session";
import { useSessionFeed } from "../features/session/model";
import { fetchRuntimeTrace } from "../services/lifecycle";
import { fetchSessionMeta, type SessionMeta } from "../services/session";

interface SessionPageProps {
  sessionId?: string;
}

function TraceSection({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="rounded-[8px] border border-border bg-background">
      <div className="border-b border-border px-4 py-3">
        <h2 className="text-sm font-semibold text-foreground">{title}</h2>
      </div>
      <div className="p-4">{children}</div>
    </section>
  );
}

function EmptyTrace({ message }: { message: string }) {
  return (
    <p className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-6 text-center text-sm text-muted-foreground">
      {message}
    </p>
  );
}

function TraceRefButton({
  label,
  onClick,
}: {
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="rounded-[6px] border border-border bg-secondary/40 px-2 py-1 font-mono text-xs text-muted-foreground hover:text-foreground"
    >
      {label}
    </button>
  );
}

export function SessionPage({ sessionId: propSessionId }: SessionPageProps) {
  const navigate = useNavigate();
  const sessionId = propSessionId ?? null;
  const [meta, setMeta] = useState<SessionMeta | null>(null);
  const [trace, setTrace] = useState<RuntimeSessionTraceView | null>(null);
  const [traceError, setTraceError] = useState<string | null>(null);
  const [showProjection, setShowProjection] = useState(false);
  const [showLineage, setShowLineage] = useState(false);

  const {
    displayItems,
    rawEvents,
    isConnected,
    isLoading,
    error: streamError,
    reconnect,
    streamingEntryId,
    tokenUsage,
  } = useSessionFeed({
    sessionId: sessionId ?? "__missing__",
    enabled: sessionId !== null,
  });

  useEffect(() => {
    if (!sessionId) {
      setMeta(null);
      setTrace(null);
      setTraceError("Runtime trace id 缺失");
      return;
    }

    let cancelled = false;
    setTraceError(null);
    void Promise.all([
      fetchSessionMeta(sessionId),
      fetchRuntimeTrace(sessionId),
    ])
      .then(([nextMeta, nextTrace]) => {
        if (cancelled) return;
        setMeta(nextMeta);
        setTrace(nextTrace);
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setTrace(null);
        setTraceError(error instanceof Error ? error.message : "Runtime trace 加载失败");
      });

    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  const title = meta?.title?.trim() || "Runtime Trace";
  const frameId = trace?.frame_ref?.frame_id ?? null;
  const agentId = trace?.frame_ref?.agent_id ?? null;
  const traceEventCount = useMemo(() => trace?.events.length ?? 0, [trace]);
  const traceTurnCount = useMemo(() => trace?.turns.length ?? 0, [trace]);

  return (
    <div className="h-full overflow-y-auto bg-background">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-4 px-6 py-5">
        <header className="flex flex-wrap items-center justify-between gap-3 border-b border-border pb-4">
          <div className="min-w-0">
            <div className="mb-2 flex flex-wrap items-center gap-2">
              <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                RUNTIME TRACE
              </span>
              <span className={`inline-flex items-center gap-1.5 rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground`}>
                <span className={`h-1.5 w-1.5 rounded-full ${isConnected ? "bg-success" : isLoading ? "bg-warning" : "bg-muted-foreground/40"}`} />
                {isConnected ? "已连接" : isLoading ? "连接中" : "未连接"}
              </span>
            </div>
            <h1 className="truncate text-lg font-semibold text-foreground">{title}</h1>
            {sessionId && (
              <p className="mt-1 font-mono text-xs text-muted-foreground">{sessionId}</p>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {agentId && frameId && (
              <button
                type="button"
                onClick={() => navigate(`/agent/${agentId}`, {
                  state: { frame_id: frameId },
                })}
                className="rounded-[8px] border border-border bg-background px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
              >
                打开 Agent
              </button>
            )}
            <button
              type="button"
              onClick={() => sessionId && void navigator.clipboard.writeText(sessionId)}
              className="rounded-[8px] border border-border bg-background px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            >
              复制 ID
            </button>
          </div>
        </header>

        {(traceError || streamError) && (
          <div className="flex items-center justify-between gap-3 rounded-[8px] border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            <span>{traceError ?? streamError?.message}</span>
            {streamError && (
              <button
                type="button"
                onClick={reconnect}
                className="rounded-[6px] border border-destructive/30 px-2 py-1 text-xs"
              >
                重连
              </button>
            )}
          </div>
        )}

        <TraceSection title="Trace Links">
          <div className="flex flex-wrap gap-2">
            {agentId && frameId && (
              <TraceRefButton
                label={`frame ${frameId}`}
                onClick={() => navigate(`/agent/${agentId}`, {
                  state: { frame_id: frameId },
                })}
              />
            )}
            <span className="rounded-[6px] border border-border bg-secondary px-2 py-1 text-xs text-muted-foreground">
              events {Math.max(rawEvents.length, traceEventCount)}
            </span>
            <span className="rounded-[6px] border border-border bg-secondary px-2 py-1 text-xs text-muted-foreground">
              turns {traceTurnCount}
            </span>
            {tokenUsage && (
              <span className="rounded-[6px] border border-border bg-secondary px-2 py-1 text-xs text-muted-foreground">
                tokens {tokenUsage.total.totalTokens}
              </span>
            )}
          </div>
        </TraceSection>

        <TraceSection title="Views">
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              onClick={() => setShowProjection((value) => !value)}
              className={`rounded-[8px] border px-3 py-1.5 text-xs transition-colors ${showProjection ? "border-primary/30 bg-primary/10 text-primary" : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground"}`}
            >
              Context Projection
            </button>
            <button
              type="button"
              onClick={() => setShowLineage((value) => !value)}
              className={`rounded-[8px] border px-3 py-1.5 text-xs transition-colors ${showLineage ? "border-primary/30 bg-primary/10 text-primary" : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground"}`}
            >
              Lineage
            </button>
          </div>
          {sessionId && showProjection && (
            <div className="mt-4">
              <SessionProjectionView sessionId={sessionId} refreshKey={rawEvents.length} tokenUsage={tokenUsage} />
            </div>
          )}
          {sessionId && showLineage && (
            <div className="mt-4">
              <SessionLineageView sessionId={sessionId} refreshKey={rawEvents.length} />
            </div>
          )}
        </TraceSection>

        <TraceSection title="Events">
          {isLoading && displayItems.length === 0 ? (
            <EmptyTrace message="正在加载 runtime events" />
          ) : displayItems.length === 0 ? (
            <EmptyTrace message="暂无 runtime event" />
          ) : (
            <div className="space-y-3">
              {displayItems.map((item) => {
                const key = "groupKey" in item ? item.groupKey : item.id;
                return (
                  <SessionEntry
                    key={key}
                    item={item}
                    isStreaming={key === streamingEntryId}
                    sessionId={sessionId}
                  />
                );
              })}
            </div>
          )}
        </TraceSection>
      </div>
    </div>
  );
}

export default SessionPage;
