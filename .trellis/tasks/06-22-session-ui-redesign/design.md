# Card Body 统一设计语言

## 设计目标

建立一套跨所有 card body 渲染器共享的视觉 token 体系，消除当前各 body 独立设计导致的不一致。

## 设计 Token 体系

### 容器层级

| 层级 | 用途 | 样式 |
|------|------|------|
| 代码/数据块 | 终端输出、代码预览、JSON 树 | `rounded-[6px] bg-muted/12 px-2.5 py-2 font-mono text-xs leading-relaxed` |
| 展开面板 | strip 展开后的 body 区域 | `rounded-[6px] border border-border/40 bg-secondary/10` |
| 内嵌条目 | FileChange 文件条目、CTX section | `rounded-[6px] border border-border/30 hover:bg-secondary/20` |

### 文本层级

| 元素 | 样式 |
|------|------|
| 分区标题 | `text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground/50` |
| 正文 | `text-xs text-foreground/80` |
| 辅助信息 | `text-[10px] text-muted-foreground/40` |
| 行号 | `text-[10px] tabular-nums text-muted-foreground/30` |

### 操作按钮

| 类型 | 样式 |
|------|------|
| 内联操作 | `rounded-[4px] px-1.5 py-0.5 text-[10px] text-muted-foreground/60 hover:bg-secondary/40 hover:text-foreground` |
| 展开/折叠 | `text-[10px] text-muted-foreground/40` |

### 状态着色

| 状态 | 文本色 | 背景色 |
|------|--------|--------|
| 成功 | `text-success` | `bg-success/5` |
| 失败 | `text-destructive` | `bg-destructive/5` |
| 警告 | `text-warning` | `bg-warning/5` |
| 中性 | `text-muted-foreground/60` | — |

### 错误块

统一为：`rounded-[6px] bg-destructive/5 px-2 py-1.5 text-xs text-destructive`

## CTX = TOOLS 对称结构

CTX stream 对齐 AggregatedToolGroupEntry：
- 一级：折叠行 `▶ 上下文已更新 3 帧`（同 tool group summary）
- 二级：每个 frame 用 strip 行展示（同 ToolCallCardShell strip 模式）
- body：展开后 section 样式和 tool body section 完全对齐

## 间距规范

- body 内分区间距：`space-y-2`
- 分区内元素间距：`space-y-1`
- 代码块内 padding：`px-2.5 py-2`
- metadata footer：`mt-1.5`
