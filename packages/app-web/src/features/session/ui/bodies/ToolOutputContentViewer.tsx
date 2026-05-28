/**
 * 统一工具输出内容 viewer
 *
 * 渲染 ToolOutputBlock[]：文本输出用可折叠文本面板，
 * 图片输出用图片预览，JSON fallback 用 JsonTree。
 */

import { useState, type ReactNode } from "react";
import type { ToolOutputBlock } from "./toolOutputContent";
import { JsonTree } from "./JsonTree";

const PREVIEW_CHARS = 2000;
const PREVIEW_LINES = 24;

export interface ToolOutputContentViewerProps {
  blocks: ToolOutputBlock[];
}

export function ToolOutputContentViewer({ blocks }: ToolOutputContentViewerProps): ReactNode {
  if (blocks.length === 0) return null;

  return (
    <div className="space-y-2">
      {blocks.map((block, idx) => (
        <OutputBlock key={idx} block={block} />
      ))}
    </div>
  );
}

function OutputBlock({ block }: { block: ToolOutputBlock }): ReactNode {
  switch (block.kind) {
    case "text":
      return <TextOutputPanel text={block.text} />;
    case "image":
      return <ImageOutputPanel imageUrl={block.imageUrl} label={block.label} />;
    case "resource":
      return <ResourceOutputPanel uri={block.uri} label={block.label} text={block.text} />;
    case "json":
      return <JsonTree data={block.value} defaultDepth={1} />;
  }
}

// ─── Text panel ────────────────────────────────────────────

function TextOutputPanel({ text }: { text: string }): ReactNode {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);

  const lines = text.split("\n");
  const totalLines = lines.length;
  const totalChars = text.length;
  const needsFold = totalLines > PREVIEW_LINES || totalChars > PREVIEW_CHARS;

  const showLines = expanded || !needsFold ? lines : lines.slice(0, PREVIEW_LINES);
  const hidden = totalLines - showLines.length;

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch { /* ignore */ }
  };

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-[11px] text-muted-foreground/60">
        <span className="tabular-nums">
          {totalLines} 行 · {totalChars.toLocaleString()} 字符
        </span>
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
          className={`overflow-auto whitespace-pre-wrap break-words p-2.5 font-mono text-xs leading-relaxed text-foreground/85 ${
            expanded ? "max-h-[60vh]" : ""
          }`}
        >
          {showLines.join("\n")}
        </pre>

        {hidden > 0 && !expanded && (
          <button
            type="button"
            onClick={() => setExpanded(true)}
            className="block w-full border-t border-border bg-secondary/30 px-2.5 py-1 text-center text-[11px] text-muted-foreground transition-colors hover:bg-secondary/60 hover:text-foreground"
          >
            展开余下 {hidden} 行
          </button>
        )}
        {expanded && needsFold && (
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

// ─── Image panel ───────────────────────────────────────────

function ImageOutputPanel({ imageUrl, label }: { imageUrl: string; label?: string }): ReactNode {
  return (
    <div className="space-y-1">
      {label && (
        <p className="text-[11px] text-muted-foreground/60">{label}</p>
      )}
      <img
        src={imageUrl}
        alt={label ?? "工具输出图片"}
        className="max-h-80 max-w-full rounded-[8px] border border-border object-contain"
      />
    </div>
  );
}

// ─── Resource panel ────────────────────────────────────────

function ResourceOutputPanel({
  uri,
  label,
  text,
}: {
  uri: string;
  label?: string;
  text?: string;
}): ReactNode {
  const [showText, setShowText] = useState(false);

  return (
    <div className="overflow-hidden rounded-[8px] border border-border">
      <button
        type="button"
        onClick={() => text && setShowText((v) => !v)}
        className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-secondary/35"
      >
        <span className="shrink-0 rounded bg-secondary px-1 py-px text-[10px] font-semibold text-muted-foreground">
          RES
        </span>
        <span className="min-w-0 flex-1 truncate font-mono text-foreground">
          {label ?? uri}
        </span>
        {text && (
          <span className="shrink-0 text-[10px] text-muted-foreground/40">
            {showText ? "▲" : "▼"}
          </span>
        )}
      </button>
      {showText && text && (
        <div className="border-t border-border p-2.5">
          <pre className="overflow-auto whitespace-pre-wrap break-words font-mono text-xs leading-relaxed text-foreground/85">
            {text}
          </pre>
        </div>
      )}
    </div>
  );
}
