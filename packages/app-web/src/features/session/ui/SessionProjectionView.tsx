import { useCallback, useState } from "react";
import type {
  AgentContextContribution,
} from "../../../generated/agent-runtime-contracts";
import type { ContextFrame } from "../../../generated/backbone-protocol";
import type {
  AgentRuntimeContextProjection,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-codecs";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import { useAgentRuntimeContextProjection } from "../../agent-run-runtime/model/useAgentRuntimeContextProjection";
import type { TokenUsageInfo } from "../model/types";
import type { SessionChatCommandModel } from "./SessionChatViewTypes";

type AgentContextUsageAnalysis = AgentRuntimeContextProjection["recipe"]["usage"];

export interface SessionProjectionViewProps {
  agentRunTarget?: AgentRunRuntimeTarget | null;
  contextCoordinate?: AgentRuntimeView["observation"]["context"] | null;
  tokenUsage?: TokenUsageInfo | null;
  compactContextCommand?: SessionChatCommandModel;
  onCompactContext?: () => Promise<void>;
  embedded?: boolean;
}

export interface SessionProjectionViewPanelProps {
  projection: AgentRuntimeContextProjection | null;
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

function formatNumber(value: number | bigint | undefined): string {
  if (value == null) return "-";
  const numeric = typeof value === "bigint" ? Number(value) : value;
  if (numeric >= 1_000_000) return `${(numeric / 1_000_000).toFixed(1)}M`;
  if (numeric >= 1_000) return `${(numeric / 1_000).toFixed(1)}K`;
  return String(numeric);
}

function formatPercentage(value: number | bigint, total: number | bigint): string {
  const numericTotal = Number(total);
  if (numericTotal <= 0) return "0%";
  return `${Math.round((Number(value) / numericTotal) * 100)}%`;
}

function contributionKey(contribution: AgentContextContribution, index: number): string {
  switch (contribution.kind) {
    case "frame":
      return `frame:${contribution.frame.id}`;
    case "tool":
      return `tool:${contribution.tool.name}:${index}`;
    case "message":
      return `message:${contribution.source_entry_id}:${index}`;
    case "opaque":
      return `opaque:${contribution.label}:${index}`;
  }
}

function ContextUsageSummary({
  usage,
  providerUsage,
}: {
  usage: AgentContextUsageAnalysis;
  providerUsage?: TokenUsageInfo | null;
}) {
  const messageRows = [
    ["用户消息", usage.messages.user_message_tokens],
    ["助手消息", usage.messages.assistant_message_tokens],
    ["工具调用", usage.messages.tool_call_tokens],
    ["工具结果", usage.messages.tool_result_tokens],
  ] as const;

  return (
    <section className="border-t border-border px-3 py-3 text-xs">
      <div className="flex flex-wrap items-baseline gap-2">
        <span className="font-medium">模型输入估算</span>
        <span className="text-base font-semibold">
          {formatNumber(usage.estimated_total_tokens)} tokens
        </span>
        {providerUsage && (
          <span className="text-muted-foreground">
            Provider 最近确认 {formatNumber(providerUsage.currentContextTokens)}
          </span>
        )}
      </div>
      <div className="mt-3 grid gap-4 md:grid-cols-2">
        <div>
          <div className="mb-1.5 font-medium text-muted-foreground">主要段落</div>
          <div className="space-y-1">
            {usage.categories.length === 0 && (
              <div className="text-muted-foreground">Agent 未提供可估算分类</div>
            )}
            {usage.categories.map((category) => (
              <div key={category.kind} className="flex items-center justify-between gap-3">
                <span className="truncate" title={`${category.kind} · ${category.source}`}>
                  {category.label}
                </span>
                <span className="shrink-0 font-mono text-muted-foreground">
                  {formatNumber(category.estimated_tokens)}
                  {" · "}
                  {formatPercentage(category.estimated_tokens, usage.estimated_total_tokens)}
                </span>
              </div>
            ))}
          </div>
        </div>
        <div>
          <div className="mb-1.5 font-medium text-muted-foreground">消息明细</div>
          <div className="space-y-1">
            {messageRows.map(([label, tokens]) => (
              <div key={label} className="flex items-center justify-between gap-3">
                <span>{label}</span>
                <span className="font-mono text-muted-foreground">{formatNumber(tokens)}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
      {usage.top_tools.length > 0 && (
        <details className="mt-3">
          <summary className="cursor-pointer font-medium text-muted-foreground">
            Top Tools · {usage.top_tools.length}
          </summary>
          <div className="mt-1.5 space-y-1">
            {usage.top_tools.map((tool) => (
              <div key={tool.name} className="flex flex-wrap items-center justify-between gap-x-3">
                <span className="font-mono">{tool.name}</span>
                <span className="text-muted-foreground">
                  定义 {formatNumber(tool.definition_tokens)}
                  {" · "}调用 {formatNumber(tool.call_tokens)}
                  {" · "}结果 {formatNumber(tool.result_tokens)}
                </span>
              </div>
            ))}
          </div>
        </details>
      )}
    </section>
  );
}

function FrameContribution({ frame }: { frame: ContextFrame }) {
  return (
    <article className="space-y-2 border-t border-border/70 px-3 py-3 text-xs">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 font-medium">
          ContextFrame
        </span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5">{frame.kind}</span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5">{frame.delivery_status}</span>
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

function ToolContribution({
  contribution,
}: {
  contribution: Extract<AgentContextContribution, { kind: "tool" }>;
}) {
  const { tool } = contribution;
  return (
    <article className="space-y-2 border-t border-border/70 px-3 py-3 text-xs">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 font-medium">
          Tool
        </span>
        <span className="font-mono">{tool.name}</span>
        <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 text-muted-foreground">
          {tool.context_usage_kind}
        </span>
      </div>
      <p className="text-foreground/85">{tool.description}</p>
      <details>
        <summary className="cursor-pointer text-[10px] text-muted-foreground">
          输入 Schema · {tool.source}
        </summary>
        <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap rounded-[6px] bg-secondary p-2 text-[10px] text-muted-foreground">
          {JSON.stringify(tool.input_schema, null, 2)}
        </pre>
      </details>
    </article>
  );
}

function ContributionRow({ contribution }: { contribution: AgentContextContribution }) {
  switch (contribution.kind) {
    case "frame":
      return <FrameContribution frame={contribution.frame} />;
    case "tool":
      return <ToolContribution contribution={contribution} />;
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
              snapshot #{projection.recipe.coordinate.snapshot_revision}
            </span>
            <span className="text-xs text-muted-foreground">
              {projection.recipe.coordinate.authority} · {projection.recipe.coordinate.fidelity}
            </span>
            <span className="text-xs text-muted-foreground">
              {projection.recipe.contributions.length} contributions
            </span>
            {projection.recipe.coordinate.context_revision && (
              <span className="rounded-[6px] bg-secondary px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
                {projection.recipe.coordinate.context_revision}
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
          <span className="font-mono" title={projection.recipe.coordinate.recipe_digest}>
            recipe {projection.recipe.coordinate.recipe_digest}
          </span>
        </div>
      )}
      {error && <div className="border-t border-border px-3 py-2 text-xs text-destructive">{error}</div>}
      {projection && (
        <ContextUsageSummary usage={projection.recipe.usage} providerUsage={tokenUsage} />
      )}
      {projection && projection.recipe.contributions.length > 0 && (
        <details className="border-t border-border">
          <summary className="cursor-pointer px-3 py-2 text-xs text-muted-foreground">
            完整模型输入 · {projection.recipe.contributions.length} 项
          </summary>
          <div className="max-h-[34rem] overflow-y-auto">
            {projection.recipe.contributions.map((contribution, index) => (
              <ContributionRow
                key={contributionKey(contribution, index)}
                contribution={contribution}
              />
            ))}
          </div>
        </details>
      )}
      {projection && projection.recipe.contributions.length === 0 && (
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
  contextCoordinate = null,
  tokenUsage = null,
  compactContextCommand,
  onCompactContext,
  embedded = false,
}: SessionProjectionViewProps) {
  const { state, refresh } = useAgentRuntimeContextProjection({
    target: agentRunTarget,
    required: contextCoordinate,
  });
  const projection =
    state.status === "ready" || state.status === "refreshing"
      ? state.projection
      : state.status === "error"
        ? state.previous
        : null;
  const isLoading = state.status === "loading" || state.status === "refreshing";
  const error = state.status === "error" ? state.error.message : null;

  return (
    <SessionProjectionViewPanel
      projection={projection}
      agentRunTarget={agentRunTarget}
      tokenUsage={tokenUsage}
      compactContextCommand={compactContextCommand}
      onCompactContext={onCompactContext}
      isLoading={isLoading}
      error={error}
      onRefresh={refresh}
      embedded={embedded}
    />
  );
}

export default SessionProjectionView;
