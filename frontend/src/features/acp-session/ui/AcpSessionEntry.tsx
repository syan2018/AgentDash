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
 *   - 其他事件保持静默
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
import { AcpTaskContextCard, isAgentDashTaskContextBlock } from "./AcpTaskContextCard";
import { AcpTaskEventCard, isTaskEventUpdate } from "./AcpTaskEventCard";

export interface AcpSessionEntryProps {
  item: AcpDisplayItem;
  streamingEntryId?: string | null;
}

export function AcpSessionEntry({ item, streamingEntryId }: AcpSessionEntryProps) {
  if (isAggregatedGroup(item)) {
    if (item.aggregationType === "file_edit") {
      return <AggregatedDiffGroupEntry group={item} />;
    }
    return <AggregatedToolGroupEntry group={item} />;
  }

  if (isAggregatedThinkingGroup(item)) {
    return <AggregatedThinkingGroupEntry group={item} />;
  }

  if (isDisplayEntry(item)) {
    return <SingleEntry entry={item} isStreaming={item.id === streamingEntryId} />;
  }

  return null;
}

function SingleEntry({ entry, isStreaming = false }: { entry: AcpDisplayEntry; isStreaming?: boolean }) {
  const { update, isPendingApproval } = entry;

  switch (update.sessionUpdate) {
    case "user_message_chunk": {
      const content = update.content as ContentBlock | undefined;

      // 对于 resource/resource_link 类型，使用优雅的卡片展示
      if (content?.type === "resource" || content?.type === "resource_link") {
        const contextCard = isAgentDashTaskContextBlock(content)
          ? <AcpTaskContextCard block={content} />
          : <ContentBlockCard block={content} variant="compact" />;
        return (
          <div className="flex gap-3">
            {/* 头像/图标 */}
            <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-primary/10">
              <span className="text-xs">👤</span>
            </div>
            <div className="flex-1 min-w-0">
              <p className="mb-1 text-xs text-primary font-medium">用户</p>
              <div className="max-w-md">
                {contextCard}
              </div>
            </div>
          </div>
        );
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
        />
      );

    case "plan":
      return <AcpPlanCard entries={update.entries} />;

    case "session_info_update":
      if (isTaskEventUpdate(update)) {
        return <AcpTaskEventCard update={update} />;
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

  const getIcon = () => {
    switch (aggregationType) {
      case "file_read":
      case "command_run_read":
        return "📄";
      case "search":
      case "command_run_search":
        return "🔍";
      case "web_fetch":
      case "command_run_fetch":
        return "🌐";
      case "command_run_edit":
        return "✏️";
      default:
        return "🔧";
    }
  };

  const getLabel = () => {
    switch (aggregationType) {
      case "file_read":
        return "读取文件";
      case "search":
        return "搜索";
      case "web_fetch":
        return "获取网页";
      case "command_run_read":
        return "读取命令";
      case "command_run_search":
        return "搜索命令";
      case "command_run_edit":
        return "编辑命令";
      case "command_run_fetch":
        return "获取命令";
      default:
        return "工具调用";
    }
  };

  return (
    <div className="rounded-lg border border-border bg-card/50 overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2 px-3 py-2.5 text-sm text-muted-foreground hover:bg-muted/30 transition-colors"
      >
        <span>{getIcon()}</span>
        <span>{getLabel()}</span>
        <span className="ml-auto text-xs tabular-nums">{entries.length} 个</span>
        <span className="text-xs">{expanded ? "▲" : "▼"}</span>
      </button>
      {expanded && (
        <div className="border-t border-border px-3 py-2 space-y-1.5">
          {entries.map((entry) => (
            <div key={entry.id} className="text-xs text-muted-foreground">
              {entry.update.sessionUpdate === "tool_call" && (
                <span>{entry.update.title}</span>
              )}
              {entry.update.sessionUpdate === "tool_call_update" && (
                <span>{entry.update.title ?? "更新"}</span>
              )}
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
    <div className="rounded-lg border border-border/50 bg-muted/20 overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center justify-between px-3 py-2.5 text-sm text-muted-foreground hover:bg-muted/30 transition-colors"
      >
        <span className="flex items-center gap-2">
          <span className="text-xs opacity-70">🧠</span>
          <span className="text-xs">思考过程 ({entries.length} 条)</span>
        </span>
        <span className="text-xs">{expanded ? "收起" : "展开"}</span>
      </button>
      {expanded && (
        <div className="border-t border-border/50 px-3 py-2.5">
          <pre className="whitespace-pre-wrap font-mono text-xs text-muted-foreground/80 leading-relaxed">
            {combinedContent}
          </pre>
        </div>
      )}
    </div>
  );
}

function AggregatedDiffGroupEntry({ group }: { group: AggregatedEntryGroup }) {
  const filePath = group.filePath ?? "未知文件";
  const { entries } = group;

  return (
    <div className="rounded-lg border border-border bg-card/50 overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2.5 text-sm border-b border-border/50">
        <span>📝</span>
        <span className="font-mono text-xs">{filePath}</span>
        <span className="ml-auto text-xs text-muted-foreground tabular-nums">
          {entries.length} 次编辑
        </span>
      </div>
      <div className="px-3 py-2 space-y-1.5">
        {entries.map((entry) => (
          <AcpToolCallCard
            key={entry.id}
            update={entry.update}
            isPendingApproval={entry.isPendingApproval}
            compact
          />
        ))}
      </div>
    </div>
  );
}

export default AcpSessionEntry;
