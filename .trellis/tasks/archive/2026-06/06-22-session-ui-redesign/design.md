# 会话界面统一设计语言

## 设计原则

文本流优先、渐进展开、视觉一致。所有工具调用、上下文帧、系统事件共享同一套外壳结构和内容样式 token。

## 双层 Token 体系

位于 `packages/app-web/src/features/session/ui/bodies/cardBodyTokens.ts`。

### ST (Shell Token) — 外壳结构

控制一级 group（TOOLS / CTX）和二级 item（单条 tool / frame）的标题栏、折叠行、徽标。

| Token | 用途 | 关键样式 |
|-------|------|----------|
| `groupRow` | 一级 group 折叠行 | `flex items-center gap-2 rounded-[6px] px-2 py-1.5 hover:bg-secondary/40` |
| `itemRow` | 二级 item 标题栏 | 同上但 `py-1` |
| `badge` | 无边框粗体徽标 | `text-[10px] font-semibold uppercase tracking-[0.08em]` |
| `chevron` | 折叠三角 | `text-muted-foreground/50` |
| `title` | 标题文字 | `truncate text-xs text-foreground/70` |
| `hint` | 摘要辅助文字 | `text-muted-foreground/60 text-xs` |
| `dot` | 状态圆点 | `h-1.5 w-1.5 rounded-[8px]` |
| `itemList` | 展开后 item 列表 | `ml-1 pl-2 border-l border-border/40` |
| `bodyArea` | 展开后 body 区 | `space-y-2 px-2 py-2` |

### CB (Content Body Token) — 内容渲染

控制 body 内部的代码块、条目、按钮、错误提示等。

| Token | 用途 |
|-------|------|
| `codeBlock` | 代码/终端输出/JSON 数据块 |
| `expandPanel` | strip 展开后 body 面板 |
| `inlineEntry` / `inlineEntryButton` | 内嵌可折叠条目 |
| `sectionTitle` | 分区标题 |
| `sectionGap` / `itemGap` | 分区间距 / 区内间距 |
| `meta` | 辅助 metadata 文本 |
| `lineNumber` | 行号 |
| `actionButton` | 内联操作按钮 |
| `expandToggle` | 展开/折叠文字 |
| `errorBlock` | 错误信息块 |
| `kindBadge` | kind/type badge |
| `diffAdded` / `diffRemoved` | diff 统计着色 |

## 对称结构

TOOLS 和 CTX 共享相同的层级模型：

```
一级 group: ▶ [BADGE] summary        ← ST.groupRow + ST.badge
└── 二级 items:                       ← ST.itemList
    ● [BADGE] title  hint  duration   ← ST.itemRow + ST.dot + ST.badge
    └── body                          ← ST.bodyArea + CB.*
```

## 状态区分

状态仅通过标题栏背景色和 dot 颜色区分，不引入嵌套卡片：

| 状态 | 标题栏背景 | Dot |
|------|-----------|-----|
| inProgress / pending | `bg-primary/5` | `bg-primary animate-pulse` |
| failed / declined | `bg-destructive/5` | `bg-destructive` |
| pendingApproval | `bg-warning/5` | `bg-warning animate-pulse` |
| completed (展开) | `bg-secondary/30` | `bg-success` |
| completed (折叠) | 无 | `bg-success` |
