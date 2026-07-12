/**
 * Read 类工具专用预览面板
 *
 * 适配 fsRead 与 dynamicToolCall(read)：
 * - 解析 `file: path` 标头和 `N | text` 行号前缀
 * - 文件路径显示在顶部 metadata，正文不含 `file:` 或 `N |`
 * - 默认折叠 ~24 行预览，可展开全文
 * - 提供复制正文和复制原始输出
 */

import { useMemo, useState, type ReactNode } from "react";
import type { ThreadItem, AgentDashThreadItem } from "../../../../generated/backbone-protocol";
import { parseReadToolText, type ParsedReadOutput } from "./readPayload";
import { CB } from "./cardBodyTokens";

const PREVIEW_LINES = 24;

export interface ReadCardBodyProps {
  item: AgentDashThreadItem;
}

export function ReadCardBody({ item }: ReadCardBodyProps): ReactNode {
  const parsed = useMemo(() => buildParsedRead(item), [item]);
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState<"body" | "raw" | false>(false);

  if (!parsed || parsed.lines.length === 0) {
    return <p className="text-xs text-muted-foreground">尚无读取内容</p>;
  }

  const totalLines = parsed.lines.length;
  const showLines = expanded ? parsed.lines : parsed.lines.slice(0, PREVIEW_LINES);
  const hidden = totalLines - showLines.length;

  const handleCopy = async (mode: "body" | "raw") => {
    try {
      const text = mode === "body" ? parsed.bodyText : parsed.rawText;
      await navigator.clipboard.writeText(text);
      setCopied(mode);
      window.setTimeout(() => setCopied(false), 1500);
    } catch { /* ignore */ }
  };

  return (
    <div className={CB.sectionGap}>
      <div className={`flex items-center justify-between ${CB.meta}`}>
        <div className="flex items-center gap-2">
          {parsed.filePath && (
            <span className="truncate font-mono text-foreground/70" title={parsed.filePath}>
              {parsed.filePath}
            </span>
          )}
          <span className="tabular-nums">{totalLines} 行</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => void handleCopy("body")}
            className={CB.actionButton}
          >
            {copied === "body" ? "已复制" : "复制正文"}
          </button>
          <button
            type="button"
            onClick={() => void handleCopy("raw")}
            className={CB.actionButton}
          >
            {copied === "raw" ? "已复制" : "复制原始"}
          </button>
        </div>
      </div>

      <div className={`overflow-hidden ${CB.inlineEntry}`}>
        <pre
          className={`overflow-auto font-mono text-xs leading-relaxed ${
            expanded ? "max-h-[60vh]" : ""
          }`}
        >
          {showLines.map((line, idx) => (
            <div
              key={idx}
              className="grid grid-cols-[3.5rem_1fr] items-baseline"
            >
              <span className={CB.lineNumber}>
                {line.lineNo}
              </span>
              <span className="whitespace-pre-wrap break-words pr-2 text-foreground/80">
                {line.text || " "}
              </span>
            </div>
          ))}
        </pre>

        {hidden > 0 && !expanded && (
          <button
            type="button"
            onClick={() => setExpanded(true)}
            className={`block w-full border-t border-border/30 bg-secondary/20 px-2.5 py-1 text-center ${CB.actionButton}`}
          >
            展开余下 {hidden} 行
          </button>
        )}
        {expanded && totalLines > PREVIEW_LINES && (
          <button
            type="button"
            onClick={() => setExpanded(false)}
            className={`block w-full border-t border-border/30 bg-secondary/20 px-2.5 py-1 text-center ${CB.actionButton}`}
          >
            折叠
          </button>
        )}
      </div>
    </div>
  );
}

function buildParsedRead(item: AgentDashThreadItem): ParsedReadOutput | null {
  const rawText = extractTextFromItem(item);
  if (rawText == null) return null;

  const fallbackStart = getFallbackStartLine(item);
  return parseReadToolText(rawText, fallbackStart);
}

function getFallbackStartLine(item: AgentDashThreadItem): number {
  if (item.type === "fsRead") return item.offset ?? 1;
  if (item.type === "dynamicToolCall" && item.tool.toLowerCase() === "read") {
    const args = item.arguments as Record<string, unknown> | null;
    const offsetRaw = args?.offset;
    return typeof offsetRaw === "number" && Number.isFinite(offsetRaw) ? offsetRaw : 1;
  }
  return 1;
}

type ContentItems = NonNullable<
  Extract<ThreadItem, { type: "dynamicToolCall" }>["contentItems"]
>;

function extractTextFromItem(item: AgentDashThreadItem): string | null {
  if (item.type === "fsRead" || item.type === "dynamicToolCall") {
    return extractText(item.contentItems ?? null);
  }
  return null;
}

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
