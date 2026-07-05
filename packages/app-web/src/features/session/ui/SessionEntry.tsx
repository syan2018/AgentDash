/**
 * 会话条目渲染组件
 *
 * 根据 BackboneEvent 类型渲染不同的 UI：
 * - agent_message_delta → SessionMessageCard (agent)
 * - reasoning_text_delta / reasoning_summary_delta → SessionMessageCard (thinking)
 * - item_started / item_updated / item_completed → ToolCallCardShell + toolCardRegistry (AgentDashThreadItem)
 * - turn_plan_updated → SessionPlanCard
 * - platform:
 *   - executor_session_bound / hook_trace / task_* / companion_* 等 → 系统事件卡片
 * - approval_request → 审批卡片
 * - error → 错误卡片
 * - token_usage_updated / turn_started / turn_completed → 静默
 */

import { memo, useState } from "react";
import { ST } from "./bodies/cardBodyTokens";
import { ContextFrameStream } from "./ContextFrameStream";
import {
  isAggregatedGroup,
  isAggregatedContextFrameGroup,
  isAggregatedThinkingGroup,
  isDisplayEntry,
  partitionUserInputs,
  getThreadItemStatus,
} from "../model/types";
import { resolveKind, KIND_REGISTRY, type ThreadItemKind } from "../model/threadItemKind";
import type {
  SessionDisplayItem,
  SessionDisplayEntry,
  AggregatedContextFrameGroup,
  AggregatedEntryGroup,
  AggregatedThinkingGroup,
} from "../model/types";
import { ToolCallCardShell } from "./ToolCallCardShell";
import { renderToolCallCard } from "./toolCardRegistry";
import { SessionMessageCard } from "./SessionMessageCard";
import { SessionPlanCard } from "./SessionPlanCard";
import { SessionTaskEventCard } from "./SessionTaskEventCard";
import { isTaskEventUpdate } from "./SessionTaskEventGuard";
import { SessionSystemEventCard } from "./SessionSystemEventCard";
import { isRenderableSystemEventUpdate } from "./SessionSystemEventGuard";
import { useDebugPrefs } from "../../../hooks/use-debug-prefs";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import type { CodexErrorInfo, ErrorNotification } from "../../../generated/backbone-protocol";

export interface SessionEntryProps {
  item: SessionDisplayItem;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  isStreaming?: boolean;
  sessionId?: string | null;
  /** 该条目后面是否紧跟 agent message（用于 tool group 自动折叠） */
  followedByMessage?: boolean;
}

export const SessionEntry = memo(function SessionEntry({
  item,
  agentRunTarget,
  isStreaming,
  sessionId,
  followedByMessage,
}: SessionEntryProps) {
  if (isAggregatedGroup(item)) {
    return (
      <AggregatedToolGroupEntry
        group={item}
        agentRunTarget={agentRunTarget}
        sessionId={sessionId}
        followedByMessage={followedByMessage}
      />
    );
  }

  if (isAggregatedThinkingGroup(item)) {
    return <AggregatedThinkingGroupEntry group={item} />;
  }

  if (isAggregatedContextFrameGroup(item)) {
    return <AggregatedContextFrameGroupEntry group={item} />;
  }

  if (isDisplayEntry(item)) {
    return (
      <SingleEntry
        entry={item}
        agentRunTarget={agentRunTarget}
        isStreaming={!!isStreaming}
        sessionId={sessionId}
      />
    );
  }

  return null;
});

export function SingleEntry({
  entry,
  agentRunTarget,
  isStreaming = false,
  sessionId,
}: {
  entry: SessionDisplayEntry;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  isStreaming?: boolean;
  sessionId?: string | null;
}) {
  const { event, isPendingApproval, accumulatedText } = entry;
  const { prefs } = useDebugPrefs();

  switch (event.type) {
    case "agent_message_delta": {
      return (
        <SessionMessageCard
          type="agent"
          content={accumulatedText ?? event.payload.delta}
          isStreaming={isStreaming}
        />
      );
    }

    case "reasoning_text_delta":
    case "reasoning_summary_delta": {
      return (
        <SessionMessageCard
          type="thinking"
          content={accumulatedText ?? event.payload.delta}
        />
      );
    }

    case "item_started":
    case "item_updated":
    case "item_completed": {
      const threadItem = event.payload.item;
      const card = renderToolCallCard(threadItem, {
        sessionId: sessionId ?? undefined,
        outputText: accumulatedText,
      });
      return (
        <ToolCallCardShell
          kind={card.kind}
          header={card.header}
          status={card.status}
          isPendingApproval={isPendingApproval}
          agentRunTarget={agentRunTarget}
          itemId={threadItem.id}
          durationMs={card.durationMs}
        >
          {card.body}
        </ToolCallCardShell>
      );
    }

    case "turn_plan_updated": {
      return <SessionPlanCard steps={event.payload.plan} />;
    }

    case "approval_request": {
      return (
        <div className="flex items-center gap-2 px-2 py-1 text-xs text-warning">
          <span className="inline-flex rounded-[4px] border border-warning/25 bg-warning/10 px-1 py-px text-[9px] font-semibold tracking-[0.08em]">
            审批
          </span>
          <span>等待审批</span>
        </div>
      );
    }

    case "error": {
      return <SessionErrorCard notification={event.payload} />;
    }

    case "user_input_submitted": {
      const { text, images } = partitionUserInputs(event.payload.content);
      return <SessionMessageCard type="user" content={text} images={images} />;
    }

    case "platform": {
      if (isTaskEventUpdate(event)) {
        return <SessionTaskEventCard event={event} />;
      }

      if (isRenderableSystemEventUpdate(event, { includeVerboseEvents: prefs.hookVerbose })) {
        return (
          <SessionSystemEventCard
            event={event}
            sessionId={sessionId ?? undefined}
            contextFrame={entry.contextFrame}
          />
        );
      }

      return null;
    }

    default:
      return null;
  }
}

function SessionErrorCard({ notification }: { notification: ErrorNotification }) {
  const { error } = notification;
  const errorInfo = formatCodexErrorInfo(error.codexErrorInfo);
  const details = error.additionalDetails?.trim();

  return (
    <div className="rounded-[8px] border border-destructive/30 bg-destructive/10 px-3 py-2.5 text-sm text-destructive">
      <div className="flex flex-wrap items-center gap-2">
        <span className="inline-flex rounded-[4px] border border-destructive/25 bg-background/60 px-1.5 py-px text-[10px] font-semibold">
          ERROR
        </span>
        <span className="font-medium">
          {notification.willRetry ? "执行错误，等待重试" : "执行失败"}
        </span>
        {errorInfo && (
          <span className="font-mono text-[11px] text-destructive/80">{errorInfo}</span>
        )}
      </div>

      <pre className="mt-2 whitespace-pre-wrap wrap-anywhere font-sans text-sm leading-6 text-foreground">
        {error.message}
      </pre>

      {details && (
        <details className="mt-2 text-xs text-destructive/80">
          <summary className="cursor-pointer select-none">错误详情</summary>
          <pre className="mt-1 whitespace-pre-wrap wrap-anywhere rounded-[6px] bg-background/60 px-2 py-1.5 font-mono text-[11px] leading-5 text-foreground/80">
            {details}
          </pre>
        </details>
      )}

      <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 font-mono text-[11px] text-destructive/70">
        <span>turn {notification.turnId}</span>
        <span>thread {notification.threadId}</span>
      </div>
    </div>
  );
}

function formatCodexErrorInfo(info: CodexErrorInfo | null): string | null {
  if (info == null) return null;
  if (typeof info === "string") return info;
  if ("httpConnectionFailed" in info) {
    return formatHttpErrorInfo("http_connection_failed", info.httpConnectionFailed.httpStatusCode);
  }
  if ("responseStreamConnectionFailed" in info) {
    return formatHttpErrorInfo(
      "response_stream_connection_failed",
      info.responseStreamConnectionFailed.httpStatusCode,
    );
  }
  if ("responseStreamDisconnected" in info) {
    return formatHttpErrorInfo(
      "response_stream_disconnected",
      info.responseStreamDisconnected.httpStatusCode,
    );
  }
  if ("responseTooManyFailedAttempts" in info) {
    return formatHttpErrorInfo(
      "response_too_many_failed_attempts",
      info.responseTooManyFailedAttempts.httpStatusCode,
    );
  }
  if ("activeTurnNotSteerable" in info) {
    return `active_turn_not_steerable:${info.activeTurnNotSteerable.turnKind}`;
  }
  return null;
}

function formatHttpErrorInfo(kind: string, httpStatusCode: number | null): string {
  return httpStatusCode == null ? kind : `${kind}:HTTP ${httpStatusCode}`;
}

function AggregatedContextFrameGroupEntry({
  group,
}: {
  group: AggregatedContextFrameGroup;
}) {
  const frames = group.entries
    .map((entry) => entry.contextFrame)
    .filter((frame): frame is NonNullable<typeof frame> => frame != null);

  if (frames.length === 0) {
    return null;
  }

  return <ContextFrameStream frames={frames} />;
}

function AggregatedToolGroupEntry({
  group,
  agentRunTarget,
  sessionId,
  followedByMessage = false,
}: {
  group: AggregatedEntryGroup;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  sessionId?: string | null;
  /** 后续有 agent message 时自动折叠 */
  followedByMessage?: boolean;
}) {
  const { entries } = group;
  const hasPendingApproval = entries.some((e) => e.isPendingApproval);
  const hasRunningTool = entries.some((entry) => {
    const item = extractThreadItem(entry);
    return item ? getThreadItemStatus(item) === "inProgress" : false;
  });
  // 默认展开；有后续 agent 消息后才折叠
  const [expanded, setExpanded] = useState(!followedByMessage || hasPendingApproval || hasRunningTool);
  const [prevFollowed, setPrevFollowed] = useState(followedByMessage);
  const [prevPending, setPrevPending] = useState(hasPendingApproval);
  const [prevRunning, setPrevRunning] = useState(hasRunningTool);

  if (hasPendingApproval !== prevPending) {
    setPrevPending(hasPendingApproval);
    if (hasPendingApproval) setExpanded(true);
  }
  if (hasRunningTool !== prevRunning) {
    setPrevRunning(hasRunningTool);
    if (hasRunningTool) setExpanded(true);
  }
  if (followedByMessage !== prevFollowed) {
    setPrevFollowed(followedByMessage);
    if (followedByMessage && !hasPendingApproval && !hasRunningTool) setExpanded(false);
  }
  const summary = buildKindSummary(entries);

  return (
    <div>
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className={ST.groupRow}
      >
        <span className={ST.chevron}>{expanded ? "▼" : "▶"}</span>
        <span className={ST.badge}>TOOLS</span>
        <span className={ST.hint}>{summary}</span>
      </button>
      {expanded && (
        <div className={ST.itemList}>
          {entries.map((entry) => (
            <SingleEntry
              key={entry.id}
              entry={entry}
              agentRunTarget={agentRunTarget}
              sessionId={sessionId}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function AggregatedThinkingGroupEntry({ group }: { group: AggregatedThinkingGroup }) {
  const combinedContent = group.entries
    .map((entry) => entry.accumulatedText ?? "")
    .join("");

  return (
    <SessionMessageCard
      type="thinking"
      content={combinedContent}
      isStreaming={group.isStreamingThinking}
    />
  );
}

function extractThreadItem(entry: SessionDisplayEntry): import("../../../generated/backbone-protocol").AgentDashThreadItem | null {
  const evt = entry.event;
  if (evt.type === "item_started" || evt.type === "item_updated" || evt.type === "item_completed") {
    return evt.payload.item;
  }
  return null;
}

function buildKindSummary(entries: AggregatedEntryGroup["entries"]): string {
  const counts = new Map<ThreadItemKind, number>();
  let pending = 0;
  let running = 0;
  let failed = 0;

  for (const entry of entries) {
    if (entry.isPendingApproval) pending += 1;
    const item = extractThreadItem(entry);
    if (!item) {
      counts.set("other", (counts.get("other") ?? 0) + 1);
      continue;
    }
    const status = getThreadItemStatus(item);
    if (status === "inProgress") running += 1;
    if (status === "failed") failed += 1;
    const meta = resolveKind(item);
    counts.set(meta.kind, (counts.get(meta.kind) ?? 0) + 1);
  }

  const parts: string[] = [];
  // 按 KIND_REGISTRY 声明顺序生成摘要，保证稳定
  for (const kind of Object.keys(KIND_REGISTRY) as ThreadItemKind[]) {
    const n = counts.get(kind);
    if (!n) continue;
    const meta = KIND_REGISTRY[kind];
    parts.push(`${meta.summaryVerb} ${n} ${meta.summaryUnit}`);
  }
  if (running > 0) parts.push(`${running} 运行中`);
  if (pending > 0) parts.push(`${pending} 待审批`);
  if (failed > 0) parts.push(`${failed} 失败`);

  return parts.join(" · ");
}

export default SessionEntry;
