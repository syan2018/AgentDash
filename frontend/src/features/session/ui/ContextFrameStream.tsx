/**
 * ContextFrame 外层 shell
 *
 * 单帧 / 多帧 context_frame 事件统一经此渲染：
 *
 * - header 行：CTX badge + "N 帧 · 最后阶段 X" 汇总 + ▲▼
 * - 展开后：横向 frame tab 条（单帧时等效 pill label）+ 对应 frame body
 *
 * 所有 frame 数据是已解析的 `ContextFrame`。SessionEntry 负责从 platform
 * event 提取并解析，再调用本组件。
 */

import { useMemo, useState } from "react";
import { BADGE } from "./EventCards";
import type { ContextFrame } from "../model/contextFrame";
import { frameKindToToken } from "../model/contextFrame";
import { ContextFrameBody } from "./ContextFrameBody";
import { TokenBadge } from "./contextFrame/SectionRenderers";

export interface ContextFrameStreamProps {
  frames: ContextFrame[];
  /** 默认激活的 frame id；用于测试或持久化恢复，未指定时使用首帧 */
  defaultActiveFrameId?: string;
  /** 默认是否展开外层 shell；测试或持久化可覆盖，默认 false */
  defaultExpanded?: boolean;
}

export function ContextFrameStream({
  frames,
  defaultActiveFrameId,
  defaultExpanded = false,
}: ContextFrameStreamProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);

  const initialActive = useMemo(() => {
    if (frames.length === 0) return null;
    if (defaultActiveFrameId) {
      const match = frames.find((frame) => frame.id === defaultActiveFrameId);
      if (match) return match.id;
    }
    return frames[0]!.id;
  }, [frames, defaultActiveFrameId]);

  const [activeId, setActiveId] = useState<string | null>(initialActive);

  if (frames.length === 0) {
    return null;
  }

  const activeFrame =
    frames.find((frame) => frame.id === activeId) ?? frames[0]!;
  const summary = summarizeFrames(frames);

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors cursor-pointer hover:bg-secondary/35"
      >
        <span
          className={`inline-flex shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] ${BADGE.neutral}`}
        >
          CTX
        </span>
        <span className="min-w-0 flex-1 truncate text-sm text-foreground/85">
          {describeFrameSet(frames)}
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground/60">{summary}</span>
        <span className="shrink-0 text-[10px] text-muted-foreground/40">
          {expanded ? "▲" : "▼"}
        </span>
      </button>

      {expanded && (
        <div className="border-t border-border">
          <FrameTabBar
            frames={frames}
            activeId={activeFrame.id}
            onSelect={setActiveId}
          />
          <div className="border-t border-border px-3 py-2.5">
            <ContextFrameBody frame={activeFrame} />
          </div>
        </div>
      )}
    </div>
  );
}

function FrameTabBar({
  frames,
  activeId,
  onSelect,
}: {
  frames: ContextFrame[];
  activeId: string;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="flex flex-wrap gap-1.5 px-3 py-2">
      {frames.map((frame) => {
        const token = frameKindToToken(frame.kind);
        const active = frame.id === activeId;
        return (
          <button
            key={frame.id}
            type="button"
            onClick={() => onSelect(frame.id)}
            className={`inline-flex items-center gap-1.5 rounded-[6px] border px-2 py-1 text-[11px] transition-colors ${
              active
                ? "border-border bg-secondary/50 text-foreground"
                : "border-border/70 bg-background text-muted-foreground hover:bg-secondary/35"
            }`}
          >
            <TokenBadge token={token} />
            <span className="truncate max-w-[18rem]">{frameTabLabel(frame)}</span>
          </button>
        );
      })}
    </div>
  );
}

function summarizeFrames(frames: ContextFrame[]): string {
  const last = frames[frames.length - 1]!;
  const phase = last.phase_node ? `最后阶段 ${last.phase_node}` : last.kind;
  if (frames.length === 1) {
    return last.phase_node ? `阶段 ${last.phase_node}` : last.kind;
  }
  return `${frames.length} 帧 · ${phase}`;
}

const FRAME_KIND_LABELS: Record<string, string> = {
  identity: "身份与偏好",
  assignment_context: "任务分派",
  capability_state_update: "能力状态",
  continuation_context: "会话续跑",
  pending_action: "待执行动作",
  auto_resume: "自动续跑",
  compaction_summary: "上下文压缩",
};

function describeFrameSet(frames: ContextFrame[]): string {
  if (frames.length === 1) {
    const frame = frames[0]!;
    return FRAME_KIND_LABELS[frame.kind] ?? "上下文已更新";
  }
  const kindSet = new Set(frames.map((f) => f.kind));
  const labels = [...kindSet]
    .map((kind) => FRAME_KIND_LABELS[kind] ?? kind)
    .slice(0, 3);
  return labels.join(" + ");
}

/**
 * 单个 frame tab 上的文字描述：优先展示阶段/关键变化，退化为 kind。
 *
 * - capability_state_update：展示能力/工具 delta 的增减统计
 * - auto_resume：展示 reason
 * - compaction_summary：展示压缩条数
 * - 其他：phase_node 或 kind
 */
function frameTabLabel(frame: ContextFrame): string {
  const parts: string[] = [];
  if (frame.phase_node) parts.push(frame.phase_node);
  else parts.push(frame.kind);

  if (frame.kind === "capability_state_update") {
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
      parts.push(`${compaction.messages_compacted} 条`);
    }
  }

  return parts.join(" · ");
}

function summarizeRuntimeUpdate(frame: ContextFrame): string | null {
  let added = 0;
  let removed = 0;
  let changed = 0;
  for (const section of frame.sections) {
    if (section.kind === "capability_delta") {
      added +=
        section.added_capabilities.length +
        section.unblocked_tool_paths.length +
        section.whitelisted_tool_paths.length +
        section.added_mcp_servers.length +
        section.vfs_mounts_added.length;
      removed +=
        section.removed_capabilities.length +
        section.blocked_tool_paths.length +
        section.removed_whitelist_paths.length +
        section.removed_mcp_servers.length +
        section.vfs_mounts_removed.length;
      changed += section.changed_mcp_servers.length;
    } else if (section.kind === "tool_schema_delta") {
      // CAP 已经统计过工具路径级的增减;这里只统计 TOOL 真正新增的 schema,避免双计数。
      added += section.added_tools.length;
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
