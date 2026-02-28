/**
 * ACP 会话条目渲染组件
 *
 * 根据条目类型渲染不同的 UI
 */

import { useState } from "react";
import {
  isAggregatedGroup,
  isAggregatedThinkingGroup,
  isDisplayEntry,
  extractTextFromContentBlock,
} from "../model/types";
import type { AcpDisplayItem, AcpDisplayEntry, AggregatedEntryGroup, AggregatedThinkingGroup } from "../model/types";
import { AcpToolCallCard } from "./AcpToolCallCard";
import { AcpMessageCard } from "./AcpMessageCard";
import { AcpPlanCard } from "./AcpPlanCard";

export interface AcpSessionEntryProps {
  item: AcpDisplayItem;
  /** ID of the entry currently being streamed, or null */
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
      const text = extractTextFromContentBlock(update.content);
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

    case "available_commands_update":
    case "current_mode_update":
    case "config_option_update":
    case "session_info_update":
    case "usage_update":
      return null;

    default:
      return null;
  }
}

function AggregatedToolGroupEntry({ group }: { group: AggregatedEntryGroup }) {
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
    <div className="rounded-md border border-border bg-card/50 p-3">
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <span>{getIcon()}</span>
        <span>{getLabel()}</span>
        <span className="ml-auto text-xs">{entries.length} 个</span>
      </div>
      <div className="mt-2 space-y-1">
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
    <div className="rounded-md border border-border bg-muted/30 p-3">
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center justify-between text-sm text-muted-foreground"
      >
        <span className="flex items-center gap-2">
          <span>🧠</span>
          <span>思考过程 ({entries.length} 条)</span>
        </span>
        <span>{expanded ? "收起" : "展开"}</span>
      </button>
      {expanded && (
        <div className="mt-2 text-sm text-muted-foreground">
          <pre className="whitespace-pre-wrap font-mono text-xs">
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
    <div className="rounded-md border border-border bg-card/50 p-3">
      <div className="flex items-center gap-2 text-sm">
        <span>📝</span>
        <span className="font-mono">{filePath}</span>
        <span className="ml-auto text-xs text-muted-foreground">
          {entries.length} 次编辑
        </span>
      </div>
      <div className="mt-2 space-y-1">
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
