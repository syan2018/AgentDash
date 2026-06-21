# 会话界面重构执行记录

## Phase 1 — 外壳重构 ✅

commit: `295d4f6e` refactor(session-ui): 会话界面文本流优先设计重构

- Agent 消息去卡片化，直出 markdown 文档流
- 用户消息右侧气泡样式
- ToolCallCardShell 双模式（strip / 轻量高亮卡片）
- 工具聚合无条件折叠 + agent 消息到达后自动收起
- EventStripCard / ContextFrameStream 去边框改 inline
- TurnSegment 轮次折叠
- SessionChatStatusBar 恢复原始设计
- 间距收窄 space-y-3 → space-y-1.5

## Phase 2 — Card Body 统一设计语言 ✅

### 2.1 Token 体系建立 ✅

commit: `49d58ef7` style(session): 统一 card body 设计 token 体系

- 创建 `cardBodyTokens.ts`，导出 CB token 常量
- CommandExecutionCardBody → CB.codeBlock / CB.actionButton
- ReadCardBody → CB.inlineEntry / CB.lineNumber / CB.actionButton
- FileChangeCardBody → CB.inlineEntry / CB.kindBadge
- McpCardBody + GenericJsonBody → CB.sectionTitle / CB.errorBlock
- ContextFrameBody → CB.sectionGap / CB.meta / CB.codeBlock
- WebSearchCardBody / CollabAgentCardBody / ImageCardBody → CB.*

### 2.2 结构统一 ✅

commit: `e0ef0549` refactor(session): 统一 ToolCallCardShell / CTX / EventCards 展示结构

- ToolCallCardShell：strip/card 合并为单一渲染路径，状态仅通过背景色区分
- ContextFrameStream：从 tab 面板转为 expandable strip items，与 AggregatedToolGroupEntry 对称
- EventCards：展开区去嵌套卡片化，使用 CB token

### 2.3 Shell Token 与徽标标准化 ✅

commit: `bff211a0` style(session): 提取 Shell Token (ST) 统一一级/二级徽标样式

- 引入 ST (Shell Token) 层
- AggregatedToolGroupEntry / ToolCallCardShell / ContextFrameStream 共用 ST.groupRow / ST.itemRow / ST.badge
- 无边框粗体徽标统一替代原 bordered badge

### 2.4 剩余 Body 组件清扫 ✅

commit: `ae7e827e` fix(session): 修复 cwd 重复绘制 + 统一剩余 body 组件样式

- CommandExecutionCardBody：移除 body 内 cwd 重复绘制
- DiffCardBody → CB.itemGap / CB.meta / CB.diffAdded / CB.diffRemoved / CB.lineNumber
- ToolOutputContentViewer → CB.itemGap / CB.actionButton / CB.codeBlock / CB.kindBadge
- JsonTree CopyJsonButton → CB.actionButton

### 2.5 SectionRenderers 去卡片化 ✅

commit: `0196663a` style(session): SectionRenderers 去卡片化 + TaskToolCardBody 空状态处理

- Chip 统一使用 CB.kindBadge
- SectionBlock 去 card 包裹
- TokenBadge 改为无边框粗体风格

### 2.6 TaskToolCardBody + ToolSchemaDelta ✅

commit: `8dd0568e` style(session): TaskToolCardBody 与 ToolSchemaDelta 设计语言统一

- TaskToolCardBody Overview：空状态 → CB.meta "暂无任务"，有进度 → progress bar + CB.meta
- ToolSchemaDelta：+ 号标识新增、CB.meta 参数计数、展开用 JsonTree

### 2.7 收尾 ✅

- ContextCompactionCardBody → CB.meta
- TypeScript 编译通过
- Lint 无新增错误

## 完整覆盖组件清单

| 组件 | ST | CB | 状态 |
|------|----|----|------|
| ToolCallCardShell | ✅ | — | ✅ |
| SessionEntry (AggregatedToolGroupEntry) | ✅ | — | ✅ |
| ContextFrameStream | ✅ | — | ✅ |
| ContextFrameBody | — | ✅ | ✅ |
| EventCards | — | ✅ | ✅ |
| SectionRenderers | — | ✅ | ✅ |
| CommandExecutionCardBody | — | ✅ | ✅ |
| ReadCardBody | — | ✅ | ✅ |
| FileChangeCardBody | — | ✅ | ✅ |
| DiffCardBody | — | ✅ | ✅ |
| McpCardBody | — | ✅ | ✅ |
| GenericJsonBody | — | ✅ | ✅ |
| WebSearchCardBody | — | ✅ | ✅ |
| CollabAgentCardBody | — | ✅ | ✅ |
| ImageCardBody | — | ✅ | ✅ |
| TaskToolCardBody | — | ✅ | ✅ |
| ToolOutputContentViewer | — | ✅ | ✅ |
| JsonTree | — | ✅ | ✅ |
| ContextCompactionCardBody | — | ✅ | ✅ |
| DynamicToolCallCardBody | — | — | ✅ (路由) |
