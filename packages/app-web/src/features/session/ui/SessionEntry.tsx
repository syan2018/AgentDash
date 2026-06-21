/**
 * 会话条目渲染组件
 *
 * 根据 BackboneEvent 类型渲染不同的 UI：
 * - agent_message_delta → SessionMessageCard (agent)
 * - reasoning_text_delta / reasoning_summary_delta → SessionMessageCard (thinking)
 * - item_started / item_completed → ToolCallCardShell + toolCardRegistry (AgentDashThreadItem)
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

export interface SessionEntryProps {
  item: SessionDisplayItem;
  isStreaming?: boolean;
  sessionId?: string | null;
  /** 该条目后面是否紧跟 agent message（用于 tool group 自动折叠） */
  followedByMessage?: boolean;
}

export const SessionEntry = memo(function SessionEntry({ item, isStreaming, sessionId, followedByMessage }: SessionEntryProps) {
  if (isAggregatedGroup(item)) {
    return <AggregatedToolGroupEntry group={item} sessionId={sessionId} followedByMessage={followedByMessage} />;
  }

  if (isAggregatedThinkingGroup(item)) {
    return <AggregatedThinkingGroupEntry group={item} />;
  }

  if (isAggregatedContextFrameGroup(item)) {
    return <AggregatedContextFrameGroupEntry group={item} />;
  }

  if (isDisplayEntry(item)) {
    return <SingleEntry entry={item} isStreaming={!!isStreaming} sessionId={sessionId} />;
  }

  return null;
});

export function SingleEntry({
  entry,
  isStreaming = false,
  sessionId,
}: {
  entry: SessionDisplayEntry;
  isStreaming?: boolean;
  sessionId?: string | null;
}) {
  const { event, isPendingApproval, accumulatedText } = entry;

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
          sessionId={sessionId ?? undefined}
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
      return (
        <div className="flex items-center gap-2 px-2 py-1 text-xs">
          <span className="inline-flex rounded-[4px] border border-destructive/25 bg-destructive/10 px-1 py-px text-[9px] font-semibold tracking-[0.08em] text-destructive">
            错误
          </span>
          <span className="text-destructive">{event.payload.error.message}</span>
        </div>
      );
    }

    case "user_input_submitted": {
      const { text, images } = partitionUserInputs(event.payload.content);
      return <SessionMessageCard type="user" content={text} images={images} />;
    }

    case "platform": {
      if (isTaskEventUpdate(event)) {
        return <SessionTaskEventCard event={event} />;
      }

      if (isRenderableSystemEventUpdate(event)) {
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
  sessionId,
  followedByMessage = false,
}: {
  group: AggregatedEntryGroup;
  sessionId?: string | null;
  /** 后续有 agent message 时自动折叠 */
  followedByMessage?: boolean;
}) {
  const { entries } = group;
  const hasPendingApproval = entries.some((e) => e.isPendingApproval);
  // 默认展开；有后续 agent 消息后才折叠
  const [expanded, setExpanded] = useState(!followedByMessage || hasPendingApproval);
  const [prevFollowed, setPrevFollowed] = useState(followedByMessage);
  const [prevPending, setPrevPending] = useState(hasPendingApproval);

  if (hasPendingApproval !== prevPending) {
    setPrevPending(hasPendingApproval);
    if (hasPendingApproval) setExpanded(true);
  }
  if (followedByMessage !== prevFollowed) {
    setPrevFollowed(followedByMessage);
    if (followedByMessage && !hasPendingApproval) setExpanded(false);
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
            <SingleEntry key={entry.id} entry={entry} sessionId={sessionId} />
          ))}
        </div>
      )}
    </div>
  );
}

function AggregatedThinkingGroupEntry({ group }: { group: AggregatedThinkingGroup }) {
  const [expanded, setExpanded] = useState(false);
  const { entries } = group;

  const combinedContent = entries
    .map((entry) => entry.accumulatedText ?? "")
    .join("");

  return (
    <div className="group">
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2 py-1 text-left text-xs text-muted-foreground/70 transition-colors hover:text-muted-foreground"
      >
        <span className="inline-block h-px flex-1 max-w-4 bg-border/60" />
        <span className="shrink-0 font-medium">思考 · {entries.length} 条</span>
        <span className="inline-block h-px flex-1 bg-border/60" />
        <span className="shrink-0 text-[10px]">{expanded ? "收起" : "展开"}</span>
      </button>
      {expanded && (
        <div className="pl-1 pt-1">
          <pre className="whitespace-pre-wrap text-xs leading-6 text-muted-foreground/75">
            {combinedContent}
          </pre>
        </div>
      )}
    </div>
  );
}

function extractThreadItem(entry: SessionDisplayEntry): import("../../../generated/backbone-protocol").AgentDashThreadItem | null {
  const evt = entry.event;
  if (evt.type === "item_started" || evt.type === "item_completed") {
    return evt.payload.item;
  }
  return null;
}

function buildKindSummary(entries: AggregatedEntryGroup["entries"]): string {
  const counts = new Map<ThreadItemKind, number>();
  let pending = 0;
  let failed = 0;

  for (const entry of entries) {
    if (entry.isPendingApproval) pending += 1;
    const item = extractThreadItem(entry);
    if (!item) {
      counts.set("other", (counts.get("other") ?? 0) + 1);
      continue;
    }
    if (getThreadItemStatus(item) === "failed") failed += 1;
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
  if (pending > 0) parts.push(`${pending} 待审批`);
  if (failed > 0) parts.push(`${failed} 失败`);

  return parts.join(" · ");
}

export default SessionEntry;
