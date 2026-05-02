/**
 * Project / Story 级 owner 上下文引用卡片
 *
 * 渲染 agentdash://project-context/* 和 agentdash://story-context/* 资源块。
 * 复用 EventStripCard 模板，badge 是唯一染色点。
 */

import { useMemo } from "react";
import type { ContentBlock } from "../model/types";
import { EventStripCard } from "./EventCards";

type OwnerContextType = "project" | "story";

interface ParsedOwnerContext {
  type: OwnerContextType;
  ownerId: string;
  text: string;
}

function parseOwnerContextBlock(block: ContentBlock): ParsedOwnerContext | null {
  if (block.type !== "resource") return null;
  const { uri } = block.resource;

  let type: OwnerContextType;
  let prefix: string;

  if (uri.startsWith("agentdash://project-context/")) {
    type = "project";
    prefix = "agentdash://project-context/";
  } else if (uri.startsWith("agentdash://story-context/")) {
    type = "story";
    prefix = "agentdash://story-context/";
  } else {
    return null;
  }

  const ownerId = uri.slice(prefix.length) || "unknown";
  const text =
    "text" in block.resource && typeof block.resource.text === "string"
      ? block.resource.text
      : "";

  return { type, ownerId, text };
}

const OWNER_TYPE_CONFIG: Record<OwnerContextType, { badge: string; label: string; badgeClass: string }> = {
  project: {
    badge: "PROJ",
    label: "Project 上下文",
    badgeClass: "border-primary/25 bg-primary/8 text-primary",
  },
  story: {
    badge: "STORY",
    label: "Story 上下文",
    badgeClass: "border-success/25 bg-success/8 text-success",
  },
};

export interface AcpOwnerContextCardProps {
  block: ContentBlock;
}

export function AcpOwnerContextCard({ block }: AcpOwnerContextCardProps) {
  const parsed = useMemo(() => parseOwnerContextBlock(block), [block]);
  if (!parsed) return null;

  const config = OWNER_TYPE_CONFIG[parsed.type];
  const shortId =
    parsed.ownerId.length > 8 ? `${parsed.ownerId.slice(0, 8)}…` : parsed.ownerId;
  const charCount = parsed.text.length;

  const rightHint = [
    charCount > 0 ? `${charCount.toLocaleString()} 字符` : null,
    shortId,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <EventStripCard
      badgeToken={config.badge}
      badgeClass={config.badgeClass}
      label={config.label}
      rightHint={rightHint}
      expandContent={
        parsed.text
          ? { raw: parsed.text }
          : { sections: [{ lines: ["上下文内容已通过 system prompt 注入，此处无预览。"] }] }
      }
    />
  );
}

export default AcpOwnerContextCard;
