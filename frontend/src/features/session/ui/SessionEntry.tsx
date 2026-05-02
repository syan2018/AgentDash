/**
 * 会话条目渲染组件
 *
 * 根据 BackboneEvent 类型渲染不同的 UI：
 * - agent_message_delta → AcpMessageCard (agent)
 * - reasoning_text_delta / reasoning_summary_delta → AcpMessageCard (thinking)
 * - item_started / item_completed → AcpToolCallCard (ThreadItem)
 * - turn_plan_updated → AcpPlanCard
 * - platform:
 *   - user_message_chunk → AcpMessageCard (user)
 *   - executor_session_bound / hook_trace / task_* / companion_* 等 → 系统事件卡片
 * - approval_request → 审批卡片
 * - error → 错误卡片
 * - token_usage_updated / turn_started / turn_completed → 静默
 */

import { memo, useState } from "react";
import {
  isAggregatedGroup,
  isAggregatedThinkingGroup,
  isDisplayEntry,
  extractTextFromContentBlock,
  parseContentBlock,
} from "../model/types";
import type { AcpDisplayItem, AcpDisplayEntry, AggregatedEntryGroup, AggregatedThinkingGroup } from "../model/types";
import { AcpToolCallCard } from "./SessionToolCallCard";
import { AcpMessageCard } from "./SessionMessageCard";
import { AcpPlanCard } from "./SessionPlanCard";
import { ContentBlockCard } from "./ContentBlockCard";
import { AcpTaskContextCard } from "./SessionTaskContextCard";
import { isAgentDashTaskContextBlock } from "./SessionTaskContextGuard";
import { AcpOwnerContextCard } from "./SessionOwnerContextCard";
import { AcpSessionCapabilityCard, isSessionCapabilitiesBlock } from "./SessionCapabilityCard";
import { AcpTaskEventCard } from "./SessionTaskEventCard";
import { isTaskEventUpdate } from "./SessionTaskEventGuard";
import { AcpSystemEventCard } from "./SessionSystemEventCard";
import { isRenderableSystemEventUpdate } from "./SessionSystemEventGuard";

export interface SessionEntryProps {
  item: AcpDisplayItem;
  isStreaming?: boolean;
  sessionId?: string | null;
}

export const SessionEntry = memo(function SessionEntry({ item, isStreaming, sessionId }: SessionEntryProps) {
  if (isAggregatedGroup(item)) {
    if (item.aggregationType === "file_edit") {
      return <AggregatedDiffGroupEntry group={item} sessionId={sessionId} />;
    }
    return <AggregatedToolGroupEntry group={item} sessionId={sessionId} />;
  }

  if (isAggregatedThinkingGroup(item)) {
    return <AggregatedThinkingGroupEntry group={item} />;
  }

  if (isDisplayEntry(item)) {
    return <SingleEntry entry={item} isStreaming={!!isStreaming} sessionId={sessionId} />;
  }

  return null;
});

function SingleEntry({
  entry,
  isStreaming = false,
  sessionId,
}: {
  entry: AcpDisplayEntry;
  isStreaming?: boolean;
  sessionId?: string | null;
}) {
  const { event, isPendingApproval, accumulatedText } = entry;

  switch (event.type) {
    case "agent_message_delta": {
      return (
        <AcpMessageCard
          type="agent"
          content={accumulatedText ?? event.payload.delta}
          isStreaming={isStreaming}
        />
      );
    }

    case "reasoning_text_delta":
    case "reasoning_summary_delta": {
      return (
        <AcpMessageCard
          type="thinking"
          content={accumulatedText ?? event.payload.delta}
        />
      );
    }

    case "item_started":
    case "item_completed": {
      return (
        <AcpToolCallCard
          item={event.payload.item}
          isPendingApproval={isPendingApproval}
          sessionId={sessionId ?? undefined}
          outputText={accumulatedText}
        />
      );
    }

    case "turn_plan_updated": {
      return <AcpPlanCard steps={event.payload.plan} />;
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
                return <AcpTaskContextCard block={block} />;
              }

              const uri = block.resource.uri;
              if (
                uri.startsWith("agentdash://project-context/") ||
                uri.startsWith("agentdash://story-context/")
              ) {
                return <AcpOwnerContextCard block={block} />;
              }

              if (isSessionCapabilitiesBlock(block)) {
                return <AcpSessionCapabilityCard block={block} />;
              }
            }
            return <ContentBlockCard block={block} variant="compact" />;
          }

          if (block.type === "image" || block.type === "audio") {
            return <ContentBlockCard block={block} variant="compact" />;
          }
        }

        return (
          <AcpMessageCard
            type="user"
            content={accumulatedText ?? extractTextFromContentBlock(block)}
          />
        );
      }

      if (isTaskEventUpdate(event)) {
        return <AcpTaskEventCard event={event} />;
      }

      if (isRenderableSystemEventUpdate(event)) {
        return <AcpSystemEventCard event={event} sessionId={sessionId ?? undefined} />;
      }

      return null;
    }

    default:
      return null;
  }
}

function AggregatedToolGroupEntry({
  group,
  sessionId,
}: {
  group: AggregatedEntryGroup;
  sessionId?: string | null;
}) {
  const [expanded, setExpanded] = useState(false);
  const { aggregationType, entries } = group;
  const badge = getAggregationBadgeConfig(aggregationType);
  const summary = buildKindSummary(entries);

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-3 px-3 py-2.5 text-left transition-colors hover:bg-secondary/35"
      >
        <span className="inline-flex min-w-10 shrink-0 items-center justify-center rounded-[8px] border border-border bg-secondary px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          {badge.token}
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-foreground">{badge.label}</p>
          <p className="text-xs text-muted-foreground">{summary}</p>
        </div>
        <span className="text-xs text-muted-foreground/70">{expanded ? "收起" : "展开"}</span>
      </button>
      {expanded && (
        <div className="space-y-1.5 border-t border-border px-3 py-2.5">
          {entries.map((entry) => {
            const item = extractThreadItem(entry);
            if (!item) return null;
            return (
              <AcpToolCallCard
                key={entry.id}
                item={item}
                isPendingApproval={entry.isPendingApproval}
                compact
                sessionId={sessionId ?? undefined}
              />
            );
          })}
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

function AggregatedDiffGroupEntry({
  group,
  sessionId,
}: {
  group: AggregatedEntryGroup;
  sessionId?: string | null;
}) {
  const filePath = group.filePath ?? "未知文件";
  const { entries } = group;

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      <div className="flex items-center gap-2.5 px-3 py-2.5 text-sm border-b border-border">
        <span className="inline-flex rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          EDIT
        </span>
        <span className="font-mono text-xs">{filePath}</span>
        <span className="ml-auto text-xs text-muted-foreground tabular-nums">
          {entries.length} 次编辑
        </span>
      </div>
      <div className="space-y-1.5 px-3 py-2.5">
        {entries.map((entry) => {
          const item = extractThreadItem(entry);
          if (!item) return null;
          return (
            <AcpToolCallCard
              key={entry.id}
              item={item}
              isPendingApproval={entry.isPendingApproval}
              compact
              sessionId={sessionId ?? undefined}
            />
          );
        })}
      </div>
    </div>
  );
}

function extractThreadItem(entry: AcpDisplayEntry): import("../../../generated/backbone-protocol").ThreadItem | null {
  const evt = entry.event;
  if (evt.type === "item_started" || evt.type === "item_completed") {
    return evt.payload.item;
  }
  return null;
}

function getAggregationBadgeConfig(aggregationType: AggregatedEntryGroup["aggregationType"]): {
  token: string;
  label: string;
} {
  switch (aggregationType) {
    case "info_gather":
      return { token: "INFO", label: "信息获取" };
    case "file_read":
      return { token: "READ", label: "读取文件" };
    case "search":
      return { token: "FIND", label: "搜索文件" };
    case "web_fetch":
      return { token: "FETCH", label: "获取网页" };
    case "command_run_read":
      return { token: "READ", label: "读取命令结果" };
    case "command_run_search":
      return { token: "FIND", label: "搜索命令结果" };
    case "command_run_edit":
      return { token: "EDIT", label: "命令写入" };
    case "command_run_fetch":
      return { token: "FETCH", label: "命令获取" };
    case "file_edit":
      return { token: "EDIT", label: "文件编辑" };
    default:
      return { token: "TOOL", label: "工具调用" };
  }
}

function buildKindSummary(entries: AggregatedEntryGroup["entries"]): string {
  const kindLabels: Record<string, string> = {
    commandExecution: "命令执行",
    fileChange: "文件编辑",
    mcpToolCall: "MCP 工具",
    dynamicToolCall: "工具调用",
    webSearch: "搜索",
  };

  const counts = new Map<string, number>();
  for (const entry of entries) {
    const item = extractThreadItem(entry);
    const kind = item?.type ?? "other";
    counts.set(kind, (counts.get(kind) ?? 0) + 1);
  }

  const parts: string[] = [];
  for (const [kind, count] of counts) {
    const label = kindLabels[kind] ?? "工具调用";
    parts.push(`${count} 次${label}`);
  }

  return parts.join(" · ");
}

export default SessionEntry;
