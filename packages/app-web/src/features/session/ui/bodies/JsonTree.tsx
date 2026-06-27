/**
 * 可折叠 JSON 树组件
 *
 * 递归渲染任意 JSON 值：标量直接显示，对象/数组可折叠，
 * 附带复制原始 JSON 按钮。
 */

import { useState, useCallback, type ReactNode } from "react";
import { CB } from "./cardBodyTokens";

export interface JsonTreeProps {
  data: unknown;
  /** 默认展开层数；0 = 全折叠 */
  defaultDepth?: number;
}

export function JsonTree({ data, defaultDepth = 1 }: JsonTreeProps) {
  return (
    <div className="font-mono text-xs leading-relaxed">
      <JsonNode value={data} depth={0} defaultDepth={defaultDepth} />
    </div>
  );
}

function JsonNode({
  value,
  depth,
  defaultDepth,
  label,
}: {
  value: unknown;
  depth: number;
  defaultDepth: number;
  label?: string;
}) {
  const [expanded, setExpanded] = useState(depth < defaultDepth);

  if (value === null || value === undefined) {
    return (
      <Row label={label}>
        <span className="text-muted-foreground/60">null</span>
      </Row>
    );
  }

  if (typeof value === "boolean") {
    return (
      <Row label={label}>
        <span className="text-primary">{String(value)}</span>
      </Row>
    );
  }

  if (typeof value === "number" || typeof value === "bigint") {
    return (
      <Row label={label}>
        <span className="text-primary">{String(value)}</span>
      </Row>
    );
  }

  if (typeof value === "string") {
    const isLong = value.length > 120;
    return (
      <Row label={label}>
        <span className="text-success break-all" title={isLong ? value : undefined}>
          &quot;{isLong ? value.slice(0, 120) + "…" : value}&quot;
        </span>
      </Row>
    );
  }

  if (Array.isArray(value)) {
    if (value.length === 0) {
      return <Row label={label}><span className="text-muted-foreground">[]</span></Row>;
    }
    return (
      <CollapsibleNode
        label={label}
        summary={`[ ${value.length} items ]`}
        expanded={expanded}
        onToggle={() => setExpanded(!expanded)}
      >
        {value.map((item, i) => (
          <JsonNode
            key={i}
            value={item}
            depth={depth + 1}
            defaultDepth={defaultDepth}
            label={String(i)}
          />
        ))}
      </CollapsibleNode>
    );
  }

  if (typeof value === "object") {
    const entries = Object.entries(value);
    if (entries.length === 0) {
      return <Row label={label}><span className="text-muted-foreground">{"{}"}</span></Row>;
    }
    return (
      <CollapsibleNode
        label={label}
        summary={`{ ${entries.length} keys }`}
        expanded={expanded}
        onToggle={() => setExpanded(!expanded)}
      >
        {entries.map(([key, val]) => (
          <JsonNode
            key={key}
            value={val}
            depth={depth + 1}
            defaultDepth={defaultDepth}
            label={key}
          />
        ))}
      </CollapsibleNode>
    );
  }

  return (
    <Row label={label}>
      <span className="text-muted-foreground">{String(value)}</span>
    </Row>
  );
}

function Row({ label, children }: { label?: string; children: ReactNode }) {
  return (
    <div className="flex gap-1.5 py-px">
      {label != null && (
        <span className="shrink-0 text-foreground/70">{label}:</span>
      )}
      {children}
    </div>
  );
}

function CollapsibleNode({
  label,
  summary,
  expanded,
  onToggle,
  children,
}: {
  label?: string;
  summary: string;
  expanded: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div>
      <button
        type="button"
        onClick={onToggle}
        className="flex items-center gap-1 py-px text-left hover:text-foreground"
      >
        <span className="text-muted-foreground/50">{expanded ? "▾" : "▸"}</span>
        {label != null && (
          <span className="text-foreground/70">{label}:</span>
        )}
        {!expanded && (
          <span className="text-muted-foreground">{summary}</span>
        )}
      </button>
      {expanded && (
        <div className="ml-3 border-l border-border/40 pl-2">
          {children}
        </div>
      )}
    </div>
  );
}

export function CopyJsonButton({ data }: { data: unknown }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(JSON.stringify(data, null, 2));
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* clipboard not available */ }
  }, [data]);

  return (
    <button
      type="button"
      onClick={() => { void handleCopy(); }}
      className={CB.actionButton}
    >
      {copied ? "已复制" : "复制 JSON"}
    </button>
  );
}
