/**
 * 统一 diff 渲染面板
 *
 * 服务于 fileChange 与 dynamicToolCall(edit/str_replace_editor/applypatch/write)。
 * 解析/合成 helper 见同目录 diffPayload.ts。
 *
 * 渲染：双列行号 + 行级 +/-/context 着色，超长折叠。
 */

import { useMemo, useState, type ReactNode } from "react";
import {
  parseUnifiedDiff,
  synthesizeFromOldNew,
  type DiffLine,
  type DiffPayload,
} from "./diffPayload";

const DEFAULT_PREVIEW_LINES = 40;

export interface DiffCardBodyProps {
  payload: DiffPayload;
}

export function DiffCardBody({ payload }: DiffCardBodyProps): ReactNode {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const totalLines = payload.lines.length;
  const showLines = expanded
    ? payload.lines
    : payload.lines.slice(0, DEFAULT_PREVIEW_LINES);
  const hidden = totalLines - showLines.length;

  if (totalLines === 0) {
    return <p className="text-xs text-muted-foreground">无差异</p>;
  }

  const handleCopyDiff = async () => {
    try {
      const text = payload.lines
        .map((l) => {
          if (l.kind === "hunk" || l.kind === "meta") return l.text;
          const prefix = l.kind === "add" ? "+" : l.kind === "remove" ? "-" : " ";
          return prefix + l.text;
        })
        .join("\n");
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch { /* ignore */ }
  };

  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-2 text-[11px]">
        <span className="text-success tabular-nums">+{payload.added}</span>
        <span className="text-destructive tabular-nums">-{payload.removed}</span>
        <span className="text-muted-foreground/50">· {totalLines} 行</span>
        <button
          type="button"
          onClick={() => void handleCopyDiff()}
          className="ml-auto rounded px-1.5 py-0.5 text-[11px] text-muted-foreground/70 transition-colors hover:bg-secondary hover:text-foreground"
        >
          {copied ? "已复制" : "复制 diff"}
        </button>
      </div>

      <div className="overflow-hidden rounded-[8px] border border-border bg-muted/15">
        <pre className={`overflow-auto font-mono text-xs leading-relaxed ${expanded ? "max-h-[60vh]" : ""}`}>
          {showLines.map((line, idx) => (
            <DiffLineRow key={idx} line={line} />
          ))}
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
        {expanded && totalLines > DEFAULT_PREVIEW_LINES && (
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

function DiffLineRow({ line }: { line: DiffLine }): ReactNode {
  if (line.kind === "hunk" || line.kind === "meta") {
    return (
      <div className="grid grid-cols-[3rem_3rem_1rem_1fr] items-baseline bg-secondary/40">
        <span className="px-1 text-right tabular-nums text-muted-foreground/40">·</span>
        <span className="px-1 text-right tabular-nums text-muted-foreground/40">·</span>
        <span className="text-muted-foreground/40">·</span>
        <span className="whitespace-pre-wrap break-words pr-2 text-muted-foreground/60">
          {line.text}
        </span>
      </div>
    );
  }

  const sign = line.kind === "add" ? "+" : line.kind === "remove" ? "-" : " ";
  const rowBg =
    line.kind === "add"
      ? "bg-success/10"
      : line.kind === "remove"
        ? "bg-destructive/10"
        : "";
  const textColor =
    line.kind === "add"
      ? "text-success"
      : line.kind === "remove"
        ? "text-destructive"
        : "text-foreground/85";

  return (
    <div className={`grid grid-cols-[3rem_3rem_1rem_1fr] items-baseline ${rowBg}`}>
      <span className="select-none px-1 text-right tabular-nums text-muted-foreground/40">
        {line.oldNo ?? ""}
      </span>
      <span className="select-none px-1 text-right tabular-nums text-muted-foreground/40">
        {line.newNo ?? ""}
      </span>
      <span className={`select-none text-center ${textColor}`}>{sign}</span>
      <span className={`whitespace-pre-wrap break-words pr-2 ${textColor}`}>
        {line.text || " "}
      </span>
    </div>
  );
}

export interface DiffCardBodyAutoProps {
  /** 已合好的 unified diff 文本 */
  diff?: string;
  /** old + new 文本对 */
  oldText?: string;
  newText?: string;
}

export function DiffCardBodyAuto(props: DiffCardBodyAutoProps): ReactNode {
  const payload = useMemo(() => {
    if (props.diff != null && props.diff.length > 0) {
      return parseUnifiedDiff(props.diff);
    }
    return synthesizeFromOldNew(props.oldText ?? "", props.newText ?? "");
  }, [props.diff, props.oldText, props.newText]);

  return <DiffCardBody payload={payload} />;
}
