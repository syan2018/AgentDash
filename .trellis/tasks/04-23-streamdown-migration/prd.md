# 前端 Markdown 渲染切换为 Streamdown

## Goal

将 ACP 会话消息的 markdown 渲染库从 `react-markdown` 完整替换为 [Vercel Streamdown](https://github.com/vercel/streamdown)，**直接替换不并存**，**完全对齐 streamdown 默认行为**（不保留任何自定义 component / 行为 patch）。收益：内建流式渲染支持、Shiki 高亮、rehype-harden 安全加固。

## Requirements

- 移除依赖 `react-markdown`、`remark-gfm`
- 新增依赖 `streamdown` 及全部官方 plugin（`code` / `math` / `mermaid` / `cjk`）
- 改造 [frontend/src/features/acp-session/ui/AcpMessageCard.tsx](frontend/src/features/acp-session/ui/AcpMessageCard.tsx) 的 `MarkdownRenderer`：
  - 用 `<Streamdown>` 替代 `<Markdown>`
  - 丢弃现有 4 个自定义 components（`pre` / `code` / `table` / `a`），全部交给 streamdown 默认
  - 透传 `isStreaming` → streamdown `isAnimating`
- 在 [frontend/src/styles/index.css](frontend/src/styles/index.css) 加 Tailwind 4 `@source` 指令，路径从 `frontend/src/styles/` 上溯到 `frontend/node_modules`：`@source "../../node_modules/streamdown/dist/*.js"`（plugin 同理各加一条）
- 按需 import `streamdown/styles.css` 与 `katex/dist/katex.min.css`
- 保留现有 `.agentdash-chat-markdown` 作用域 CSS（基于元素选择器命中 streamdown 默认 DOM，元素级样式继续生效）
- `.agentdash-chat-code-block` 这个 class 在 [AcpToolCallCard.tsx](frontend/src/features/acp-session/ui/AcpToolCallCard.tsx) / [task-drawer.tsx](frontend/src/features/task/task-drawer.tsx) 被其它模块直接使用，**不得删除该 class**（仅从 markdown 渲染路径上移除）
- `<file:xxx>` 自定义 pill 仅在 user 消息生效，本次不动

## Acceptance Criteria

- [ ] ACP 会话页 agent 回复的 Markdown（标题/列表/引用/代码块/表格/链接）可正常渲染
- [ ] 流式输出中未闭合的 ```` ``` ```` 代码块 / 半截表格 / 半截链接 `[text](http` 不渲染崩坏
- [ ] 代码块走 Shiki 默认主题，外观可接受（不强求与旧版一致）
- [ ] `package.json` 不再存在 `react-markdown` / `remark-gfm`
- [ ] `pnpm -F frontend check`（typecheck + lint + test）全绿
- [ ] 手动过一条包含 h1/h2/列表/代码块/表格/链接的真实 ACP 会话
- [ ] 控制台无 error / warning

## Definition of Done

- typecheck / lint / test 全绿
- 手动走查 ACP 会话页流式 + 静态两种场景
- 无 react-markdown 代码残留（包括注释、死代码）
- 提交信息使用中文

## Technical Approach

**采纳 Approach A（纯对齐 streamdown）**：不保留任何自定义 component，不补 `target="_blank"`、不补 `table` 横向滚动 wrapper。理由：原有 4 个自定义 component 中 2 个是冗余空壳（`pre` 加冗余 class / `code` 空壳），另 2 个（`target=_blank` 与 `table overflow`）没有经过特殊产品考量，streamdown 原始设计（含 rehype-harden 等安全加固）考虑得比项目内特设逻辑更周全，接受默认行为差异。

## Decision (ADR-lite)

**Context**：迁移时要不要保留当前 react-markdown 下的 4 个 component override？

**Decision**：全部丢弃，直接对齐 streamdown 默认。

**Consequences**：
- 可能行为差异：链接不再强制新 tab、长表格不再外层包 overflow-auto、代码块走 Shiki 主题
- 接受这些差异；若后续产品上明确需要，再单独提任务补
- 代码更干净，升级 streamdown 无 patch 负担

## Out of Scope

- 不补 `target="_blank"` rehype plugin
- 不补 table 横向滚动 wrapper
- 不做 markdown 深色模式专项适配
- 不扩展 `<file:xxx>` pill 到 agent 回复
- 不动 tool call / task drawer 中 `.agentdash-chat-code-block` 的既有非 markdown 用法

## Implementation Plan

单 PR 即可，步骤：

1. 依赖变更：`pnpm -F frontend add streamdown`、同步添加其 plugin 包（以官方 README 为准）；从 `package.json` 移除 `react-markdown` 与 `remark-gfm`
2. 样式接入：[frontend/src/styles/index.css](frontend/src/styles/index.css) 加 `@source` 指令；在入口文件 import `streamdown/styles.css` 和 `katex/dist/katex.min.css`
3. 改造 [AcpMessageCard.tsx](frontend/src/features/acp-session/ui/AcpMessageCard.tsx)：重写 `MarkdownRenderer`，引入 `<Streamdown animated isAnimating={isStreaming} plugins={{ code, math, mermaid, cjk }}>`；从 `AcpMessageCard` 透传 `isStreaming` 到 `MarkdownRenderer`
4. 运行 `pnpm -F frontend check`，修复 type/lint/test
5. 手动走查一条真实 ACP 会话（含代码块、表格、链接、列表）

## Technical Notes

- 唯一改造文件：[frontend/src/features/acp-session/ui/AcpMessageCard.tsx](frontend/src/features/acp-session/ui/AcpMessageCard.tsx)
- 样式入口：[frontend/src/styles/index.css](frontend/src/styles/index.css)
- React 19 / Tailwind 4 / Vite 7 / pnpm + Windows 平台
- 参考：<https://github.com/vercel/streamdown>
- streamdown `plugins` 对象式 API，非 react-markdown 的 remark/rehype 数组
- 流式关键点：`isAnimating={isStreaming}` 透传，streamdown 内部 `parseIncompleteMarkdown` 自动处理未闭合 token
