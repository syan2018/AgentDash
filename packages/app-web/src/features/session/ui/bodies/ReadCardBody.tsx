/**
 * Read 类工具专用预览面板
 *
 * 适配 fsRead 与 dynamicToolCall(read)：
 * - 从 contentItems 抽取文本内容
 * - 行号从 offset 起算（默认 1）
 * - 默认折叠 ~24 行预览，可展开全文
 * - 提供"复制原文"按钮
 *
 * MVP 不做语法高亮，仅等宽体 + 行号。
 */

import { useMemo, useState, type ReactNode } from "react";
import type { ThreadItem, AgentDashThreadItem } from "../../../../generated/backbone-protocol";

const PREVIEW_LINES = 24;

export interface ReadCardBodyProps {
  item: AgentDashThreadItem;
}

export function ReadCardBody({ item }: ReadCardBodyProps): ReactNode {
  const payload = useMemo(() => normalizeReadItem(item), [item]);
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);

  if (!payload || payload.text.length === 0) {
    return <p className="text-xs text-muted-foreground">尚无读取内容</p>;
  }

  const lines = payload.text.split("\n");
  const totalLines = lines.length;
  const showLines = expanded ? lines : lines.slice(0, PREVIEW_LINES);
  const hidden = totalLines - showLines.length;

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(payload.text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      /* ignore */
    }
  };

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-[11px] text-muted-foreground/60">
        <span className="tabular-nums">{totalLines} 行</span>
        <button
          type="button"
          onClick={() => void handleCopy()}
          className="rounded px-1.5 py-0.5 text-[11px] text-muted-foreground/70 transition-colors hover:bg-secondary hover:text-foreground"
        >
          {copied ? "已复制" : "复制"}
        </button>
      </div>

      <div className="overflow-hidden rounded-[8px] border border-border bg-muted/20">
        <pre
          className={`overflow-auto font-mono text-xs leading-relaxed ${
            expanded ? "max-h-[60vh]" : ""
          }`}
        >
          {showLines.map((line, idx) => {
            const lineNo = payload.startLine + idx;
            return (
              <div
                key={lineNo}
                className="grid grid-cols-[3.5rem_1fr] items-baseline"
              >
                <span className="select-none px-2 text-right tabular-nums text-muted-foreground/40">
                  {lineNo}
                </span>
                <span className="whitespace-pre-wrap break-words pr-2 text-foreground/85">
                  {line || " "}
                </span>
              </div>
            );
          })}
        </pre>

        {hidden > 0 && (
          <button
            type="button"
            onClick={() => setExpanded(true)}
            className="block w-full border-t border-border bg-secondary/30 px-2.5 py-1 text-center text-[11px] text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
          >
            展开余下 {hidden} 行
          </button>
        )}
        {expanded && totalLines > PREVIEW_LINES && (
          <button
            type="button"
            onClick={() => setExpanded(false)}
            className="block w-full border-t border-border bg-secondary/30 px-2.5 py-1 text-center text-[11px] text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
          >
            折叠
          </button>
        )}
      </div>
    </div>
  );
}

interface ReadPayload {
  text: string;
  startLine: number;
  totalLines: number;
}

function normalizeReadItem(item: AgentDashThreadItem): ReadPayload | null {
  // fsRead：原生字段
  if (item.type === "fsRead") {
    const text = extractText(item.contentItems);
    if (text == null) return null;
    return {
      text,
      startLine: item.offset ?? 1,
      totalLines: text.split("\n").length,
    };
  }

  // dynamicToolCall(read)：从 arguments 取 offset
  if (item.type === "dynamicToolCall" && item.tool.toLowerCase() === "read") {
    const text = extractText(item.contentItems);
    if (text == null) return null;
    const args = item.arguments as Record<string, unknown> | null;
    const offsetRaw = args?.offset;
    const startLine = typeof offsetRaw === "number" && Number.isFinite(offsetRaw) ? offsetRaw : 1;
    return {
      text,
      startLine,
      totalLines: text.split("\n").length,
    };
  }

  return null;
}

type ContentItems = NonNullable<
  Extract<ThreadItem, { type: "dynamicToolCall" }>["contentItems"]
>;

function extractText(items: ContentItems | null): string | null {
  if (!items || items.length === 0) return null;
  const parts: string[] = [];
  for (const item of items) {
    if (item.type === "inputText") {
      parts.push(item.text);
    }
  }
  if (parts.length === 0) return null;
  return parts.join("");
}
