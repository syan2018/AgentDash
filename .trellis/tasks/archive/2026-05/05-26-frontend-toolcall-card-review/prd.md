# 前端工具调用卡片信息架构重构 review

## 背景

后端工具事件事实源已经由 `05-26-backend-tool-event-source-convergence` 收束到
Backbone `AgentDashThreadItem`：

- Codex Protocol 已有的一等 item 直接复用 Codex `ThreadItem` wire shape。
- AgentDash 只在 Codex 尚未覆盖的工具事实上做加法扩展，当前 native item 为
  `fsRead` / `fsGrep` / `fsGlob`。
- `shell_exec` 映射为 Codex `commandExecution`。
- `fs_apply_patch` 映射为 Codex `fileChange`，让文件修改统一进入 Codex patch/file
  change 语义。
- vibe-kanban `ActionType` 只存在于 legacy adapter 边界，不进入前端 renderer 模型。

因此，前端工具卡片后续不再围绕 connector 私有语义或旧 normalized log 做分发，而是
直接消费 generated `backbone-protocol.ts` 中的 `AgentDashThreadItem` union。

当前 UI 仍存在信息架构问题：折叠态缺少请求摘要，展开态主要依赖大段 JSON 文本，
kind 元数据散落在多个位置，旧 ACP 工具视图和 compact 分支也有清理空间。本任务的
前端部分负责把这些显示与组织问题收束到新的 Backbone item 契约之上。

## Goal

让会话流的工具调用卡片在折叠态就能说明 agent 正在执行什么操作，并让展开态保留完整
调试信息。前端以 `AgentDashThreadItem` 为唯一输入契约：

```ts
type AgentDashThreadItem = ThreadItem | AgentDashNativeThreadItem;
```

Codex 原生 item 与 AgentDash native item 都由同一套 card shell、kind registry 和
renderer registry 渲染。前端只使用 generated 类型，不重新声明后端 union。

## Current Backend Baseline

本任务 P3 之后的前端工作依赖以下后端基线：

- `BackboneEvent::item_started/item_completed.payload.item` 类型为 `AgentDashThreadItem`。
- Codex `commandExecution` 承载 `shell_exec`。
- Codex `fileChange` 承载 `fs_apply_patch` 和 legacy `FileEdit`。
- AgentDash native `fsRead` / `fsGrep` / `fsGlob` 承载 read/search/list 工具事实。
- Codex `dynamicToolCall` 只作为其他工具或无法结构化解析时的通用形态。
- Codex status enum 和 output content item 直接作为前端状态与输出事实来源。

## Requirements

### 后端 / connector 输入基线

R1. **前端只消费 Backbone item 事实。** renderer 输入为 generated
`AgentDashThreadItem`，不解析 `ActionType`、connector 私有 payload 或 runtime 私有
metadata。

R2. **Codex 原生 item 优先。** `commandExecution`、`fileChange`、`mcpToolCall`、
`webSearch`、`imageView`、`imageGeneration`、`collabAgentToolCall`、
`contextCompaction` 等 Codex 已有 variant 直接作为一级 renderer key。

R3. **AgentDash native item 作为加法扩展。** `fsRead` / `fsGrep` / `fsGlob` 在前端
注册为一级 renderer，与 Codex item 同级处理；它们使用结构化字段做 header 摘要，
展开态仍显示 `arguments` 与 `contentItems`。

R4. **apply_patch 按 Codex file change 处理。** `fs_apply_patch` 在后端进入
`fileChange`，前端不为它建立 dynamicToolCall 专用 renderer；它复用
`FileChangeCardBody` 的文件列表、加减行摘要和 diff 展示。

R5. **application / persistence 消费路径保持 item union 语义。**
`journey/tool_calls.rs` / `task/artifact.rs` / `session/continuation.rs` /
`session_repository.rs` 以 `AgentDashThreadItem` 为输入，并能从 Codex item 与
AgentDash native item 中提取 tool call id 和 tool projection。

### 前端架构

R6. **抽 `ToolCallCardShell`。** header、状态、审批操作、错误展示与折叠容器由 shell
统一承载；具体 renderer 只负责 title/body。

R7. **建立 `AgentDashThreadItem` renderer registry。** 一级分发覆盖：

| item type | renderer |
| --- | --- |
| `commandExecution` | command renderer，header 展示 `$ command` |
| `fileChange` | file change renderer，按文件显示 kind / `+N -M` / diff |
| `fsRead` | read renderer，摘要 path 与 line range |
| `fsGrep` | grep renderer，摘要 pattern 与 path/glob/type |
| `fsGlob` | glob renderer，摘要 pattern 与 path |
| `mcpToolCall` | MCP renderer |
| `webSearch` | web search renderer |
| `imageView` / `imageGeneration` | image renderer |
| `collabAgentToolCall` | collab agent renderer |
| `contextCompaction` | context lifecycle renderer |
| `dynamicToolCall` | generic/dynamic renderer |

R8. **保留 dynamicToolCall 二级摘要。** 对其他未结构化工具，按 `tool` 名提供轻量
summarizer；未知工具进入 GenericJsonBody。

R9. **GenericJsonBody 兜底。** 入参 / 出参双分区，可折叠 JSON 树，保留复制原始 JSON
能力，不再默认把完整对象平铺进 `<pre>`。

R10. **kind 元数据集中。** 新增 `threadItemKind.ts`，统一导出 kind/icon/label；
`getThreadItemKind`、`getKindConfig`、`buildKindSummary` 不再各自维护字面量映射。

R11. **清理孤儿与死路径。** 清理旧 `components/acp/tool-call.tsx`、
`SessionToolCallCard.compact` 分支，以及被 registry 替代的重复 detail 逻辑。

R12. **保留原始信息可见性。** 所有摘要字段在展开态都能看到完整原始值，包括 path、
pattern、patch diff、tool arguments、content items、URL 和错误内容。

## Acceptance Criteria

- [ ] AC1: 前端 renderer 输入类型来自 generated `AgentDashThreadItem`；代码中没有
      围绕 `ActionType` 或 connector 私有结构建立 UI 注册表。
- [ ] AC2: 通过 fixture 或单元测试覆盖 `commandExecution`、`fileChange`、
      `fsRead`、`fsGrep`、`fsGlob`、`dynamicToolCall` 的 registry 分发。
- [ ] AC3: `fs_apply_patch` 产生的 `fileChange` 命中 `FileChangeCardBody`，折叠态展示
      文件路径与 `+N -M` 摘要，展开态按文件显示 diff / add / delete / rename。
- [ ] AC4: `fsRead` / `fsGrep` / `fsGlob` 折叠态分别展示：
      `Read <path>`、`Grep "<pattern>" in <path|glob>`、`Glob <pattern>`。
- [ ] AC5: 任意未注册 dynamic tool 落到 GenericJsonBody，默认能看到“入参 / 出参”
      分区与折叠树。
- [ ] AC6: `packages/app-web/src/components/acp/tool-call.tsx` 已删除；全仓 grep
      `ToolCallView`、`SessionToolCallCard.compact` 无残留命中。
- [ ] AC7: kind 元数据全仓只在 `threadItemKind.ts` 维护；重复 switch 字面量被移除。
- [ ] AC8: `pnpm -C packages/app-web typecheck`、`pnpm -C packages/app-web lint`、
      `pnpm -C packages/app-web test` 不引入新失败；后端协议生成 drift check 通过。

## Out of Scope

- 不重做 `SessionMessageCard` / `SessionPlanCard` / `SessionTaskEventCard` 等非工具卡。
- 不做高级 diff 语法高亮或 JSON 树搜索过滤。
- 不改 `CommandExecutionCard` 的 promote-to-terminal 行为。
- 不把 legacy vibe-kanban `ActionType` 作为前端长期模型。

## Notes

- “外层没有请求摘要”指单个工具卡折叠态 header 缺少关键参数摘要。
- 后端事实源收束由 `05-26-backend-tool-event-source-convergence` 承接；本任务只消费该
  基线提供的 generated Backbone contract。
- `dynamicToolCall` 的二级摘要只服务兜底工具；`fsRead` / `fsGrep` / `fsGlob` /
  `fileChange` 都是一级 renderer。
