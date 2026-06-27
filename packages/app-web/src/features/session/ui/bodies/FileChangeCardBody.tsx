/**
 * 文件变更 body — 按文件展示 kind / +N -M / diff
 */

import { useState, useMemo } from "react";
import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { DiffCardBody } from "./DiffCardBody";
import { parseUnifiedDiff } from "./diffPayload";
import { CB } from "./cardBodyTokens";

type FileChangeItem = Extract<ThreadItem, { type: "fileChange" }>;

export function FileChangeCardBody({ item }: { item: FileChangeItem }) {
  if (item.changes.length === 0) {
    return <p className="text-xs text-muted-foreground">无文件变更</p>;
  }

  return (
    <div className={CB.itemGap}>
      {item.changes.map((change) => (
        <FileChangeBlock key={change.path} change={change} />
      ))}
    </div>
  );
}

function FileChangeBlock({
  change,
}: {
  change: FileChangeItem["changes"][number];
}) {
  const [expanded, setExpanded] = useState(false);
  const payload = useMemo(() => parseUnifiedDiff(change.diff ?? ""), [change.diff]);
  const kindLabel = getChangeKindLabel(change.kind);

  return (
    <div className={`overflow-hidden ${CB.inlineEntry}`}>
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className={CB.inlineEntryButton}
      >
        {kindLabel && (
          <span className={CB.kindBadge}>
            {kindLabel}
          </span>
        )}
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground/80">
          {change.path}
        </span>
        {change.kind.type === "update" && change.kind.move_path && (
          <span className={CB.meta}>
            → {change.kind.move_path}
          </span>
        )}
        {(payload.added > 0 || payload.removed > 0) && (
          <span className="flex shrink-0 gap-1.5 text-[10px]">
            {payload.added > 0 && (
              <span className={CB.diffAdded}>+{payload.added}</span>
            )}
            {payload.removed > 0 && (
              <span className={CB.diffRemoved}>-{payload.removed}</span>
            )}
          </span>
        )}
        <span className={CB.expandToggle}>
          {expanded ? "▲" : "▼"}
        </span>
      </button>
      {expanded && change.diff && (
        <div className="border-t border-border/30 p-2">
          <DiffCardBody payload={payload} />
        </div>
      )}
    </div>
  );
}

function getChangeKindLabel(
  kind: FileChangeItem["changes"][number]["kind"],
): string | null {
  switch (kind.type) {
    case "add":
      return "NEW";
    case "delete":
      return "DEL";
    case "update":
      return kind.move_path ? "RENAME" : null;
    default:
      return null;
  }
}
