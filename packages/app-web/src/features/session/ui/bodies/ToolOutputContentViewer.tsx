/**
 * 统一工具输出内容 viewer
 *
 * 渲染 ToolOutputBlock[]：文本输出用可折叠文本面板，
 * 图片输出用图片预览，JSON fallback 用 JsonTree。
 */

import { useState, type ReactNode } from "react";
import type { ToolOutputBlock } from "./toolOutputContent";
import { formatBytes, parseBoundedOutputText, type BoundedOutputInfo } from "../../model/boundedOutput";
import { JsonTree } from "./JsonTree";
import { CB } from "./cardBodyTokens";

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
  const boundedOutput = parseBoundedOutputText(text);

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
    <div className={CB.itemGap}>
      {boundedOutput && (
        <BoundedOutputNotice info={boundedOutput} />
      )}

      <div className={`flex items-center justify-between ${CB.meta}`}>
        <span className="tabular-nums">
          {totalLines} 行 · {totalChars.toLocaleString()} 字符
        </span>
        <button
          type="button"
          onClick={() => void handleCopy()}
          className={CB.actionButton}
        >
          {copied ? "已复制" : "复制"}
        </button>
      </div>

      <div className={`overflow-hidden ${CB.inlineEntry}`}>
        <pre
          className={`overflow-auto whitespace-pre-wrap break-words ${CB.codeBlock} ${
            expanded ? "max-h-[60vh]" : ""
          }`}
        >
          {showLines.join("\n")}
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
        {expanded && needsFold && (
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

function BoundedOutputNotice({ info }: { info: BoundedOutputInfo }): ReactNode {
  const parts = ["输出已裁切"];
  if (info.omittedBytes != null) {
    parts.push(`省略 ${formatBytes(info.omittedBytes)}`);
  }
  if (info.policy) {
    parts.push(`policy: ${info.policy}`);
  }

  return (
    <div className={`rounded-[6px] border border-warning/25 bg-warning/5 px-2 py-1.5 ${CB.meta}`}>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className={CB.statusWarning}>{parts.join(" · ")}</span>
        {info.lifecyclePath && (
          <code className="max-w-full truncate text-[10px] text-muted-foreground/60">
            {info.lifecyclePath}
          </code>
        )}
      </div>
    </div>
  );
}

// ─── Image panel ───────────────────────────────────────────

function ImageOutputPanel({ imageUrl, label }: { imageUrl: string; label?: string }): ReactNode {
  return (
    <div className={CB.itemGap}>
      {label && (
        <p className={CB.meta}>{label}</p>
      )}
      <img
        src={imageUrl}
        alt={label ?? "工具输出图片"}
        className="max-h-80 max-w-full rounded-[6px] border border-border/30 object-contain"
      />
    </div>
  );
}

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
    <div className={`overflow-hidden ${CB.inlineEntry}`}>
      <button
        type="button"
        onClick={() => text && setShowText((v) => !v)}
        className={CB.inlineEntryButton}
      >
        <span className={CB.kindBadge}>RES</span>
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground/80">
          {label ?? uri}
        </span>
        {text && (
          <span className={CB.expandToggle}>
            {showText ? "▲" : "▼"}
          </span>
        )}
      </button>
      {showText && text && (
        <div className="border-t border-border/30 p-2">
          <pre className={`overflow-auto whitespace-pre-wrap break-words ${CB.codeBlock}`}>
            {text}
          </pre>
        </div>
      )}
    </div>
  );
}
