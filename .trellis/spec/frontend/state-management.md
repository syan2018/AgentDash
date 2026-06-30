# 状态管理

> Zustand 5 全局状态 + React useState 本地状态。

---

## 状态分层

| 状态类型 | 存放位置 | 示例 |
|----------|----------|------|
| 本地 UI 状态 | 组件内 `useState` | `isOpen`, `selectedTab` |
| Feature 状态 | Feature `model/` hooks | `entries`, `isConnected` |
| 全局应用状态 | `stores/` | `projects`, `currentProjectId` |
| 服务端缓存 | Store + API | `storyTaskProjectionByStoryId`, `workspacesByProjectId` |

派生状态使用 `useMemo` 计算，不存储在状态中。

---

## Store 清单

| Store | 职责 |
|-------|------|
| `projectStore` | Project CRUD + 选择 |
| `workspaceStore` | Workspace CRUD + 状态管理 |
| `storyStore` | Story 数据 + Story Task projection cache |
| `coordinatorStore` | 后端连接管理 |
| `eventStore` | 项目级 NDJSON 事件流 |
| `workflowStore` | `WorkflowGraph` 定义态管理；Agent Activity 关联的 `AgentProcedure` draft 是配套编辑数据 |
| `lifecycleStore` | Lifecycle 运行态 view projection：run、graph instance、subject execution、agent、frame、runtime trace |
| `taskPlanStore` / AgentRun workspace model | Run-scoped Task plan facts：创建、推进、归档、assignment |
| `sessionHistoryStore` | 会话历史 |
| `settingsStore` | 全局设置 |
| `currentUserStore` | 当前用户 |
| `activeSessionsStore` | 活跃会话追踪 |
| `llmProviderStore` | LLM Provider 管理 |
| `routineStore` | Routine 管理 |
| `authStore` | 认证状态 |
| `sidebarSessionsStore` | 侧边栏会话列表 |
| `workspaceTabStore` | 工作空间标签页 |

---

## 何时使用全局 Store

1. **跨组件共享**：多个不相关组件需要访问同一份数据
2. **跨页面持久**：路由切换后仍需保持的状态
3. **服务端缓存**：从 API 获取的数据需要缓存

---

## Store 规范

- 使用 `isLoading` / `error` 追踪加载和错误状态
- 内部 API response 由 service 层按 generated contract type 返回；store 不为 generated DTO 再做字段级归一化
- Store state 消费 service 层产出的 typed DTO 或 view model；跨层 DTO 类型来自 `src/generated/*`，原因是 store 不应成为协议字段事实源
- 按 Feature 拆分 Store，避免单个 Store 过大
- 始终通过 `set` 更新状态，不直接修改
- `workflowStore` 不保存运行态事实；Activity attempt、agent assignment、runtime trace 等观察数据进入 `lifecycleStore` 或 session projection。
- `lifecycleStore` 只缓存后端 lifecycle view，不作为 command input；写命令应从 SubjectRef、run/graph/agent/frame refs 或明确的 API intent 发起。
- Session UI 可以消费 `RuntimeSessionTraceView` 与 frame runtime projection，但不能从 session title、session 存在性或 trace 内容推导 Task / Story / Lifecycle 状态。
- Task plan facts 从 Run / AgentRun scoped API 进入 AgentRun workspace model 或专用 Task plan store；Story 页面只消费 Story Task projection cache。
- `lifecycleStore` 是 SubjectExecution、runtime artifacts、latest runtime node 与 linked runs 的唯一执行投影缓存；Task plan store 和 `storyStore` 不保存这些运行事实。

## Story Task Projection 与 Run-scoped Task Plan State

Story 页面展示的 Task 列表是 projection，来源于 Story-bound LifecycleRun、linked run 和可选 `story_ref`。该缓存可以放在 `storyStore`，命名应表达 projection 语义，例如 `storyTaskProjectionByStoryId`。

AgentRun workspace 是 Task plan facts 的写入口。创建、推进、归档和 assignment 命令使用 run / agent refs 发起，并在成功后刷新对应 run-scoped Task plan view。Story projection 需要通过 projection endpoint 或后端事件重新拉取，不能用本地写入的 Task plan DTO 推断 Story ownership。

Task runtime artifacts、running / failed / cancelled 等执行状态只进入 `lifecycleStore` 的 `SubjectExecutionView` 或 AgentRun / RuntimeSession projection。Task plan status 只使用 `open / active / review / blocked / done / dropped`。

## AgentRun Workspace Conversation Snapshot

执行工作台输入区的可执行状态来自后端 `AgentConversationSnapshot.commands`、
`ConversationKeyboardMapView` 与 mailbox projection。页面层可以把 snapshot 转成组件 view
model，但 command id、keyboard mapping、stale guard、snapshot id、model policy、mailbox status
与 disabled reason 都保持后端生成的 generated DTO 形状。这样做的原因是 AgentRun 命令可用性同时
依赖 run / agent / frame、delivery runtime、active AgentRunTurn、mailbox envelope、模型解析和
connector capability；这些事实只能由后端在同一个 snapshot 中一致投影。

用户意图 command 包括 draft start、message submit、mailbox promote/delete/resume 与 cancel。
文本输入统一提交 `composer-submit`，后端返回 scheduler outcome；前端不把
`launched | queued | steered` 推导成独立业务分支。mailbox row 按 `MailboxMessageView.status`、
`barrier` 和 `delivery` 展示排队、正在注入、等待 turn 边界、等待恢复、已投递或失败。这样按钮、
键盘和 mailbox row 共享同一份后端投影，刷新滞后、completed/idle 状态和 active-turn 变化都会表现
为结构化 command result。

模型选择显示来自 `AgentConversationSnapshot.model_config`。ProjectAgent preset、当前 frame
execution profile、用户显式 override 与后端认可的 discovery default 在后端解析成同形
`effective_executor_config` 或 `model_required`。前端 selector 可以维护用户正在编辑的 override，
但输入区提交能力由 snapshot command 和 model policy 决定，原因是 ProjectAgent 默认模型与运行中
frame 模型必须在同一层完成字段级合并。

mailbox UI 消费 `AgentConversationSnapshot.mailbox` 的 visible message count、message rows、
user attention 与 resume command。队列是否暂停、message 是否 blocked/failed 是机制事实，是否渲染
提示是用户注意力事实；把两者分开可以让 terminal/ready 状态下的历史暂停不变成新的用户工作。

`AgentRunWorkspaceControlPlaneView.status` 使用 AgentRun workspace 语义：
`ready | running | terminal | frame_missing | delivery_missing`。RuntimeSession detail 使用
`SessionRuntimeControlView`，原因是 runtime trace/detail 从 runtime session identity 出发，而
AgentRun workspace 从 run / agent identity 出发。

SessionChatView 的职责是执行传入 command，不持有业务分派规则。Enter、Ctrl/Cmd+Enter 与按钮点击
都从 snapshot keyboard/command list 选择 command id；cancel 作为独立命令展示。这样 running
workspace 可以同时显示 mailbox message、运行中注入状态和取消，ready workspace 显示可提交消息，
只读 trace 展示后端 reason。

RuntimeSession stream 中的 turn 终态以 `Platform(SessionMetaUpdate)` 的 `turn_terminal` key 作为
统一事件形态，value 中的 `terminal_type` 表达 `turn_completed`、`turn_failed` 或
`turn_interrupted`。AgentRun workspace 监听该终态事件后刷新 workspace snapshot，原因是终态落库、
active turn cleanup 与 command projection 已在后端完成，前端应重新读取权威 snapshot，而不是继续
使用上一帧 running command。

`ConversationCommandView.stale_guard.snapshot_id` 是一次 workspace projection 的不透明前置条件。
文本输入提交使用 AgentRun `composer-submit` command：前端把当时选中的 command id、kind 与 stale
guard 原样回传，后端只把它作为用户意图上下文，然后用当前 run / agent / frame / runtime /
active AgentRunTurn / mailbox 事实创建 envelope 并调度。这样做的原因是 Enter/Ctrl-Enter 表达的是
“提交这段用户输入”，而不是要求前端持有的上一帧 running/ready token 继续有效；投影刷新滞后时 follow-up
仍由后端当前 snapshot 接受为正确的 mailbox message。

非文本控制命令（如 cancel、mailbox promote/delete/resume）继续携带 stale guard 并由后端做精确
precondition 校验，原因是这些命令没有可重新归类的用户输入，必须绑定到 snapshot 暴露的具体
runtime/AgentRunTurn/mailbox envelope 事实。

## AgentRun Workspace 状态来源

AgentRun Workspace 的 title、status、list entry 和 action state 来自后端提供的
AgentRun Workspace projection。该 projection 面向用户工作台 shell，聚合 ProjectAgent display
name、Subject association、LifecycleAgent、AgentFrame、active turn、delivery summary、command
receipt 与 workspace activity 时间。

Delivery-backed AgentRun 的工作台标题由后端 `AgentRunWorkspaceShell.display_title` 承接
RuntimeSession `SessionMeta.title` / `title_source`，原因是用户可见的会话标题随 runtime trace
更新，而前端 header、侧栏快捷入口和 AgentRun 列表仍应消费同一个 workspace shell 投影。没有
delivery RuntimeSession meta 的 workspace 再使用 AgentRun/workspace 备用 title。

RuntimeSession trace metadata 仍进入 trace/feed/debug 展示：事件游标、trace title provenance、
delivery trace summary、last turn pointer、terminal summary 和 executor continuation 都属于
runtime trace 视角。Workspace route 可以展示关联的 `delivery_trace_meta` 或 trace link，但
侧栏列表、工作台标题、运行状态和最近活动以 AgentRun Workspace shell/projection 为准；输入区、
keyboard shortcut、mailbox promote/delete/resume 和 cancel 的可执行性以
`AgentRunWorkspaceView.conversation.commands` 为准。这样工作台 shell 和用户命令投影各自保持窄职责：
shell 服务导航与展示，conversation snapshot 服务可执行控制面。

同一 `run_id + agent_id + source_key` 的 AgentRun Workspace refresh 保留上一帧 `workspace`、
`runtime_session_id`、resource surface 与 frame，原因是 `SessionChatView` 的 NDJSON stream
生命周期绑定 runtime session identity，右侧 resource browser 也需要展示连续性。输入区 command
authority 来自最新 `AgentConversationSnapshot.commands`；`loading` / `refreshing` / `error` /
stale projection 状态下上一帧 snapshot 只能用于展示诊断。

`session_meta_updated`、`Platform(SessionMetaUpdate)` 与 RuntimeSession event stream 仍是 feed
和 debug 面板可渲染的事实。工作台标题编辑和状态刷新通过 AgentRun Workspace shell 刷新或后续
AgentRun shell event 进入 store，原因是用户可见工作台 shell 与 trace metadata 的更新节奏和事实源
不同。

---

## Projection Store 写后刷新

HTTP-only projection store（如 `extensionRuntimeStore` 缓存的 `ExtensionRuntimeProjectionResponse`）没有 SSE / NDJSON 失效流。**任何会改变底层实体的写操作（HTTP POST/DELETE 等），调用方必须在 success 分支显式调 `store.fetchProject(projectId)` 触发重拉**，不能依赖局部 patch 或 optimistic update。

**为什么**：projection 由后端聚合多张表（installation / artifact / runtime action / workspace tab / permission / bundle）派生，前端无法本地推导；漏 refetch 会造成"写完了但 UI 还是过期数据"，或更糟：不同入口看到的投影不一致。

**典型形态**（写入处复制此模式）：

```ts
async function handleUninstall() {
  setBusy(true);
  try {
    await uninstallExtensionInstallation(projectId, installationId);
    await useExtensionRuntimeStore.getState().fetchProject(projectId); // 必填
    setNotice({ tone: "success", message: "已更新 Extension runtime projection" });
  } catch (err) {
    setNotice({ tone: "danger", message: extractMessage(err) });
  } finally {
    setBusy(false);
  }
}
```

适用范围：写后无 stream invalidation 的 store。如果 store 已订阅事件流（`eventStore`、`sessionHistoryStore` 这类），由 reducer 接管失效，不需要手动 refetch。新建此类 store 时把"写操作的入口在哪里 fetch"写在 store 顶部注释里，避免漏配。

---

## 常见错误

| 错误 | 正确做法 |
|------|----------|
| 在多个 Store 存储同一份数据 | 单一 Store 存储，其他使用 selector |
| 存储可计算数据 | 使用 `useMemo` 计算 |
| 直接修改状态 | 始终通过 `set` 更新 |
| Store 过于庞大 | 按 Feature 拆分 |
| 忘记 reset 状态 | 提供 reset action |
