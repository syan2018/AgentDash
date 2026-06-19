# 设计：Task 工具集回传修复与展示重构

## 1. 后端回传修复（R1）

### 根因

- [`crates/agentdash-application/src/task/tools.rs`](../../../crates/agentdash-application/src/task/tools.rs) 的 `result_from_details(message, details, is_error)` 把真实数据放进 `AgentToolResult.details`，`content` 仅一句状态串。
- `AgentToolResult`（`crates/agentdash-agent-types/src/runtime/tool.rs`）有 `content: Vec<ContentPart>` 与 `details: Option<Value>`。
- 模型可见 output 仅由 `content` 构造，所有 bridge 均如此：
  - codex：`stream_mapper.rs::decode_tool_result_to_content_items`（只遍历 `result.content`）。
  - anthropic：`bridges/anthropic_bridge.rs::anthropic_tool_result_content(content)`。
  - openai：`bridges/openai_content.rs::tool_result_output_text(content)` / `responses_tool_result_output(content)`。
- → `details` 永不进模型。

### 方案

在 `tools.rs` 把 view JSON 渲染成文本写入 `content`，同时保留 `details` 给 UI。

新增渲染函数（示意）：

```rust
fn result_with_view(summary: &str, view: serde_json::Value, is_error: bool) -> AgentToolResult {
    // content: 人/模型可读文本 = summary + 紧凑 JSON（稳定 schema，便于模型解析）
    let body = serde_json::to_string_pretty(&view).unwrap_or_else(|_| view.to_string());
    let text = format!("{summary}\n{body}");
    AgentToolResult {
        content: vec![ContentPart::text(text)],
        is_error,
        details: Some(view), // UI 自定义卡片复用同一结构
    }
}
```

- `task_read`：`build_read_view` 的返回值既进 `content` 文本也进 `details`。`overview` 已是紧凑摘要；`list`/`detail` 默认走 compact 投影（见下），避免上下文膨胀。
- `task_write`：当前写后用 `return_mode` 重新 `build_read_view`。在此基础上**额外回传本次变更清单** `changes: [{ task_id, title, change_kind, status_from?, status_to? }]`，既进 content 文本也进 details，作为 R2 卡片的数据源。`change_kind ∈ {created, updated, status_changed, reordered, dropped, context_refs_replaced}`。

### compact / full 分级

- `task_read` 增加 `format: compact | full`（默认 compact）。
- compact：每个 Task 仅 `id/title/status/priority/order/assigned_agent_id`，`body` 截断到 N 字（如 160），`context_refs` 仅计数。
- full：完整字段（现有行为）。
- `detail` mode 默认 full（本就用于读单个 Task）。
- 实现：在 `build_read_view` 出口对 `tasks` 做投影映射，不改底层 `list_run_tasks`。

### 变更追踪

`apply_operation` / `apply_snapshot` 当前只收集 `changed_task_ids: Vec<Uuid>`。改为收集 `Vec<TaskChange>`（携带 `change_kind` 与 status 迁移）。`set_status` / snapshot 内的 status 推进记录 `status_from→status_to`（推进前先读当前 status）。

## 2. 前端自定义工具卡片（R2）

### 接入点

- task 工具是 builtin AgentTool，会话流里表现为 `dynamicToolCall`（`item.tool == "task_write" | "task_read"`）。
- 卡片二级 header 在 [`toolCardRegistry.ts`](../../../packages/app-web/src/features/session/ui/toolCardRegistry.ts) 的 `getDynamicToolHeader` 按 `meta.family` 分发；body 走 `DynamicToolCallCardBody`。
- family 映射在 `features/session/model/threadItemKind.ts`（`resolveDynamicToolMeta`）。需新增 `task` family（匹配 `task_read`/`task_write`）。

### 卡片内容

- **header**：
  - `task_write`：主标题「更新 N 项 Task」，副标题取 mode（patch/snapshot）。
  - `task_read`：主标题取 mode（overview/list/detail…）。
- **body**：新增 `TaskToolCardBody`（参考现有 `bodies/` 下组件）：
  - **数据来源已核实**：前端 `dynamicToolCall` thread item 只有 `arguments` + `contentItems`，**没有 `details` 字段**（`backbone-protocol.ts` 确认）。所以卡片从 `item.contentItems` 取出 text part 并 `JSON.parse` 出 R1 写入的 view。→ 这是 R1 必须把**稳定结构化 JSON** 写进 `content`（而非纯散文）的硬约束，R1 是 R2 的前置依赖。
  - `task_write`：解析 view 里的 `changes[]`，每行 = 状态徽标 + 标题 + 变更类型；status_changed 显示 `from→to`。复用 `TaskStatusBadge`（`components/ui/status-badge.tsx`）。
  - `task_read`：overview 渲染进度计数 + active 项；list 渲染紧凑行。
  - 解析失败兜底：回退现有 `GenericJsonBody`，不报错。

## 3. 综合会话状态栏（R3）

### 现状

- mailbox（pending/steering）：`SessionChatView.tsx:659` 渲染 `MailboxMessageList`，就在 `SessionChatComposer` 之上；数据来自 `mailboxSnapshot`（messages/state/user_attention/paused）。
- `inputPrefix`：composer 顶部插槽（`SessionChatViewParts.tsx:386`），当前传 owner/draft binding bar。
- Task 进度：`TaskPlanPanel`（`pages/AgentRunWorkspacePage.tsx:743`）挂在聊天区顶部，数据来自 `taskPlanStore`（`stores/taskPlanStore.ts` / `services/taskPlan.ts`），同源 `LifecycleRun.tasks`。

### 方案

新增组件 `features/agent-run-workspace/ui/SessionStatusBar.tsx`（命名以实现时既有约定为准），渲染在 `MailboxMessageList` 当前位置，**取代**它对外的容器，内部组合：

- **Task 进度区**（数据：`taskPlanStore` 当前 run 的 tasks，过滤未归档）：
  - 折叠态：当前 active Task 标题（取第一个 `status==active`，无则取 review/blocked/open 优先级最高项）+ 进度 `done/total`（如 `3/4`，参考截图）。
  - 展开态：完整 Task 清单（`TaskStatusBadge` + 标题 + priority），按 order/updated_at 排序。
- **Mailbox 区**：保留 `MailboxMessageList` 既有渲染与操作（promote/delete/move/recall/resume），并入展开态；折叠态若有 pending/paused 给小徽标提示。
- 折叠/展开状态本地 `useState`，默认折叠（有 active task 或 pending 时可默认展开，实现时定）。

### TaskPlanPanel 处置

- Task 清单能力并入状态栏后，移除 `AgentRunWorkspacePage.tsx:743` 的 `TaskPlanPanel` 挂载，避免两套 Task UI。
- 创建/编辑 Task 的表单能力（`TaskDrawer`）保留：状态栏 Task 行点击仍可打开 `TaskDrawer` 查看/编辑详情。是否在状态栏保留「新建」入口，实现时按交互简洁性决定（倾向保留一个轻量入口）。

### 数据流

- 状态栏挂在 AgentRunWorkspacePage，已有 `currentRunId`/`currentAgentId`，沿用 `useTaskPlanStore().fetchAgentRunTasks` 拉取；工具写入后由现有刷新路径（session 事件 / 轮询）更新，与 mailbox 同区域实时反映。
- 不新增 store / 不新增事实源。

## 4. 契约与生成物

- 工具结果是 agent tool 的 JSON（非 REST DTO），前端只能从 `contentItems` text part 解析，**不经过 generated contract**。因此工具结果 view 的 JSON 形状由 `tools.rs` 与前端卡片各自约定（在 `TaskToolCardBody` 内定义最小解析类型），无需改 `crates/agentdash-contracts`。
- 例外：状态栏 Task 进度走 `taskPlanStore`（REST），沿用现有 `task-contracts.ts`，不变。

## 5. 测试策略

- 后端：`task_read`/`task_write` 单测断言 `content` 文本非空且含 Task 标题/状态；`task_write` 断言 `changes` 含正确 change_kind 与 status 迁移；compact 投影截断断言。
- 前端：`TaskToolCardBody` 渲染 changes/overview 的快照或断言；`SessionStatusBar` 折叠/展开、进度计数、mailbox 操作透传断言。
- 既有测试（`AgentRunWorkspacePage.hook-runtime.test.tsx` 等）回归。

## 6. 风险

- ~~dynamicToolCall 取数字段~~ 已核实：thread item 只带 `contentItems`，R2 卡片解析 content 文本 JSON，R1 为其前置。
- 状态栏与 `inputPrefix` 叠放：两者都在 composer 上方，需确认视觉层次与高度占用。
- R1 把完整 JSON 写进 content 会增加 token 占用 → 靠 compact 默认 + body 截断控制；`task_write` 写后回传默认用 compact return_mode。
