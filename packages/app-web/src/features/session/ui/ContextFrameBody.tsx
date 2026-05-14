/**
 * ContextFrame 内层 body
 *
 * 展示单个已解析的 ContextFrame：严格按 `frame.sections[]` 原顺序单列平铺
 * section，末尾附加 "Agent 实际原文" 与 "调试信息" 两个折叠块。
 *
 * 设计原则见 PRD 「所见即所得（WYSIWYG-for-Agent）」：不排序、不跳过、
 * 不按重要性合并；section 自身不再独立折叠（唯一例外是 tool_schema 单项的
 * parameters JSON）。
 */

import { useState } from "react";
import type { ContextFrame } from "../model/contextFrame";
import { Chip, SectionBlock } from "./contextFrame/SectionRenderers";

export interface ContextFrameBodyProps {
  frame: ContextFrame;
}

export function ContextFrameBody({ frame }: ContextFrameBodyProps) {
  return (
    <div className="space-y-2.5">
      {frame.sections.map((section, index) => (
        <SectionBlock key={`${section.kind}:${index}`} section={section} />
      ))}
      <AgentVisibleTextBlock text={frame.rendered_text} />
      <DebugBlock frame={frame} />
    </div>
  );
}

function AgentVisibleTextBlock({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const lineCount = text.length === 0 ? 0 : text.split(/\r?\n/).length;

  return (
    <div className="rounded-[8px] border border-border/70 bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-2.5 py-2 text-left hover:bg-secondary/35"
      >
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground/85">
          ▸ Agent 实际原文
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground/60">{lineCount} 行</span>
        <span className="shrink-0 text-[10px] text-muted-foreground/40">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <pre className="max-h-96 overflow-auto whitespace-pre-wrap border-t border-border/70 bg-secondary/15 p-2.5 text-xs leading-relaxed text-foreground/80">
          {text}
        </pre>
      )}
    </div>
  );
}

function DebugBlock({ frame }: { frame: ContextFrame }) {
  const [open, setOpen] = useState(false);
  const chips = [
    `kind: ${frame.kind}`,
    `source: ${frame.source}`,
    `channel: ${frame.delivery_channel}`,
    `role: ${frame.message_role}`,
    `delivery: ${frame.delivery_status}`,
    `sections: ${frame.sections.length}`,
    frame.phase_node ? `phase: ${frame.phase_node}` : null,
    frame.apply_mode ? `apply: ${frame.apply_mode}` : null,
  ].filter((chip): chip is string => chip != null);

  return (
    <div className="rounded-[8px] border border-border/70 bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-2.5 py-2 text-left hover:bg-secondary/35"
      >
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground/85">
          ▸ 调试信息
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground/40">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <div className="space-y-2 border-t border-border/70 bg-secondary/15 px-2.5 py-2">
          <div className="flex flex-wrap gap-1.5">
            {chips.map((chip) => (
              <Chip key={chip} label={chip} />
            ))}
          </div>
          <pre className="max-h-96 overflow-auto rounded-[6px] border border-border/70 bg-background p-2 text-[11px] leading-relaxed text-muted-foreground">
            {JSON.stringify(frame, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

export default ContextFrameBody;
