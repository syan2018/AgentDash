/**
 * ContextFrame 外层 shell
 *
 * 单帧 / 多帧 context_frame 事件统一经此渲染：
 *
 * - header 行：CTX badge + "N 帧 · 最后阶段 X" 汇总 + ▲▼
 * - 展开后：横向 frame tab 条（单帧时等效 pill label）+ 对应 frame body
 *
 * 所有 frame 数据是 model 层已解析的 `ContextFrame`。UI 只负责展示。
 */

import { useState } from "react";
import type { ContextFrame } from "../model/contextFrame";
import { frameKindToToken } from "../model/contextFrame";
import { ContextFrameBody } from "./ContextFrameBody";
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

  return (
    <div>
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className={ST.groupRow}
      >
        <span className={ST.chevron}>{expanded ? "▼" : "▶"}</span>
        <span className={ST.badge}>CTX</span>
        <span className={ST.hint}>
          上下文已更新 {describeFrameSet(frames)} {summary ? `· ${summary}` : ""}
        </span>
      </button>

      {expanded && (
        <div className={ST.itemList}>
          {frames.map((frame) => (
            <FrameStripItem key={frame.id} frame={frame} />
          ))}
        </div>
      )}
    </div>
  );
}

/** 每个 frame 渲染为一条 strip 行，结构对齐 ToolCallCardShell */
function FrameStripItem({ frame }: { frame: ContextFrame }) {
  const [open, setOpen] = useState(false);
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
  const phase = last.phase_node ?? last.kind;
  if (frames.length === 1) {
    return last.phase_node ?? "";
  }
  return `${frames.length}x · ${phase}`;
}

const FRAME_KIND_LABELS: Record<string, string> = {
  identity: "IDENTITY",
  continuation_context: "CONTINUATION",
  capability_state_snapshot: "CAPABILITY SNAPSHOT",
  capability_state_delta: "CAPABILITY DELTA",
  assignment_context: "ASSIGNMENT",
  pending_action: "ACTION",
  auto_resume: "RESUME",
  compaction_summary: "COMPACTION",
  system_guidelines: "GUIDELINES",
};

function describeFrameSet(frames: ContextFrame[]): string {
  const kindSet = new Set(frames.map((f) => f.kind));
  const labels = [...kindSet]
    .map((kind) => FRAME_KIND_LABELS[kind] ?? kind.toUpperCase())
    .slice(0, 4);
  return labels.join(" / ");
}

/**
 * 单个 frame tab 上的文字描述：优先展示阶段/关键变化，退化为 kind。
 *
 * - capability_state_snapshot / capability_state_delta：展示能力/工具统计
 * - auto_resume：展示 reason
 * - compaction_summary：展示压缩条数
 * - 其他：phase_node 或 kind
 */
function frameTabLabel(frame: ContextFrame): string {
  const parts: string[] = [];
  if (frame.phase_node) parts.push(frame.phase_node);
  else parts.push(frame.kind);

  if (
    frame.kind === "capability_state_snapshot" ||
    frame.kind === "capability_state_delta"
  ) {
    const diff = summarizeRuntimeUpdate(frame);
    if (diff) parts.push(diff);
  } else if (frame.kind === "auto_resume") {
    const resume = frame.sections.find((section) => section.kind === "auto_resume");
    if (resume && resume.kind === "auto_resume" && resume.reason) {
      parts.push(resume.reason);
    }
  } else if (frame.kind === "compaction_summary") {
    const compaction = frame.sections.find(
      (section) => section.kind === "compaction_summary",
    );
    if (compaction && compaction.kind === "compaction_summary") {
      parts.push(`${compaction.messages_compacted} msg`);
    }
  } else if (frame.kind === "system_guidelines") {
    const preferences = frame.sections.find((section) => section.kind === "user_preferences");
    const guidelines = frame.sections.find((section) => section.kind === "project_guidelines");
    const prefCount = preferences && preferences.kind === "user_preferences" ? preferences.items.length : 0;
    const fileCount = guidelines && guidelines.kind === "project_guidelines" ? guidelines.entries.length : 0;
    if (prefCount + fileCount > 0) parts.push(`${prefCount} prefs / ${fileCount} files`);
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
    } else if (section.kind === "skill_delta") {
      added += section.added_skills.length;
      removed += section.removed_skills.length;
      changed += section.changed_skills.length;
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

export default ContextFrameStream;
