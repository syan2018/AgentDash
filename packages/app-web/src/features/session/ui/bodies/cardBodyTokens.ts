/**
 * 会话 UI 统一设计 token
 *
 * 分两层：
 * - ST (Shell Token)：一级 group / 二级 item 的标题栏、徽标、折叠行等外壳样式
 * - CB (Content Body Token)：body 渲染器内部的代码块、分区、按钮等内容样式
 *
 * 修改此处即可全局调整视觉。
 */

// ── Shell Token ──────────────────────────────────────────────────────────────

export const ST = {
  /** 一级 group 折叠行（TOOLS / CTX） */
  groupRow:
    "flex w-full items-center gap-2 rounded-[6px] px-2 py-1.5 text-left text-xs transition-colors hover:bg-secondary/40",
  /** 二级 item 标题栏（单条 tool / frame strip） */
  itemRow:
    "flex w-full items-center gap-2 rounded-[6px] px-2 py-1 text-left transition-colors hover:bg-secondary/40",

  /** 无边框粗体徽标 — 一级 group 和二级 item 共用 */
  badge:
    "shrink-0 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/60",

  /** 折叠三角 */
  chevron: "text-muted-foreground/50",

  /** 标题文字 */
  title: "min-w-0 flex-1 truncate text-xs text-foreground/70",
  /** 摘要/辅助文字 */
  hint: "shrink-0 text-muted-foreground/60 text-xs",

  /** 状态 dot */
  dot: "inline-block h-1.5 w-1.5 shrink-0 rounded-[8px]",

  /** 展开后的 item 列表容器（左边线 + 缩进） */
  itemList: "ml-1 space-y-0.5 pl-2 border-l border-border/40",
  /** 展开后的 body 区 */
  bodyArea: "space-y-2 px-2 py-2",
} as const;

// ── Content Body Token ───────────────────────────────────────────────────────

export const CB = {
  /** 代码/终端输出/JSON 数据块 */
  codeBlock:
    "rounded-[6px] bg-muted/12 px-2.5 py-2 font-mono text-xs leading-relaxed text-foreground/80",

  /** strip 展开后的 body 面板 */
  expandPanel:
    "rounded-[6px] border border-border/40 bg-secondary/10",

  /** 内嵌可折叠条目（文件条目、CTX section 等） */
  inlineEntry:
    "rounded-[6px] border border-border/30",
  inlineEntryButton:
    "flex w-full items-center gap-2 px-2 py-1.5 text-left text-xs transition-colors hover:bg-secondary/20",

  /** 分区标题 */
  sectionTitle:
    "text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground/50",

  /** 分区间距 */
  sectionGap: "space-y-2",
  /** 区内元素间距 */
  itemGap: "space-y-1",

  /** 辅助/metadata 文本 */
  meta: "text-[10px] text-muted-foreground/40",

  /** 行号 */
  lineNumber:
    "select-none px-2 text-right tabular-nums text-[10px] text-muted-foreground/30",

  /** 内联操作按钮 */
  actionButton:
    "rounded-[4px] px-1.5 py-0.5 text-[10px] text-muted-foreground/60 transition-colors hover:bg-secondary/40 hover:text-foreground",

  /** 展开/折叠文字 */
  expandToggle: "text-[10px] text-muted-foreground/40",

  /** 错误信息块 */
  errorBlock:
    "rounded-[6px] bg-destructive/5 px-2 py-1.5 text-xs text-destructive",

  /** 状态着色 */
  statusSuccess: "text-success",
  statusFailed: "text-destructive",
  statusWarning: "text-warning",
  statusNeutral: "text-muted-foreground/60",

  /** kind/type badge（文件类型、MCP 入参标签等） */
  kindBadge:
    "shrink-0 rounded-[4px] bg-secondary/40 px-1 py-px text-[9px] font-semibold text-muted-foreground/60",

  /** diff 统计 */
  diffAdded: "text-success",
  diffRemoved: "text-destructive",
} as const;
