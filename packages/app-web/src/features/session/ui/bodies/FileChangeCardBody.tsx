/**
 * 文件变更 body — 按文件展示 kind / +N -M / diff
 */

import { useState, useMemo } from "react";
import type { ThreadItem } from "../../../../generated/backbone-protocol";

type FileChangeItem = Extract<ThreadItem, { type: "fileChange" }>;

export function FileChangeCardBody({ item }: { item: FileChangeItem }) {
  if (item.changes.length === 0) {
    return <p className="text-xs text-muted-foreground">无文件变更</p>;
  }

  return (
    <div className="space-y-1.5">
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
  const stats = useMemo(() => countDiffLines(change.diff), [change.diff]);
  const kindLabel = getChangeKindLabel(change.kind);

  return (
    <div className="overflow-hidden rounded-[8px] border border-border">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left transition-colors hover:bg-secondary/35"
      >
        {kindLabel && (
          <span className="shrink-0 rounded bg-secondary px-1 py-px text-[10px] font-semibold text-muted-foreground">
            {kindLabel}
          </span>
        )}
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground">
          {change.path}
        </span>
        {change.kind.type === "update" && change.kind.move_path && (
          <span className="truncate text-[10px] text-muted-foreground">
            → {change.kind.move_path}
          </span>
        )}
        {(stats.added > 0 || stats.removed > 0) && (
          <span className="flex shrink-0 gap-1.5 text-xs">
            {stats.added > 0 && (
              <span className="text-success">+{stats.added}</span>
            )}
            {stats.removed > 0 && (
              <span className="text-destructive">-{stats.removed}</span>
            )}
          </span>
        )}
        <span className="shrink-0 text-[10px] text-muted-foreground/40">
          {expanded ? "▲" : "▼"}
        </span>
      </button>
      {expanded && change.diff && (
        <pre className="agentdash-chat-code-block max-h-96 border-t border-border">
          {change.diff}
        </pre>
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

function countDiffLines(diff: string): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  if (!diff) return { added, removed };

  for (const line of diff.split("\n")) {
    if (line.startsWith("+++") || line.startsWith("---")) continue;
    if (line.startsWith("+")) added++;
    else if (line.startsWith("-")) removed++;
  }
  return { added, removed };
}
