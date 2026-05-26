/**
 * 会话条目渲染组件
 *
 * 根据 BackboneEvent 类型渲染不同的 UI：
 * - agent_message_delta → SessionMessageCard (agent)
 * - reasoning_text_delta / reasoning_summary_delta → SessionMessageCard (thinking)
 * - item_started / item_completed → SessionToolCallCard (ThreadItem)
 * - turn_plan_updated → SessionPlanCard
 * - platform:
 *   - user_message_chunk → SessionMessageCard (user)
 *   - executor_session_bound / hook_trace / task_* / companion_* 等 → 系统事件卡片
 * - approval_request → 审批卡片
 * - error → 错误卡片
 * - token_usage_updated / turn_started / turn_completed → 静默
 */

import { memo, useState } from "react";
import { ContextFrameStream } from "./ContextFrameStream";
import {
  isAggregatedGroup,
  isAggregatedContextFrameGroup,
  isAggregatedThinkingGroup,
  isDisplayEntry,
  extractTextFromContentBlock,
  getThreadItemStatus,
  parseContentBlock,
} from "../model/types";
import { resolveKind, KIND_REGISTRY, type ThreadItemKind } from "../model/threadItemKind";
import type {
  SessionDisplayItem,
  SessionDisplayEntry,
  AggregatedContextFrameGroup,
  AggregatedEntryGroup,
  AggregatedThinkingGroup,
} from "../model/types";
import { extractPlatformEventData } from "../model/platformEvent";
import { parseContextFrame } from "../model/contextFrame";
import { SessionToolCallCard } from "./SessionToolCallCard";
import { CommandExecutionCard } from "./CommandExecutionCard";
import { SessionMessageCard } from "./SessionMessageCard";
import { SessionPlanCard } from "./SessionPlanCard";
import { ContentBlockCard } from "./ContentBlockCard";
import { SessionTaskContextCard } from "./SessionTaskContextCard";
import { isAgentDashTaskContextBlock } from "./SessionTaskContextGuard";
import { SessionOwnerContextCard } from "./SessionOwnerContextCard";
import { SessionCapabilityCard, isSessionCapabilitiesBlock } from "./SessionCapabilityCard";
import { SessionTaskEventCard } from "./SessionTaskEventCard";
import { isTaskEventUpdate } from "./SessionTaskEventGuard";
import { SessionSystemEventCard } from "./SessionSystemEventCard";
import { isRenderableSystemEventUpdate } from "./SessionSystemEventGuard";

export interface SessionEntryProps {
  item: SessionDisplayItem;
  isStreaming?: boolean;
  sessionId?: string | null;
}

export const SessionEntry = memo(function SessionEntry({ item, isStreaming, sessionId }: SessionEntryProps) {
  if (isAggregatedGroup(item)) {
    return <AggregatedToolGroupEntry group={item} sessionId={sessionId} />;
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
      if (threadItem.type === "commandExecution") {
        return (
          <CommandExecutionCard
            item={threadItem}
            sessionId={sessionId ?? undefined}
            outputText={accumulatedText}
          />
        );
      }
      if (threadItem.type === "contextCompaction") {
        return (
          <SessionToolCallCard
            item={threadItem}
            statusOverride={event.type === "item_started" ? "inProgress" : "completed"}
            kindOverride="context"
            titleOverride="上下文压缩"
          />
        );
      }
      return (
        <SessionToolCallCard
          item={threadItem}
          isPendingApproval={isPendingApproval}
          sessionId={sessionId ?? undefined}
          outputText={accumulatedText}
        />
      );
    }

    case "turn_plan_updated": {
      return <SessionPlanCard steps={event.payload.plan} />;
    }

    case "approval_request": {
      return (
        <div className="rounded-[12px] border border-warning/30 bg-warning/5 px-3 py-2.5 text-sm text-warning">
          <span className="inline-flex rounded-[6px] border border-warning/25 bg-warning/10 px-1.5 py-0.5 text-[10px] font-semibold tracking-[0.1em]">
            审批
          </span>
          <span className="ml-2">等待审批</span>
        </div>
      );
    }

    case "error": {
      return (
        <div className="rounded-[12px] border border-destructive/30 bg-destructive/5 px-3 py-2.5 text-sm">
          <span className="inline-flex rounded-[6px] border border-destructive/25 bg-destructive/10 px-1.5 py-0.5 text-[10px] font-semibold tracking-[0.1em] text-destructive">
            错误
          </span>
          <span className="ml-2 text-destructive">{event.payload.error.message}</span>
        </div>
      );
    }

    case "platform": {
      const platform = event.payload;

      if (platform.kind === "session_meta_update" && platform.data.key === "user_message_chunk") {
        const block = parseContentBlock(platform.data.value);

        if (block) {
          if (block.type === "resource" || block.type === "resource_link") {
            if (block.type === "resource") {
              if (isAgentDashTaskContextBlock(block)) {
                return <SessionTaskContextCard block={block} />;
              }

              const uri = block.resource.uri;
              if (
                uri.startsWith("agentdash://project-context/") ||
                uri.startsWith("agentdash://story-context/")
              ) {
                return <SessionOwnerContextCard block={block} />;
              }

              if (isSessionCapabilitiesBlock(block)) {
                return <SessionCapabilityCard block={block} />;
              }
            }
            return <ContentBlockCard block={block} variant="compact" />;
          }

          if (block.type === "image" || block.type === "audio") {
            return <ContentBlockCard block={block} variant="compact" />;
          }
        }

        return (
          <SessionMessageCard
            type="user"
            content={accumulatedText ?? extractTextFromContentBlock(block)}
          />
        );
      }

      if (isTaskEventUpdate(event)) {
        return <SessionTaskEventCard event={event} />;
      }

      if (isRenderableSystemEventUpdate(event)) {
        return <SessionSystemEventCard event={event} sessionId={sessionId ?? undefined} />;
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
    .map((entry) => extractPlatformEventData(entry.event))
    .filter((data): data is Record<string, unknown> => data != null)
    .map((data) => parseContextFrame(data))
    .filter((frame): frame is NonNullable<ReturnType<typeof parseContextFrame>> => frame != null);

  if (frames.length === 0) {
    return null;
  }

  return <ContextFrameStream frames={frames} />;
}

function AggregatedToolGroupEntry({
  group,
  sessionId,
}: {
  group: AggregatedEntryGroup;
  sessionId?: string | null;
}) {
  const { entries } = group;
  const hasPendingApproval = entries.some((e) => e.isPendingApproval);
  const [expanded, setExpanded] = useState(hasPendingApproval);
  const [prevPending, setPrevPending] = useState(hasPendingApproval);
  if (hasPendingApproval !== prevPending) {
    setPrevPending(hasPendingApproval);
    if (hasPendingApproval) setExpanded(true);
  }
  const summary = buildKindSummary(entries);

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-3 px-3 py-2.5 text-left transition-colors hover:bg-secondary/35"
      >
        <span className="inline-flex min-w-10 shrink-0 items-center justify-center rounded-[8px] border border-border bg-secondary px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          TOOLS
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-foreground">工具调用</p>
          <p className="text-xs text-muted-foreground">{summary}</p>
        </div>
        <span className="text-xs text-muted-foreground/70">{expanded ? "收起" : "展开"}</span>
      </button>
      {expanded && (
        <div className="space-y-1.5 border-t border-border px-3 py-2.5">
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
    <div className="overflow-hidden rounded-[12px] border border-dashed border-border bg-secondary/45">
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center justify-between gap-3 px-3 py-2.5 text-left transition-colors hover:bg-secondary/60"
      >
        <div className="flex min-w-0 items-center gap-3">
          <span className="inline-flex min-w-10 shrink-0 items-center justify-center rounded-[8px] border border-border bg-background px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            THINK
          </span>
          <div className="min-w-0">
            <p className="text-sm font-medium text-foreground">思考摘录</p>
            <p className="text-xs text-muted-foreground">{entries.length} 条思考已折叠聚合</p>
          </div>
        </div>
        <span className="text-xs text-muted-foreground/70">{expanded ? "收起" : "展开"}</span>
      </button>
      {expanded && (
        <div className="border-t border-border/80 px-3 py-2.5">
          <pre className="whitespace-pre-wrap font-mono text-xs leading-relaxed text-muted-foreground/85">
            {combinedContent}
          </pre>
        </div>
      )}
    </div>
  );
}

function extractThreadItem(entry: SessionDisplayEntry): import("../../../generated/backbone-protocol").ThreadItem | null {
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
