/**
 * ACP 会话条目渲染组件
 *
 * 根据条目类型渲染不同的 UI。
 * 对照 Zed 实现，覆盖所有 ACP SessionUpdate 类型：
 * - user_message_chunk / agent_message_chunk / agent_thought_chunk → AcpMessageCard
 *   - 其中 agentdash://task-context/* 资源块 → AcpTaskContextCard（Task 专属）
 * - tool_call / tool_call_update → AcpToolCallCard
 * - plan → AcpPlanCard
 * - session_info_update
 *   - task_* 事件 → AcpTaskEventCard（Task 专属）
 *   - 关键 system / hook / companion 事件 → AcpSystemEventCard
 *   - 其他噪音事件保持静默
 * - usage_update / available_commands_update / current_mode_update / config_option_update → 静默
 *
 * 说明：
 * - 上下文用量已在 Task 会话面板的进度区做汇总展示，此处不重复渲染 usage card
 * - 系统信息后续计划收敛到 inbox 场景，常规会话流先保持静默
 */

import { memo, useState } from "react";
import {
  isAggregatedGroup,
  isAggregatedThinkingGroup,
  isDisplayEntry,
  extractTextFromContentBlock,
} from "../model/types";
import type { AcpDisplayItem, AcpDisplayEntry, AggregatedEntryGroup, AggregatedThinkingGroup, ContentBlock } from "../model/types";
import { AcpToolCallCard } from "./AcpToolCallCard";
import { AcpMessageCard } from "./AcpMessageCard";
import { AcpPlanCard } from "./AcpPlanCard";
import { ContentBlockCard } from "./ContentBlockCard";
import { AcpTaskContextCard } from "./AcpTaskContextCard";
import { isAgentDashTaskContextBlock } from "./AcpTaskContextGuard";
import { AcpOwnerContextCard } from "./AcpOwnerContextCard";
import { AcpSessionCapabilityCard, isSessionCapabilitiesBlock } from "./AcpSessionCapabilityCard";
import { AcpTaskEventCard } from "./AcpTaskEventCard";
import { isTaskEventUpdate } from "./AcpTaskEventGuard";
import { AcpSystemEventCard } from "./AcpSystemEventCard";
import { isRenderableSystemEventUpdate } from "./AcpSystemEventGuard";

export interface AcpSessionEntryProps {
  item: AcpDisplayItem;
  isStreaming?: boolean;
  sessionId?: string | null;
}

export const AcpSessionEntry = memo(function AcpSessionEntry({ item, isStreaming, sessionId }: AcpSessionEntryProps) {
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
  const { update, isPendingApproval } = entry;

  switch (update.sessionUpdate) {
    case "user_message_chunk": {
      const content = update.content as ContentBlock | undefined;

      // 对于 resource/resource_link 类型，使用优雅的卡片展示
      if (content?.type === "resource" || content?.type === "resource_link") {
        if (isAgentDashTaskContextBlock(content)) {
          return <AcpTaskContextCard block={content} />;
        }
        const uri = content.type === "resource" ? content.resource?.uri : content.uri;
        if (typeof uri === "string" && (
          uri.startsWith("agentdash://project-context/") ||
          uri.startsWith("agentdash://story-context/")
        )) {
          return <AcpOwnerContextCard block={content} />;
        }
        if (isSessionCapabilitiesBlock(content)) {
          return <AcpSessionCapabilityCard block={content} />;
        }
        return <ContentBlockCard block={content} variant="compact" />;
      }

      const text = extractTextFromContentBlock(content);
      return (
        <AcpMessageCard
          type="user"
          content={text}
        />
      );
    }

    case "agent_message_chunk": {
      const text = extractTextFromContentBlock(update.content);
      return (
        <AcpMessageCard
          type="agent"
          content={text}
          isStreaming={isStreaming}
        />
      );
    }

    case "agent_thought_chunk": {
      const text = extractTextFromContentBlock(update.content);
      return (
        <AcpMessageCard
          type="thinking"
          content={text}
        />
      );
    }

    case "tool_call":
    case "tool_call_update":
      return (
        <AcpToolCallCard
          update={update}
          isPendingApproval={isPendingApproval}
          sessionId={sessionId ?? undefined}
        />
      );

    case "plan":
      return <AcpPlanCard entries={update.entries} />;

    case "session_info_update":
      if (isTaskEventUpdate(update)) {
        return <AcpTaskEventCard update={update} />;
      }
      if (isRenderableSystemEventUpdate(update)) {
        return <AcpSystemEventCard update={update} sessionId={sessionId ?? undefined} />;
      }
      return null;

    case "usage_update":
    case "available_commands_update":
    case "current_mode_update":
    case "config_option_update":
      return null;

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
          {entries.map((entry) => (
            <AcpToolCallCard
              key={entry.id}
              update={entry.update}
              isPendingApproval={entry.isPendingApproval}
              compact
              sessionId={sessionId ?? undefined}
            />
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
    .map((entry) => {
      if (entry.update.sessionUpdate === "agent_thought_chunk") {
        return extractTextFromContentBlock(entry.update.content);
      }
      return "";
    })
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
        {entries.map((entry) => (
          <AcpToolCallCard
            key={entry.id}
            update={entry.update}
            isPendingApproval={entry.isPendingApproval}
            compact
            sessionId={sessionId ?? undefined}
          />
        ))}
      </div>
    </div>
  );
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

/** 根据条目的 kind 生成分类摘要，如 "12 次读取 · 4 次搜索 · 2 次网页获取" */
function buildKindSummary(entries: AggregatedEntryGroup["entries"]): string {
  const kindLabels: Record<string, string> = {
    read: "读取",
    search: "搜索",
    fetch: "网页获取",
    execute: "命令执行",
  };

  const counts = new Map<string, number>();
  for (const entry of entries) {
    const kind = "kind" in entry.update ? (entry.update.kind as string) ?? "other" : "other";
    counts.set(kind, (counts.get(kind) ?? 0) + 1);
  }

  const parts: string[] = [];
  for (const [kind, count] of counts) {
    const label = kindLabels[kind] ?? "工具调用";
    parts.push(`${count} 次${label}`);
  }

  return parts.join(" · ");
}

export default AcpSessionEntry;
