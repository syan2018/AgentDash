/**
 * ContextFrame 外层 shell
 *
 * 单帧 / 多帧 context_frame 事件统一经此渲染：
 *
 * - header 行：CTX badge + "N 帧 · 最后阶段 X" 汇总 + 展开箭头
 * - 展开后：横向 frame tab 条（单帧时等效 pill label）+ 对应 frame body
 *
 * 所有 frame 数据是 model 层已解析的 `ContextFrame`。UI 只负责展示。
 */

import { useState } from "react";
import type { ContextFrame } from "../model/contextFrame";
import { frameKindToToken } from "../model/contextFrame";
import { ContextFrameBody } from "./ContextFrameBody";
import { DisclosureRow } from "../../../components/ui/disclosure";
import { ST } from "./bodies/cardBodyTokens";

export interface ContextFrameStreamProps {
  frames: ContextFrame[];
  /** 默认是否展开外层 shell；测试或持久化可覆盖，默认 false */
  defaultExpanded?: boolean;
}

export function ContextFrameStream({
  frames,
  defaultExpanded = false,
}: ContextFrameStreamProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);

  if (frames.length === 0) {
    return null;
  }

  const summary = summarizeFrames(frames);
  const header = frames.some(
    (frame) => frame.delivery_status === "applied_to_compacted_context",
  )
    ? "上下文已重建"
    : "上下文已更新";

  return (
    <div>
      <DisclosureRow
        expanded={expanded}
        onClick={() => setExpanded((v) => !v)}
        className={ST.groupRow}
      >
        <span className={ST.badge}>CTX</span>
        <span className={ST.hint}>
          {header} {describeFrameSet(frames)} {summary ? `· ${summary}` : ""}
        </span>
      </DisclosureRow>

      {expanded && (
        <div className={ST.itemList}>
          {frames.map((frame) => (
            <FrameStripItem key={frame.id} frame={frame} defaultOpen={defaultExpanded} />
          ))}
        </div>
      )}
    </div>
  );
}

/** 每个 frame 渲染为一条 strip 行，结构对齐 ToolCallCardShell */
function FrameStripItem({
  frame,
  defaultOpen = false,
}: {
  frame: ContextFrame;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const token = frameKindToToken(frame.kind);
  const label = frameTabLabel(frame);

  return (
    <div>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={`${ST.itemRow} ${open ? "bg-secondary/30" : ""}`}
      >
        <span className={`${ST.dot} bg-success`} />
        <span className={ST.badge}>{token.token}</span>
        <span className={ST.title}>{label}</span>
        <span className={ST.hint}>
          {frame.delivery_status}
        </span>
      </button>
      {open && (
        <div className={ST.bodyArea}>
          <ContextFrameBody frame={frame} />
        </div>
      )}
    </div>
  );
}

function summarizeFrames(frames: ContextFrame[]): string {
  const last = frames[frames.length - 1]!;
  if (frames.length === 1) {
    return last.kind;
  }
  return `${frames.length}x · ${last.kind}`;
}

const FRAME_KIND_LABELS: Record<string, string> = {
  identity: "IDENTITY",
  capability_state_delta: "CAPABILITY",
  assignment_context: "ASSIGNMENT",
  compaction_summary: "COMPACTION",
  system_guidelines: "GUIDELINES",
  memory_context: "MEMORY",
  user_context: "USER",
  environment: "ENVIRONMENT",
};

function describeFrameSet(frames: ContextFrame[]): string {
  const labels = frames
    .map(frameKindLabel)
    .filter((label, index, all) => all.indexOf(label) === index)
    .slice(0, 4);
  return labels.join(" / ");
}

function frameKindLabel(frame: ContextFrame): string {
  if (frame.kind === "capability_state_delta") {
    return runtimeSurfaceFrameLabel(frame);
  }
  return FRAME_KIND_LABELS[frame.kind] ?? frame.kind.toUpperCase();
}

/**
 * 单个 frame tab 上的文字描述：优先展示阶段/关键变化，退化为 kind。
 *
 * - capability_state_delta：展示能力/工具统计
 * - compaction_summary：展示压缩条数
 * - 其他：kind
 */
function frameTabLabel(frame: ContextFrame): string {
  const parts: string[] = [];
  parts.push(frame.kind);

  if (frame.kind === "capability_state_delta") {
    const diff = summarizeRuntimeUpdate(frame);
    if (diff) parts.push(diff);
  } else if (frame.kind === "compaction_summary") {
    const compaction = frame.sections.find(
      (section) => section.kind === "compaction_summary",
    );
    if (compaction && compaction.kind === "compaction_summary") {
      parts.push(`${compaction.messages_compacted} msg`);
    }
  } else if (frame.kind === "memory_context") {
    const memory = frame.sections.find((section) => section.kind === "memory_inventory");
    if (memory && memory.kind === "memory_inventory") {
      parts.push(`${memory.sources.length} sources`);
    }
  }

  return parts.join(" · ");
}

function summarizeRuntimeUpdate(frame: ContextFrame): string | null {
  let added = 0;
  let removed = 0;
  let changed = 0;
  for (const section of frame.sections) {
    if (section.kind === "capability_key_delta") {
      added += section.added_capabilities.length;
      removed += section.removed_capabilities.length;
    } else if (section.kind === "tool_path_delta") {
      added += section.unblocked_tool_paths.length + section.whitelisted_tool_paths.length;
      removed += section.blocked_tool_paths.length + section.removed_whitelist_paths.length;
    } else if (section.kind === "mcp_server_delta") {
      added += section.added_mcp_servers.length;
      removed += section.removed_mcp_servers.length;
      changed += section.changed_mcp_servers.length;
    } else if (section.kind === "vfs_delta") {
      added += section.vfs_mounts_added.length;
      removed += section.vfs_mounts_removed.length;
    } else if (section.kind === "tool_schema_delta") {
      added += section.added_tools.length;
      removed += section.removed_tools.length;
      changed += section.changed_tools.length;
    } else if (section.kind === "skill_delta") {
      added += section.added_skills.length;
      removed += section.removed_skills.length;
      changed += section.changed_skills.length;
    } else if (section.kind === "memory_inventory" && section.mode === "delta") {
      added += section.added_sources.length;
      removed += section.removed_sources.length;
      changed += section.changed_sources.length;
    } else if (section.kind === "companion_agent_roster_delta") {
      added += section.added_agents.length;
      removed += section.removed_agent_keys.length;
      changed += section.changed_agents.length;
    }
  }
  if (added + removed + changed === 0) return null;
  const tokens: string[] = [];
  if (added > 0) tokens.push(`+${added}`);
  if (removed > 0) tokens.push(`−${removed}`);
  if (changed > 0) tokens.push(`↻${changed}`);
  return tokens.join(" ");
}

function runtimeSurfaceFrameLabel(frame: ContextFrame): string {
  const sectionKinds = new Set(frame.sections.map((section) => section.kind));
  const capabilitySection = frame.sections.find(
    (section) => section.kind === "capability_key_delta",
  );
  const hasCapabilityKeyDelta =
    capabilitySection?.kind === "capability_key_delta" &&
    capabilitySection.added_capabilities.length + capabilitySection.removed_capabilities.length > 0;
  if (hasCapabilityKeyDelta) return "CAPABILITY DELTA";
  if (sectionKinds.size === 1) {
    if (sectionKinds.has("skill_delta")) return "SKILL UPDATE";
    if (sectionKinds.has("memory_inventory")) return "MEMORY UPDATE";
    if (sectionKinds.has("vfs_delta")) return "VFS UPDATE";
    if (sectionKinds.has("mcp_server_delta")) return "MCP UPDATE";
    if (sectionKinds.has("tool_schema_delta")) return "TOOL SURFACE";
    if (sectionKinds.has("companion_agent_roster_delta")) return "COMPANION UPDATE";
  }
  return "CAPABILITY";
}

export default ContextFrameStream;
