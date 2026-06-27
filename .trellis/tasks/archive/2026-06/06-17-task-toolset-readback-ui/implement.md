# 执行计划：Task 工具集回传修复与展示重构

## 依赖关系

```
M1 (后端回传修复 + changes)  ──┬──> M2 (task 工具自定义卡片)
                              └──> M3 (综合状态栏)   # M3 不强依赖 M1，但与 M2 共享 UI 验收
M1, M2, M3 ──> M4 (验证 + 真实会话验收)
```

- M1 是 M2 的硬前置（卡片解析 content JSON）。
- M3（状态栏，数据走 taskPlanStore REST）可与 M1/M2 并行。

## M1 后端：工具回传可被模型读取（R1）

文件：`crates/agentdash-application/src/task/tools.rs`

1. 新增 `result_with_view(summary, view, is_error)`：`content` = `summary + "\n" + 紧凑/pretty JSON`，`details = Some(view)`。替换 `task_read`/`task_write` 出口的 `result_from_details`。
2. `TaskReadParams` 增加 `format: TaskReadFormat { Compact, Full }`，默认 `Compact`。`build_read_view` 出口对 `list`/`overview`/`context` 的 tasks 做 compact 投影（`id/title/status/priority/order/assigned_agent_id` + body 截断 + context_refs 计数）；`detail` 默认 full。
3. 变更追踪：把 `changed_task_ids: Vec<Uuid>` 升级为 `Vec<TaskChange>`（`task_id/title/change_kind/status_from?/status_to?`）。
   - `apply_operation` 各分支填 `change_kind`；`SetStatus` / snapshot status 推进前先读旧 status 填 `status_from→status_to`。
4. `task_write` 出口：view 里加 `changes` 字段，写后 `build_read_view` 用 compact return_mode。
5. 单测（`task::plan` 或同模块 `#[cfg(test)]`）：
   - `task_read` 结果 `content` text 非空且含某 Task 标题。
   - `task_write` create/set_status 后 `changes` 含正确 `change_kind` 与 status 迁移。
   - compact 投影 body 截断、context_refs 计数。

验证：`cargo check -p agentdash-application` → `cargo test -p agentdash-application task`

## M2 前端：task_write / task_read 自定义卡片（R2）

文件：`packages/app-web/src/features/session/`

1. `model/threadItemKind.ts`：`resolveDynamicToolMeta` 为 `task_read`/`task_write` 返回 `family: "task"`（+ fallbackLabel）。
2. `ui/toolCardRegistry.ts::getDynamicToolHeader`：加 `case "task"` —— `task_write` 头「更新 N 项 Task」（N 从 args.operations/snapshot 估算或留摘要），`task_read` 头取 `args.mode`。
3. 新增 `ui/bodies/TaskToolCardBody.tsx`：
   - 从 `item.contentItems` 找 text part，剥离 summary 行后 `JSON.parse`；失败回退 `GenericJsonBody`。
   - `task_write`：渲染 `changes[]`（`TaskStatusBadge` + 标题 + change_kind；status_changed 显示 `from→to`）。
   - `task_read`：overview 渲染 counts + active_items；list 渲染紧凑行。
4. `ui/bodies/DynamicToolCallCardBody.tsx`：`tool === "task_read" || "task_write"` 分流到 `TaskToolCardBody`。
5. 前端测试：`TaskToolCardBody` 解析 changes / overview 的渲染断言；解析失败回退断言。

验证：`pnpm run frontend:check`

## M3 前端：综合会话状态栏（R3）

文件：`packages/app-web/src/features/agent-run-workspace/ui/` + `pages/AgentRunWorkspacePage.tsx`

1. 新增 `SessionStatusBar.tsx`：props 接收 mailbox 相关 props（透传 `MailboxMessageList`）+ `runId/agentId`（拉 taskPlanStore）。
   - 折叠态：当前 active Task 标题（active > review > blocked > open 优先）+ 进度 `done/total`；有 pending/paused 加徽标。
   - 展开态：完整 Task 清单（`TaskStatusBadge` + 标题 + priority，点击开 `TaskDrawer`）+ 内嵌 `MailboxMessageList`（保留 promote/delete/move/recall/resume）。
   - 折叠/展开 `useState`；默认折叠（有 active/pending 时默认展开）。
2. `SessionChatView.tsx:659`：把 `MailboxMessageList` 替换为 `SessionStatusBar`（mailbox props 透传），或在其上层包一层；保留 `shouldShowMailboxList` 逻辑作为「无 task 且无 mailbox 时不渲染」。
   - 需要把 `runId/agentId` 传进 SessionChatView → composer 区域（新增 prop 或复用既有 run 上下文）。
3. `pages/AgentRunWorkspacePage.tsx:743`：移除聊天区顶部 `TaskPlanPanel` 挂载（能力并入状态栏）。保留 `TaskDrawer` 走状态栏行点击。
4. 前端测试：`SessionStatusBar` 折叠/展开、进度计数、mailbox 操作透传；`AgentRunWorkspacePage.hook-runtime.test.tsx` 回归。

验证：`pnpm run frontend:check`

## M4 验证 + 真实会话验收

1. `cargo check --workspace`
2. `pnpm run contracts:check`（预期无契约变更；若 M1 动了 contract 才需要）
3. `pnpm run frontend:check`
4. 真实会话验收（`pnpm dev`）：
   - 启动 AgentRun，让 agent 调 `task_write` 建/改 Task，再调 `task_read`。
   - 确认 tool result 文本里**真有 Task 数据**（不再只有「Task view 已读取」），agent 能据此继续。
   - 确认会话流 `task_write` 卡片展示变更内容、`task_read` 卡片展示摘要。
   - 确认输入栏上方综合状态栏：折叠显示待办+进度，展开显示清单+mailbox。
5. 把验收结果（含失败/限制）如实记录到本文件「实现记录」。

## 回滚点

- M1 纯后端、向后兼容（只扩 content + 加可选字段），可独立回滚。
- M2/M3 纯前端，可独立回滚；M3 移除 TaskPlanPanel 若有问题可还原挂载。

## review gate

- M1 改的是模型可见回传语义，需确认 content JSON 形状稳定、compact 默认不漏关键字段。
- M3 移除 TaskPlanPanel 属于 UI 行为变化，验收时确认无功能回归（创建/编辑 Task 仍可达）。

## 实现记录（2026-06-17）

### M1 后端（crates/agentdash-application/src/task/tools.rs）
- 新增 `result_with_view`：把 view JSON 既写进 `content`（`ContentPart::text` = `"{summary}\n{pretty json}"`）也写进 `details`。替换 `task_read`/`task_write` 出口原来的 `result_from_details`（后者只把状态串放进 content，是 P0 根因）。
- 新增 `TaskReadFormat {Compact, Full}`（默认 Compact）+ `task_json`/`render_tasks` 投影：compact 只回 `id/title/status/priority/assigned_agent_id/body_preview(截断160字)/context_refs_count/archived`；detail mode 强制 full。
- 新增 `TaskChange{task_id,title,change_kind,status_from?,status_to?}` + `TaskChangeKind`。`apply_operation`/`apply_snapshot` 收集 `Vec<TaskChange>`；`set_status`/snapshot 状态推进前读旧 status 填迁移。`task_write` 出口 view 注入 `changes` 字段。
- 单测 5 个（result_with_view 把数据放进 content、compact 截断与字段集、full 保全字段、TaskChange 序列化）。

### M2 前端工具卡片
- `model/threadItemKind.ts`：新增 `task` kind（badge TASK）+ `task` family，`task_read`/`task_write` 映射到该 family。
- `ui/toolCardRegistry.ts`：`getDynamicToolHeader` 新增 `task` case（task_write「更新 N 项 Task」+ mode 副标题；task_read「读取 Task · {mode}」）。
- 新增 `ui/bodies/TaskToolCardBody.tsx`：从 `contentItems` text part 解析 R1 写入的 JSON（前端 thread item 无 details，只能解析 content），task_write 渲染 changes[]（含 status 迁移），task_read overview 渲染进度+active、list 渲染行；解析失败回退 GenericJsonBody。
- `ui/bodies/DynamicToolCallCardBody.tsx`：`task_read`/`task_write` 路由到 TaskToolCardBody。

### M3 综合会话状态栏
- 新增 `features/agent-run-workspace/ui/SessionStatusBar.tsx`：折叠态显示 `done/total` + 当前待办（active>review>blocked>open）+ pending/暂停徽标；展开态显示完整 Task 清单（点击开 TaskDrawer）+ 内嵌 mailbox 分区。Task 数据走 `taskPlanStore`（同源 LifecycleRun.tasks）。无 task 且无 mailbox 内容时返回 null。
- 重构 `MailboxMessageRow.tsx`：抽出 `MailboxSections`（无卡片 chrome 的内部分区）；`mailboxHasContent` 移到新文件 `mailboxContent.ts`（避免 react-refresh 报错）。`MailboxMessageList` 保留供测试。
- `SessionChatView`：新增 `statusBarRunId/statusBarAgentId` props，把 `MailboxMessageList` 渲染替换为 `SessionStatusBar`。
- `AgentRunWorkspacePage`：移除聊天区顶部 `TaskPlanPanel` 挂载，改传 `statusBarRunId/agentId`；删除 orphaned `task-plan-panel.tsx`。
- 顺带修复分支上**已存在**的 `task-drawer.tsx` set-state-in-effect lint error：删除同步 effect，改为依赖消费方 `key={task.id}` remount（StoryPage 本就这么传，已给 SessionStatusBar 也加上）。

### 已执行验证（全绿）
- `cargo check --workspace`
- `cargo test -p agentdash-application task::tools`（5 passed）
- `pnpm run contracts:check`（无契约变更，全部 up to date）
- `pnpm run frontend:check`（typecheck）
- `pnpm --filter app-web run lint`（全绿，含修复 1 个 pre-existing error）
- `pnpm --filter app-web run test`（59 files / 320 tests passed）

### 待办：真实会话验收（未执行）
- 需 `pnpm dev` 起全栈 + 真实 LLM agent 调用 `task_write`/`task_read`，验证：
  1. tool result 文本含真实 Task 数据（不再只有「Task view 已读取」）。
  2. 会话流 task_write 卡片展示变更、task_read 卡片展示摘要。
  3. 输入栏综合状态栏折叠/展开正确。
- 上一任务此步骤的「验收通过」记录被证伪，故本任务不在未运行的情况下声称通过；留待在运行环境中实跑。

## 反馈修复轮（2026-06-17，用户实跑后）

用户 `pnpm dev` 实跑确认：task_write 卡片「更新 N 项 Task」、状态推进卡片（open→active / active→review）、task_read overview「进度 + 计数 + 任务行」均正常渲染，模型确实读到数据。P0 与卡片成立。据反馈修复：

后端 tools.rs：
- (b) **修 bug**：task_write 写后回传不再按 `changes.first()` 过滤成单个 Task（之前批量建 4 个只回 1 个）；改为 `task_id: None` 回完整 plan（include_archived=false），具体变更由 `changes` 表达。
- (a) context kind/slot/delivery 的报错信息补上可用枚举值（之前 `note` 报「未知 context kind」无提示）。
- (c) overview `active_items` → `current_items`（含 active/review/blocked，避免与单一 active 混淆）；前端 TaskToolCardBody 同步改读 `current_items`。

前端：
- **状态栏不常驻修复**：SessionStatusBar 只在 mount 拉一次，agent 会话中途 task_write 后不刷新 → 在 `AgentRunWorkspacePage.handleTurnEnd` 增加 `fetchAgentRunTasks` 刷新（store 响应式，bar 自动更新）。
- **复用 Story 图标**：status-badge.tsx 新增 `TaskStatusIcon`（复用 Story 圆环进度视觉 + blocked/dropped 特殊标记）与 `TaskStatusToken`（图标+文案）；`TaskStatusBadge` 升级带图标；会话卡片与状态栏改用 `TaskStatusToken`，替换之前自画的粗糙徽标。

验证（全绿）：`cargo test -p agentdash-application task::tools`(5)、`frontend:check`、`lint`、`test`(320)。
