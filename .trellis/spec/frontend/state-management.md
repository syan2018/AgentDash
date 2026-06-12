# 状态管理

> Zustand 5 全局状态 + React useState 本地状态。

---

## 状态分层

| 状态类型 | 存放位置 | 示例 |
|----------|----------|------|
| 本地 UI 状态 | 组件内 `useState` | `isOpen`, `selectedTab` |
| Feature 状态 | Feature `model/` hooks | `entries`, `isConnected` |
| 全局应用状态 | `stores/` | `projects`, `currentProjectId` |
| 服务端缓存 | Store + API | `tasksByStoryId`, `workspacesByProjectId` |

派生状态使用 `useMemo` 计算，不存储在状态中。

---

## Store 清单

| Store | 职责 |
|-------|------|
| `projectStore` | Project CRUD + 选择 |
| `workspaceStore` | Workspace CRUD + 状态管理 |
| `storyStore` | Story/Task 数据 |
| `coordinatorStore` | 后端连接管理 |
| `eventStore` | 项目级 NDJSON 事件流 |
| `workflowStore` | `WorkflowGraph` 定义态管理；Agent Activity 关联的 `AgentProcedure` draft 是配套编辑数据 |
| `lifecycleStore` | Lifecycle 运行态 view projection：run、graph instance、subject execution、agent、frame、runtime trace |
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

## AgentRun Workspace Conversation Snapshot

执行工作台输入区的可执行状态来自后端 `AgentConversationSnapshot.commands` 与
`ConversationKeyboardMapView`。页面层可以把 snapshot 转成组件 view model，但 command kind、
command id、keyboard mapping、stale guard、snapshot id、model policy 与 disabled reason 都保持后端生成的
generated DTO 形状。这样做的原因是 AgentRun 命令可用性同时依赖 run / agent / frame、delivery
runtime、active turn、pending queue、模型解析和 connector capability；这些事实只能由后端在同一
个 snapshot 中一致投影。

`start_draft`、`send_next`、`enqueue`、`steer`、`promote_pending`、`resume_pending_queue`、
`cancel` 是不同用户意图：draft 首条消息 materialize runtime/lifecycle，ready `send_next` 启动
下一轮 prompt，running `enqueue` 进入待投递队列，running `steer` 注入当前 active turn，
`promote_pending` 把一条 pending message 投递到当前 turn，`resume_pending_queue` 恢复需要用户
处理的暂停队列，`cancel` 中断当前 turn。前端按 snapshot command id 提交命令，原因是按钮、键盘
和 pending row 必须共享同一份 precondition token，才能让刷新滞后、completed/idle 状态和
active-turn 变化表现为结构化 command conflict。

模型选择显示来自 `AgentConversationSnapshot.model_config`。ProjectAgent preset、当前 frame
execution profile、用户显式 override 与后端认可的 discovery default 在后端解析成同形
`effective_executor_config` 或 `model_required`。前端 selector 可以维护用户正在编辑的 override，
但输入区提交能力由 snapshot command 和 model policy 决定，原因是 ProjectAgent 默认模型与运行中
frame 模型必须在同一层完成字段级合并。

pending UI 消费 `AgentConversationSnapshot.pending` 的 `visible_message_count`、
`user_attention` 与 `resume_command`。队列是否暂停是机制事实，是否渲染提示是用户注意力事实；把
两者分开可以让 terminal/ready 状态下的历史暂停不变成新的用户工作。

`AgentRunWorkspaceControlPlaneView.status` 使用 AgentRun workspace 语义：
`ready | running | terminal | frame_missing | delivery_missing`。RuntimeSession detail 使用
`SessionRuntimeControlView`，原因是 runtime trace/detail 从 runtime session identity 出发，而
AgentRun workspace 从 run / agent identity 出发。

SessionChatView 的职责是执行传入 command，不持有业务分派规则。Enter、Ctrl/Cmd+Enter 与按钮点击
都从 snapshot keyboard/command list 选择 command id；cancel 作为独立命令展示。这样 running
workspace 可以同时显示排队、运行中 steer 和取消，ready workspace 显示下一轮发送，只读 trace
展示后端 reason。

`ConversationCommandView.stale_guard.snapshot_id` 是一次 workspace projection 的不透明前置条件。
前端提交 AgentRun command 时把 command id、kind 与 stale guard 原样回传；后端用当前 run / agent /
frame / runtime / active turn 事实重新计算 snapshot identity。这样投影刷新滞后时，旧 running
命令会成为结构化 stale conflict，前端可以刷新并使用最新 snapshot 的 replacement command，而不是
把运行态差异渲染成普通用户错误。

## AgentRun Workspace 状态来源

AgentRun Workspace 的 title、status、list entry 和 action state 来自后端提供的
AgentRun Workspace projection。该 projection 面向用户工作台 shell，聚合 ProjectAgent display
name、Subject association、LifecycleAgent、AgentFrame、active turn、delivery summary、command
receipt 与 workspace activity 时间。

Delivery-backed AgentRun 的工作台标题由后端 `AgentRunWorkspaceShell.display_title` 承接
RuntimeSession `SessionMeta.title` / `title_source`，原因是用户可见的会话标题随 runtime trace
更新，而前端 header、侧栏快捷入口和 AgentRun 列表仍应消费同一个 workspace shell 投影。没有
delivery RuntimeSession meta 的 workspace 再使用 AgentRun/workspace fallback title。

RuntimeSession trace metadata 仍进入 trace/feed/debug 展示：事件游标、trace title provenance、
delivery trace summary、last turn pointer、terminal summary 和 executor continuation 都属于
runtime trace 视角。Workspace route 可以展示关联的 `delivery_trace_meta` 或 trace link，但
侧栏列表、工作台标题、运行状态、最近活动和按钮 enablement 以 AgentRun Workspace projection /
`AgentRunWorkspaceView.actions` 为准。

同一 `run_id + agent_id + source_key` 的 AgentRun Workspace refresh 保留上一帧 `workspace`、
`runtime_session_id`、resource surface 与 frame，原因是 `SessionChatView` 的 NDJSON stream
生命周期绑定 runtime session identity，右侧 resource browser 也需要展示连续性。输入区 command
authority 只在当前 projection `status="ready"` 时消费最新 `AgentConversationSnapshot.commands`；
`loading` / `refreshing` / `error` / stale projection 状态下上一帧 snapshot 只能用于展示诊断。

`session_meta_updated`、`Platform(SessionMetaUpdate)` 与 RuntimeSession event stream 仍是 feed
和 debug 面板可渲染的事实。工作台标题编辑和状态刷新通过 AgentRun Workspace shell 刷新或后续
AgentRun shell event 进入 store，原因是用户可见工作台 shell 与 trace metadata 的更新节奏和事实源
不同。

---

## Projection Store 写后刷新

HTTP-only projection store（如 `extensionRuntimeStore` 缓存的 `ExtensionRuntimeProjectionResponse`）没有 SSE / NDJSON 失效流。**任何会改变底层实体的写操作（HTTP POST/DELETE 等），调用方必须在 success 分支显式调 `store.fetchProject(projectId)` 触发重拉**，不能依赖局部 patch 或 optimistic update。

**为什么**：projection 由后端聚合多张表（installation / artifact / runtime action / workspace tab / permission / bundle）派生，前端无法本地推导；漏 refetch 会造成"写完了但 UI 还是旧数据"，或更糟：不同入口看到的投影不一致。

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
