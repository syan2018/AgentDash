import type { ReactNode, RefObject } from "react";

import type { AgentRuntimeFeedEntry } from "../model/useAgentRuntimeFeed";

export interface AgentRuntimeFeedProps {
  containerRef: RefObject<HTMLDivElement | null>;
  entries: AgentRuntimeFeedEntry[];
  isLoading: boolean;
  streamPrefixContent?: ReactNode;
  onScroll: () => void;
}

function entryStyle(entry: AgentRuntimeFeedEntry): string {
  if (entry.role === "user") return "ml-auto bg-primary/10 text-foreground";
  if (entry.role === "agent") return "mr-auto bg-card text-foreground shadow-sm";
  return "mr-auto bg-secondary/40 text-muted-foreground";
}

export function AgentRuntimeFeed({
  containerRef,
  entries,
  isLoading,
  streamPrefixContent,
  onScroll,
}: AgentRuntimeFeedProps) {
  return (
    <div ref={containerRef} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      {streamPrefixContent}
      {entries.length === 0 && (
        <div className="flex min-h-32 items-center justify-center text-sm text-muted-foreground">
          {isLoading ? "正在读取 Agent Runtime…" : "尚无 Runtime transcript"}
        </div>
      )}
      <div className="space-y-3">
        {entries.map((entry) => (
          <article
            key={entry.id}
            className={`w-fit max-w-[85%] rounded-[8px] px-3 py-2 text-sm ${entryStyle(entry)}`}
          >
            <div className="mb-1 flex items-center gap-2 text-[10px] uppercase tracking-wider text-muted-foreground">
              <span>{entry.role}</span>
              {entry.status !== "completed" && <span>{entry.status}</span>}
            </div>
            <div className="whitespace-pre-wrap break-words leading-relaxed">{entry.text}</div>
          </article>
        ))}
      </div>
    </div>
  );
}
