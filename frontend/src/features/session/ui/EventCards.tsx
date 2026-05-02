/**
 * 通用事件卡片模板原语
 *
 * 两种形态共用同一套视觉骨架：
 *
 *   ┌─────────────────────────────────────────┐
 *   │ [badge]  主文字 / 副标题        hint ▲▼ │  ← header 行，px-3 py-2.5
 *   ├─────────────────────────────────────────┤
 *   │  展开区 / body 内容                     │  ← px-3 py-2.5，border-t
 *   └─────────────────────────────────────────┘
 *
 * EventStripCard  — header 行即全部，展开区显示补充内容
 * EventFullCard   — header 行 = badge + 主消息，body 默认展开（无折叠）
 *
 * 对齐保证：两者 badge 左边距相同（px-3），垂直对齐相同（items-center / items-start）
 *
 * 扩展点：
 *   - headerExtra：注入到 header 行 badge+label 之后、hint 之前的额外节点
 *   - bodyExtra  ：注入到展开区末尾的额外节点（自定义 tag 行、审批按钮等）
 *
 * 样式原则：badge 是唯一染色点，外框和文字保持中性色。
 */

import { useState } from "react";

// ─── 共用 badge class 常量（导出供调用方使用）────────────────────────────────

export const BADGE = {
  neutral: "border-border bg-secondary/50 text-muted-foreground",
  primary: "border-primary/25 bg-primary/8 text-primary",
  success: "border-success/25 bg-success/10 text-success",
  warning: "border-warning/25 bg-warning/10 text-warning",
  error:   "border-destructive/25 bg-destructive/10 text-destructive",
} as const;

export type BadgeVariant = keyof typeof BADGE;

// ─── EventStripCard ───────────────────────────────────────────────────────────

export interface StripExpandContent {
  sections?: Array<{
    title?: string;
    lines: string[];
  }>;
  raw?: string;
}

export interface EventStripCardProps {
  badgeToken: string;
  badgeClass?: string;
  label: string;
  /** 右侧小字辅助信息（规则数、满足状态等） */
  rightHint?: string | null;
  /** header 行 label 之后、rightHint 之前，注入额外节点（tag、状态点等） */
  headerExtra?: React.ReactNode;
  expandContent?: StripExpandContent;
  /** 展开区末尾追加自定义内容 */
  bodyExtra?: React.ReactNode;
}

export function EventStripCard({
  badgeToken,
  badgeClass = BADGE.neutral,
  label,
  rightHint,
  headerExtra,
  expandContent,
  bodyExtra,
}: EventStripCardProps) {
  const [expanded, setExpanded] = useState(false);

  const hasExpand = Boolean(bodyExtra) || (
    expandContent != null && (
      expandContent.sections?.some((s) => s.lines.length > 0) ||
      Boolean(expandContent.raw)
    )
  );

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      {/* header 行 */}
      <button
        type="button"
        onClick={() => hasExpand && setExpanded((v) => !v)}
        className={`flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors ${hasExpand ? "cursor-pointer hover:bg-secondary/35" : "cursor-default"}`}
      >
        <span className={`inline-flex shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] ${badgeClass}`}>
          {badgeToken}
        </span>
        <span className="min-w-0 flex-1 truncate text-sm text-foreground/80">
          {label}
        </span>
        {headerExtra}
        {rightHint && (
          <span className="shrink-0 text-[10px] text-muted-foreground/50">
            {rightHint}
          </span>
        )}
        {hasExpand && (
          <span className="shrink-0 text-[10px] text-muted-foreground/40">
            {expanded ? "▲" : "▼"}
          </span>
        )}
      </button>

      {/* 展开区 */}
      {expanded && hasExpand && (
        <div className="border-t border-border px-3 py-2.5 space-y-2.5">
          {expandContent?.sections?.map((section, i) =>
            section.lines.length > 0 ? (
              <div key={i} className="space-y-1">
                {section.title && (
                  <p className="text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60">
                    {section.title}
                  </p>
                )}
                {section.lines.map((line, j) => (
                  <p key={j} className="text-xs leading-5 text-foreground/75">{line}</p>
                ))}
              </div>
            ) : null
          )}
          {expandContent?.raw && (
            <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-xs leading-relaxed text-foreground/75">
              {expandContent.raw}
            </pre>
          )}
          {bodyExtra}
        </div>
      )}
    </div>
  );
}

// ─── EventFullCard ────────────────────────────────────────────────────────────

export interface EventFullCardProps {
  badgeToken: string;
  badgeClass?: string;
  /** badge 右侧第一行副标题，中性色，可选 */
  subtitle?: string;
  message: string;
  /** header 行 subtitle 之后注入额外节点（tag、状态 dot 等） */
  headerExtra?: React.ReactNode;
  detailLines?: string[];
  /** body 区末尾追加自定义内容（审批按钮等主要操作） */
  bodyExtra?: React.ReactNode;
  debugChips?: string[];
  debugLines?: string[];
  debugRaw?: string;
  /** 折叠详情区末尾追加自定义内容（diagnostics 等次要详情） */
  debugBody?: React.ReactNode;
}

export function EventFullCard({
  badgeToken,
  badgeClass = BADGE.neutral,
  subtitle,
  message,
  headerExtra,
  detailLines = [],
  bodyExtra,
  debugChips = [],
  debugLines = [],
  debugRaw,
  debugBody,
}: EventFullCardProps) {
  const [showDebug, setShowDebug] = useState(false);
  const hasDebug = debugChips.length > 0 || debugLines.length > 0 || Boolean(debugRaw) || Boolean(debugBody);
  const hasSubRow = subtitle || headerExtra;

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      {/*
        header 行：与 StripCard 骨架完全对齐。
        有 debug 内容时整行可点击，右侧显示 ▲▼；否则为普通 div。
      */}
      {hasDebug ? (
        <button
          type="button"
          onClick={() => setShowDebug((v) => !v)}
          className={`flex w-full gap-2.5 px-3 py-2.5 text-left transition-colors hover:bg-secondary/35 ${hasSubRow ? "items-start" : "items-center"}`}
        >
          <span className={`${hasSubRow ? "mt-px" : ""} inline-flex shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] ${badgeClass}`}>
            {badgeToken}
          </span>
          {hasSubRow ? (
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2 mb-0.5">
                {subtitle && <span className="text-xs text-muted-foreground">{subtitle}</span>}
                {headerExtra}
              </div>
              <p className="text-sm leading-6 text-foreground/90">{message}</p>
            </div>
          ) : (
            <p className="min-w-0 flex-1 text-sm leading-6 text-foreground/90">{message}</p>
          )}
          <span className="shrink-0 self-center text-[10px] text-muted-foreground/40">
            {showDebug ? "▲" : "▼"}
          </span>
        </button>
      ) : (
        <div className={`flex gap-2.5 px-3 py-2.5 ${hasSubRow ? "items-start" : "items-center"}`}>
          <span className={`${hasSubRow ? "mt-px" : ""} inline-flex shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] ${badgeClass}`}>
            {badgeToken}
          </span>
          {hasSubRow ? (
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2 mb-0.5">
                {subtitle && <span className="text-xs text-muted-foreground">{subtitle}</span>}
                {headerExtra}
              </div>
              <p className="text-sm leading-6 text-foreground/90">{message}</p>
            </div>
          ) : (
            <p className="min-w-0 flex-1 text-sm leading-6 text-foreground/90">{message}</p>
          )}
        </div>
      )}

      {/* body 区：detailLines + bodyExtra，始终可见（border-t 仅在有内容时出现） */}
      {(detailLines.length > 0 || bodyExtra) && (
        <div className="border-t border-border px-3 py-2.5 space-y-2.5">
          {detailLines.length > 0 && (
            <div className="space-y-1">
              {detailLines.map((line) => (
                <p key={line} className="text-xs leading-5 text-muted-foreground">{line}</p>
              ))}
            </div>
          )}
          {bodyExtra}
        </div>
      )}

      {/* 折叠详情区：与 StripCard 展开区完全一致的结构 */}
      {hasDebug && showDebug && (
        <div className="border-t border-border px-3 py-2.5 space-y-2.5">
          {debugChips.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {debugChips.map((chip) => (
                <span
                  key={chip}
                  className="rounded-[6px] border border-border bg-secondary/35 px-1.5 py-0.5 text-[10px] text-muted-foreground/60"
                >
                  {chip}
                </span>
              ))}
            </div>
          )}
          {debugLines.length > 0 && (
            <div className="space-y-1 rounded-[10px] border border-border/70 bg-secondary/20 px-3 py-2.5">
              {debugLines.map((line) => (
                <p key={line} className="text-xs leading-5 text-muted-foreground">{line}</p>
              ))}
            </div>
          )}
          {debugRaw && (
            <pre className="overflow-auto rounded-[10px] border border-border/70 bg-secondary/20 p-3 text-xs text-muted-foreground">
              {debugRaw}
            </pre>
          )}
          {debugBody}
        </div>
      )}
    </div>
  );
}
