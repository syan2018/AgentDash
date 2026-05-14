/**
 * Task 预构造上下文卡片
 *
 * 渲染 agentdash://task-context/* 资源块。
 * 复用 EventStripCard 模板，badge 是唯一染色点。
 */

import { useMemo } from "react";
import type { ContentBlock } from "../model/types";
import { EventStripCard } from "./EventCards";

interface ParsedTaskContext {
  taskId: string;
  phase: "start" | "continue" | "unknown";
  text: string;
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

  const text =
    "text" in resource && typeof resource.text === "string" ? resource.text : "";
  return { taskId, phase, text };
}

const PHASE_LABELS: Record<ParsedTaskContext["phase"], string> = {
  start:    "启动",
  continue: "继续",
  unknown:  "未知",
};

export interface AcpTaskContextCardProps {
  block: ContentBlock;
}

export function AcpTaskContextCard({ block }: AcpTaskContextCardProps) {
  const parsed = useMemo(() => parseTaskContextBlock(block), [block]);
  if (!parsed) return null;

  const shortId =
    parsed.taskId.length > 8 ? `${parsed.taskId.slice(0, 8)}…` : parsed.taskId;
  const phaseLabel = PHASE_LABELS[parsed.phase];
  const rightHint = `${phaseLabel} · ${shortId}`;

  return (
    <EventStripCard
      badgeToken="CTX"
      label="Task 上下文注入"
      rightHint={rightHint}
      expandContent={
        parsed.text
          ? { raw: parsed.text }
          : { sections: [{ lines: ["未提供上下文文本内容"] }] }
      }
    />
  );
}

export default AcpTaskContextCard;
