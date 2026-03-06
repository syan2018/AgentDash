/**
 * Task 预构造上下文卡片
 *
 * 渲染 agentdash://task-context/* 资源块，用于把任务上下文从普通文本区分出来。
 */

import { useMemo, useState } from "react";
import type { ContentBlock } from "../model/types";

interface ParsedTaskContext {
  taskId: string;
  phase: "start" | "continue" | "unknown";
  text: string;
}

export interface AcpTaskContextCardProps {
  block: ContentBlock;
}

function parseTaskContextBlock(block: ContentBlock): ParsedTaskContext | null {
  if (block.type !== "resource") return null;
  const resource = block.resource;
  if (!resource.uri.startsWith("agentdash://task-context/")) return null;

  let taskId = "unknown";
  let phase: ParsedTaskContext["phase"] = "unknown";
  try {
    const url = new URL(resource.uri);
    taskId = url.pathname.replace(/^\/+/, "") || "unknown";
    const queryPhase = url.searchParams.get("phase");
    if (queryPhase === "start" || queryPhase === "continue") {
      phase = queryPhase;
    }
  } catch {
    const fallback = resource.uri.replace("agentdash://task-context/", "");
    const [path] = fallback.split("?");
    if (path) taskId = path;
  }

  const text = "text" in resource && typeof resource.text === "string" ? resource.text : "";
  return { taskId, phase, text };
}

export function AcpTaskContextCard({ block }: AcpTaskContextCardProps) {
  const [expanded, setExpanded] = useState(false);
  const parsed = useMemo(() => parseTaskContextBlock(block), [block]);
  if (!parsed) return null;

  const phaseLabel =
    parsed.phase === "start" ? "启动" : parsed.phase === "continue" ? "继续" : "未知";

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors hover:bg-secondary/35"
      >
        <span className="inline-flex rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          CTX
        </span>
        <span className="text-sm font-medium text-foreground">Task 上下文注入</span>
        <span className="rounded-[6px] border border-primary/20 bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
          {phaseLabel}
        </span>
        <span className="ml-auto text-[10px] text-muted-foreground font-mono">
          {parsed.taskId.length > 8 ? `${parsed.taskId.slice(0, 8)}...` : parsed.taskId}
        </span>
        <span className="text-xs text-muted-foreground/50">
          {expanded ? "▲" : "▼"}
        </span>
      </button>
      {expanded && (
        <div className="border-t border-border px-3 py-2.5">
          {parsed.text ? (
            <pre className="max-h-56 overflow-auto whitespace-pre-wrap text-xs leading-relaxed text-foreground/80">
              {parsed.text}
            </pre>
          ) : (
            <p className="text-xs text-muted-foreground">未提供上下文文本内容</p>
          )}
        </div>
      )}
    </div>
  );
}

export default AcpTaskContextCard;
