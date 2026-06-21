# 会话界面设计重构

## Goal

将会话界面从"卡片堆叠"转向"文本流优先"设计，提升信息密度和阅读流畅度，同时保留详情展开能力。

## Phase 1 — 外壳重构（已完成）

- Agent 消息去卡片化，直出 markdown 文档流
- 用户消息右侧气泡样式
- ToolCallCardShell 双模式：completed → inline strip / running·failed·approval → 轻量高亮卡片
- 工具聚合无条件折叠，agent 消息到达后自动收起
- EventStripCard / ContextFrameStream 去边框，改 inline
- TurnSegment 分段 + TurnSection 轮次折叠
- 间距收窄（space-y-3 → space-y-1.5）

## Phase 2 — Card Body 统一设计语言（进行中）

### 问题

当前各 card body 渲染器（CommandExecution, FileChange, Read, DynamicToolCall, Mcp, ContextFrame, Plan, SystemEvent）各自独立设计，存在：
- 成功/失败样式不统一：strip 和 card 模式间的视觉断层
- ContextFrame 作为一级卡片与 TOOL 系列无对称结构
- 各 body 内部间距、字体、颜色体系不统一

### 目标

- 建立统一的 card body 设计 token 体系
- 所有 body 遵循同一套视觉规范：圆角、边框深度、间距、字号
- CTX 与 TOOLS 统一为同一层级结构，CTX 内部 item 与 TOOL item 视觉对齐
- 成功/失败/运行中状态在 strip 和 card 模式间保持视觉连贯

## Acceptance Criteria

- [ ] 所有 card body 使用统一的设计 token
- [ ] strip 模式和 card 模式的状态样式视觉连贯
- [ ] CTX 与 TOOLS 使用对称的一级/二级结构
- [ ] TypeScript 编译无错误
- [ ] Lint 无新增 warning
