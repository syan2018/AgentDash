/**
 * Project / Story 级 owner 上下文引用卡片
 *
 * 渲染 agentdash://project-context/* 和 agentdash://story-context/* 资源块。
 * 与 AcpTaskContextCard 保持一致的可展开细条样式，但使用不同的 badge 和标签区分。
 *
 * 该卡片仅作为展示锚点：说明"此次对话注入了 project/story 上下文"。
 * 实际上下文内容已通过 system_context 注入到 system prompt，不在消息流中重复展示指令文字。
 */

import { useMemo, useState } from "react";
import type { ContentBlock } from "../model/types";

type OwnerContextType = "project" | "story";

interface ParsedOwnerContext {
  type: OwnerContextType;
  ownerId: string;
  text: string;
}

function parseOwnerContextBlock(block: ContentBlock): ParsedOwnerContext | null {
  if (block.type !== "resource") return null;
  const { uri } = block.resource;

  let type: OwnerContextType;
  let prefix: string;

  if (uri.startsWith("agentdash://project-context/")) {
    type = "project";
    prefix = "agentdash://project-context/";
  } else if (uri.startsWith("agentdash://story-context/")) {
    type = "story";
    prefix = "agentdash://story-context/";
  } else {
    return null;
  }

  const ownerId = uri.slice(prefix.length) || "unknown";
  const text = "text" in block.resource && typeof block.resource.text === "string"
    ? block.resource.text
    : "";

  return { type, ownerId, text };
}

const OWNER_TYPE_CONFIG: Record<OwnerContextType, { badge: string; label: string; badgeStyle: string }> = {
  project: {
    badge: "PROJ",
    label: "Project 上下文",
    badgeStyle: "border-blue-400/30 bg-blue-400/10 text-blue-500",
  },
  story: {
    badge: "STORY",
    label: "Story 上下文",
    badgeStyle: "border-violet-400/30 bg-violet-400/10 text-violet-500",
  },
};

export interface AcpOwnerContextCardProps {
  block: ContentBlock;
}

export function AcpOwnerContextCard({ block }: AcpOwnerContextCardProps) {
  const [expanded, setExpanded] = useState(false);
  const parsed = useMemo(() => parseOwnerContextBlock(block), [block]);
  if (!parsed) return null;

  const config = OWNER_TYPE_CONFIG[parsed.type];
  const shortId = parsed.ownerId.length > 8
    ? `${parsed.ownerId.slice(0, 8)}…`
    : parsed.ownerId;
  const charCount = parsed.text.length;

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors hover:bg-secondary/35"
      >
        {/* 类型 badge */}
        <span className={`inline-flex shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] ${config.badgeStyle}`}>
          {config.badge}
        </span>

        {/* 标签 */}
        <span className="text-sm font-medium text-foreground">{config.label}</span>

        {/* 字符数 */}
        {charCount > 0 && (
          <span className="text-[10px] text-muted-foreground/60">
            {charCount.toLocaleString()} 字符
          </span>
        )}

        {/* owner ID */}
        <span className="ml-auto font-mono text-[10px] text-muted-foreground/50">
          {shortId}
        </span>

        <span className="shrink-0 text-[10px] text-muted-foreground/40">
          {expanded ? "▲" : "▼"}
        </span>
      </button>

      {expanded && (
        <div className="border-t border-border px-3 py-2.5">
          {parsed.text ? (
            <pre className="max-h-72 overflow-auto whitespace-pre-wrap text-xs leading-relaxed text-foreground/75">
              {parsed.text}
            </pre>
          ) : (
            <p className="text-xs text-muted-foreground">上下文内容已通过 system prompt 注入，此处无预览。</p>
          )}
        </div>
      )}
    </div>
  );
}

export default AcpOwnerContextCard;
