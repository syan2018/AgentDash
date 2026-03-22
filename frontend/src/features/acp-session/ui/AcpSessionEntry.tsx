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

import { useState } from "react";
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
import { AcpTaskEventCard } from "./AcpTaskEventCard";
import { isTaskEventUpdate } from "./AcpTaskEventGuard";
import { AcpSystemEventCard } from "./AcpSystemEventCard";
import { isRenderableSystemEventUpdate } from "./AcpSystemEventGuard";

export interface AcpSessionEntryProps {
  item: AcpDisplayItem;
  streamingEntryId?: string | null;
  sessionId?: string | null;
}

export function AcpSessionEntry({ item, streamingEntryId, sessionId }: AcpSessionEntryProps) {
  if (isAggregatedGroup(item)) {
    if (item.aggregationType === "file_edit") {
      return <AggregatedDiffGroupEntry group={item} sessionId={sessionId} />;
    }
    return <AggregatedToolGroupEntry group={item} />;
  }

  if (isAggregatedThinkingGroup(item)) {
    return <AggregatedThinkingGroupEntry group={item} />;
  }

  if (isDisplayEntry(item)) {
    return <SingleEntry entry={item} isStreaming={item.id === streamingEntryId} sessionId={sessionId} />;
  }

  return null;
}

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
        const contextCard = isAgentDashTaskContextBlock(content)
          ? <AcpTaskContextCard block={content} />
          : <ContentBlockCard block={content} variant="compact" />;
        return contextCard;
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
        return <AcpSystemEventCard update={update} />;
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

function AggregatedToolGroupEntry({ group }: { group: AggregatedEntryGroup }) {
  const [expanded, setExpanded] = useState(false);
  const { aggregationType, entries } = group;
  const badge = getAggregationBadgeConfig(aggregationType);

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
          <p className="text-xs text-muted-foreground">
            {entries.length} 次同类工具操作已聚合
          </p>
        </div>
        <span className="text-xs text-muted-foreground/70">{expanded ? "收起" : "展开"}</span>
      </button>
      {expanded && (
        <div className="space-y-1.5 border-t border-border px-3 py-2.5">
          {entries.map((entry) => (
            <div
              key={entry.id}
              className="rounded-[10px] border border-border/70 bg-secondary/25 px-3 py-2"
            >
              <p className="truncate text-sm text-foreground/90">
                {resolveAggregatedToolTitle(entry)}
              </p>
              <p className="mt-1 text-[11px] text-muted-foreground">
                {entry.update.sessionUpdate === "tool_call_update" ? "工具更新" : "工具调用"}
              </p>
            </div>
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

function resolveAggregatedToolTitle(entry: AggregatedEntryGroup["entries"][number]): string {
  if (entry.update.sessionUpdate === "tool_call") {
    return entry.update.title;
  }
  if (entry.update.sessionUpdate === "tool_call_update") {
    return entry.update.title ?? "工具更新";
  }
  return "工具更新";
}

export default AcpSessionEntry;
