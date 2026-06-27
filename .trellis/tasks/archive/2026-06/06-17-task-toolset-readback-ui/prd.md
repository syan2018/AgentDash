# Task 工具集回传修复与展示重构

## 背景

`06-17-universal-task-toolset` 已落地 `task_read` / `task_write` 两个 agent-facing 工具，写入 `LifecycleRun.tasks`。但该任务被标记 completed 的「前端会话验收」记录与实际行为不符，存在一个根因 bug 和两块未达成的展示需求。本任务是它的修复与展示收尾。

## 核心问题（已核实）

1. **P0 回传 bug：模型完全读不到 Task 数据。**
   `task_read` / `task_write` 把厚 Task view 放进 `AgentToolResult.details`，`content` 只有一句 `"Task view 已读取"` / `"Task 写入完成，变更 N 个 Task。"`。而所有 connector bridge（anthropic / openai chat+responses / codex `decode_tool_result_to_content_items`）构造模型可见 tool output 时**只读 `content`，完全丢弃 `details`**。因此模型每次调用都只收到那一行状态字符串，拿不到任何 Task 内容。直接违反原 PRD「厚 Task 读回」「写后完整读回」验收。
2. **task 工具无自定义卡片：** 会话流里 `task_read`/`task_write` 落到 `dynamicToolCall` 通用卡片，body 只渲染 content 文本（即那句状态串），用户看不到具体写入/变更了什么。
3. **缺少综合会话状态栏：** 当前 pending/steering（mailbox）栏与 Task 进度是分离的，`TaskPlanPanel` 粗糙且固定占据聊天区顶部，与原 PRD「输入栏综合状态展示」的意图不符。

## Goal

让 `task_read`/`task_write` 真正把 Task 数据回传给模型；让 `task_write` 调用在会话流里有定制 GUI 展示「这次更新了什么」；把 mailbox 栏与 Task 进度合并为输入栏上方的一个可展开综合会话状态栏。

## Requirements

### R1 后端：工具回传可被模型读取（P0）
- `task_read` / `task_write` 的结果必须把 Task view 序列化进 `content`（`ContentPart::text`），让模型能读到。`details` 保留供 UI 消费。
- 默认 compact：content 文本不能把整个 `Vec<Task>` 原样塞入导致上下文膨胀；需要分级（compact 摘要 / full 详情），与 `task_read` mode 默认行为对齐。
- `task_write` 写后回传必须包含「本次变更了哪些 Task、变更类型」，使模型与 UI 都能据此呈现。

### R2 前端：task_write / task_read 自定义工具卡片
- 在工具卡片注册表中为 task 工具加 family 渲染（参考现有 `todo` family）。
- `task_write` 卡片展示本次写入的结构化变更：每个被创建/更新/推进状态/归档/排序的 Task，标题 + 变更类型 + 关键字段（如 status 迁移 `open→active`）。
- `task_read` 卡片按 mode 给出可读摘要（overview 进度 / list 列表），而不是只显示状态串。
- 卡片数据来自工具结果（content 或 details），不额外发请求。

### R3 前端：综合会话状态栏（合并 mailbox + Task 进度）
- 在输入栏（composer）上方，把现有 mailbox（pending/steering）列表与 Task 进度合并为**一个**可折叠状态栏。
- 折叠态：显示当前正在执行的待办项（active Task 标题）+ 整体进度（如 `3/4`）；若有 pending/steering 消息或 paused 状态，并入同一栏提示。
- 展开态：显示完整 Task 清单（带状态）+ mailbox 消息行（保留现有 promote/delete/move/recall 操作）。
- 数据源：Task 进度复用 `LifecycleRun.tasks`（`taskPlanStore` / `task_read` overview 同源），与后端工具写入一致、实时刷新。
- 重新评估 `TaskPlanPanel` 在聊天区顶部的固定占位：其能力并入状态栏后应移除或退化，避免两套 Task UI。

## Acceptance Criteria

- [ ] 真实会话中 agent 调用 `task_read` 能在 tool result 文本里读到 Task 列表/详情（不再只有「Task view 已读取」），并能据此继续工作。
- [ ] `task_write` 调用后，模型 tool result 文本包含本次变更的 Task 及类型；写入后再 `task_read` 能读回一致结果。
- [ ] 会话流中 `task_write` 卡片以定制 GUI 展示本次变更内容；`task_read` 卡片展示可读摘要而非状态串。
- [ ] 输入栏上方存在单一综合状态栏：折叠显示当前待办 + 进度，展开显示完整 Task 清单与 mailbox 消息，mailbox 既有操作不回归。
- [ ] 不引入第二套 Task 事实源；状态栏 Task 数据与 `LifecycleRun.tasks` 同源。
- [ ] `cargo check` / `pnpm run contracts:check` / `pnpm run frontend:check` 通过；新增/调整逻辑有针对性测试。

## 非目标

- 不重做 `task_read` 的 execution/projection 真实接入（execution 仍可为 stub，单独跟进）。
- 不引入 snapshot 的 base revision precondition（并发安全单独评估，本轮不扩范围）。
- 不改 Task 后端事实模型与持久化（`LifecycleRun.tasks` 不动 schema）。

## 待确认 / 风险

- compact/full 分级的具体字段集合在 design.md 细化。
- 状态栏与 `inputPrefix`（owner/draft binding bar）的叠放顺序需在实现时确认不冲突。
