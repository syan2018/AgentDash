import { useCallback, useEffect, useRef, useState } from "react";
import type {
  AgentContextContribution,
  AgentContextSnapshot,
} from "../../../generated/agent-service-api";
import type { ContextFrame } from "../../../generated/backbone-protocol";
import {
  fetchAgentRunRuntimeContextProjection,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";
import type { TokenUsageInfo } from "../model/types";
import type { SessionChatCommandModel } from "./SessionChatViewTypes";

export interface SessionProjectionViewProps {
  agentRunTarget?: AgentRunRuntimeTarget | null;
  refreshKey?: number | bigint;
  tokenUsage?: TokenUsageInfo | null;
  compactContextCommand?: SessionChatCommandModel;
  onCompactContext?: () => Promise<void>;
  embedded?: boolean;
}

export interface SessionProjectionViewPanelProps {
  projection: AgentContextSnapshot | null;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  tokenUsage?: TokenUsageInfo | null;
  compactContextCommand?: SessionChatCommandModel;
  onCompactContext?: () => Promise<void>;
  isLoading?: boolean;
  error?: string | null;
  onRefresh?: () => void;
  embedded?: boolean;
}

interface ContextCompactionActionState {
  kind: "none" | "pending" | "success" | "error";
  message?: string;
}

function CompactContextIcon({ loading }: { loading: boolean }) {
  if (loading) {
    return (
      <span
        aria-hidden="true"
        className="h-3.5 w-3.5 animate-spin rounded-[8px] border border-current border-t-transparent"
      />
    );
  }
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      className="h-3.5 w-3.5"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M8 3v5H3M16 21v-5h5M3 8l6-6M21 16l-6 6M16 3v5h5M8 21v-5H3M21 8l-6-6M3 16l6 6" />
    </svg>
  );
}

function formatNumber(value: number | undefined): string {
  if (value == null) return "-";
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return String(value);
}

function contributionKey(contribution: AgentContextContribution, index: number): string {
  switch (contribution.kind) {
    case "frame":
      return `frame:${contribution.frame.id}`;
    case "message":
      return `message:${contribution.source_entry_id}:${index}`;
    case "opaque":
      return `opaque:${contribution.label}:${index}`;
  }
}

function FrameContribution({ frame }: { frame: ContextFrame }) {
  return (
    <article className="space-y-2 border-t border-border/70 px-3 py-3 text-xs">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 font-medium">
          ContextFrame
        </span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5">{frame.kind}</span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5">{frame.message_role}</span>
        <span className="truncate font-mono text-[10px] text-muted-foreground" title={frame.id}>
          {frame.id}
        </span>
      </div>
      <p className="whitespace-pre-wrap text-foreground/85">{frame.rendered_text || "(empty)"}</p>
      <details>
        <summary className="cursor-pointer text-[10px] text-muted-foreground">
          完整结构 · {frame.sections.length} sections
        </summary>
        <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap rounded-[6px] bg-secondary p-2 text-[10px] text-muted-foreground">
          {JSON.stringify(frame, (_, value: unknown) =>
            typeof value === "bigint" ? value.toString() : value, 2)}
        </pre>
      </details>
    </article>
  );
}

function MessageContribution({
  contribution,
}: {
  contribution: Extract<AgentContextContribution, { kind: "message" }>;
}) {
  return (
    <article className="space-y-2 border-t border-border/70 px-3 py-3 text-xs">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 font-medium">
          Message
        </span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5">{contribution.role}</span>
        {contribution.is_error && (
          <span className="rounded-[6px] bg-destructive/10 px-1.5 py-0.5 text-destructive">
            error
          </span>
        )}
        <span className="truncate font-mono text-[10px] text-muted-foreground">
          {contribution.source_entry_id}
        </span>
      </div>
      <p className="whitespace-pre-wrap text-foreground/85">{contribution.content || "(empty)"}</p>
      {(contribution.tool_call_id || contribution.tool_calls.length > 0) && (
        <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-[6px] bg-secondary p-2 text-[10px] text-muted-foreground">
          {JSON.stringify({
            tool_call_id: contribution.tool_call_id,
            tool_calls: contribution.tool_calls,
          }, null, 2)}
        </pre>
      )}
    </article>
  );
}

function ContributionRow({ contribution }: { contribution: AgentContextContribution }) {
  switch (contribution.kind) {
    case "frame":
      return <FrameContribution frame={contribution.frame} />;
    case "message":
      return <MessageContribution contribution={contribution} />;
    case "opaque":
      return (
        <article className="space-y-1 border-t border-border/70 px-3 py-3 text-xs">
          <div className="font-medium">{contribution.label}</div>
          <p className="text-muted-foreground">{contribution.evidence}</p>
        </article>
      );
  }
}

export function SessionProjectionViewPanel({
  projection,
  agentRunTarget = null,
  tokenUsage,
  compactContextCommand,
  onCompactContext,
  isLoading = false,
  error = null,
  onRefresh,
  embedded = false,
}: SessionProjectionViewPanelProps) {
  const [compactAction, setCompactAction] = useState<ContextCompactionActionState>({ kind: "none" });
  const compactPending = compactAction.kind === "pending";
  const compactUnavailableReason =
    compactContextCommand?.unavailable_reason
    ?? compactContextCommand?.disabled_code
    ?? "当前不可压缩";
  const compactDisabled =
    !agentRunTarget
    || !compactContextCommand
    || !onCompactContext
    || !compactContextCommand.enabled
    || compactPending;
  const handleCompactContext = useCallback(async () => {
    if (!agentRunTarget || !compactContextCommand || compactPending) return;
    if (!compactContextCommand.enabled) {
      setCompactAction({ kind: "error", message: compactUnavailableReason });
      return;
    }
    setCompactAction({ kind: "pending", message: "提交中" });
    try {
      await onCompactContext?.();
      setCompactAction({ kind: "success", message: "压缩请求已接受" });
      onRefresh?.();
    } catch (err) {
      setCompactAction({
        kind: "error",
        message: err instanceof Error ? err.message : "压缩请求失败",
      });
    }
  }, [
    agentRunTarget,
    compactContextCommand,
    compactPending,
    compactUnavailableReason,
    onCompactContext,
    onRefresh,
  ]);

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
              snapshot #{projection.snapshot_revision}
            </span>
            <span className="text-xs text-muted-foreground">
              {projection.authority} · {projection.fidelity}
            </span>
            <span className="text-xs text-muted-foreground">
              {projection.contributions.length} contributions
            </span>
            {projection.context_revision && (
              <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
                {projection.context_revision}
              </span>
            )}
            {tokenUsage && (
              <span className="text-xs text-muted-foreground">
                当前 {formatNumber(tokenUsage.currentContextTokens)}
              </span>
            )}
          </>
        ) : (
          <span className="text-xs text-muted-foreground">{isLoading ? "加载中" : "暂无上下文"}</span>
        )}
        <div className="ml-auto flex items-center gap-1.5">
          {compactAction.message && (
            <span className={compactAction.kind === "error" ? "text-xs text-destructive" : "text-xs text-muted-foreground"}>
              {compactAction.message}
            </span>
          )}
          {compactContextCommand && (
            <button
              type="button"
              onClick={() => { void handleCompactContext(); }}
              disabled={compactDisabled}
              title={compactPending
                ? "压缩请求提交中"
                : compactContextCommand.enabled
                  ? "压缩上下文"
                  : compactUnavailableReason}
              aria-label="手动压缩上下文"
              className="inline-flex h-7 items-center gap-1 rounded-[8px] border border-border bg-background px-2 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
            >
              <CompactContextIcon loading={compactPending} />
              <span>{compactPending ? "提交中" : "压缩"}</span>
            </button>
          )}
          <button
            type="button"
            onClick={onRefresh}
            disabled={isLoading}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
          >
            {isLoading ? "刷新中" : "刷新"}
          </button>
        </div>
      </div>
      {projection && (
        <div className="border-t border-border px-3 py-2 text-[10px] text-muted-foreground">
          <span className="font-mono" title={projection.recipe_digest}>
            recipe {projection.recipe_digest}
          </span>
        </div>
      )}
      {error && <div className="border-t border-border px-3 py-2 text-xs text-destructive">{error}</div>}
      {projection && projection.contributions.length > 0 && (
        <div className="max-h-[34rem] overflow-y-auto">
          {projection.contributions.map((contribution, index) => (
            <ContributionRow
              key={contributionKey(contribution, index)}
              contribution={contribution}
            />
          ))}
        </div>
      )}
      {projection && projection.contributions.length === 0 && (
        <div className="border-t border-border px-3 py-4 text-xs text-muted-foreground">
          当前模型输入配方没有 contribution。
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
  compactContextCommand,
  onCompactContext,
  embedded = false,
}: SessionProjectionViewProps) {
  const [projection, setProjection] = useState<AgentContextSnapshot | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestGeneration = useRef(0);
  const activeRequest = useRef<AbortController | null>(null);
  const targetRunId = agentRunTarget?.runId;
  const targetAgentId = agentRunTarget?.agentId;

  const refresh = useCallback(async () => {
    if (!targetRunId || !targetAgentId) {
      activeRequest.current?.abort();
      setProjection(null);
      return;
    }
    activeRequest.current?.abort();
    const controller = new AbortController();
    activeRequest.current = controller;
    const generation = ++requestGeneration.current;
    setIsLoading(true);
    setError(null);
    try {
      const next = await fetchAgentRunRuntimeContextProjection({
        runId: targetRunId,
        agentId: targetAgentId,
      }, controller.signal);
      if (requestGeneration.current === generation && !controller.signal.aborted) {
        setProjection(next);
      }
    } catch (err) {
      if (requestGeneration.current === generation && !controller.signal.aborted) {
        setError(err instanceof Error ? err.message : "加载模型上下文失败");
      }
    } finally {
      if (requestGeneration.current === generation && !controller.signal.aborted) {
        setIsLoading(false);
      }
    }
  }, [targetAgentId, targetRunId]);

  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => {
      if (!cancelled) void refresh();
    });
    return () => {
      cancelled = true;
      activeRequest.current?.abort();
    };
  }, [refresh, refreshKey]);

  return (
    <SessionProjectionViewPanel
      projection={projection}
      agentRunTarget={agentRunTarget}
      tokenUsage={tokenUsage}
      compactContextCommand={compactContextCommand}
      onCompactContext={onCompactContext}
      isLoading={isLoading}
      error={error}
      onRefresh={() => void refresh()}
      embedded={embedded}
    />
  );
}

export default SessionProjectionView;
