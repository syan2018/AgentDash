# Design

## Architecture Boundary

本修复落在 AgentRun workspace projection 与 RuntimeSession trace 的交界处。RuntimeSession 是会话交互界面的运行时身份，AgentRun workspace 是承载该运行时的工作台投影。普通 workspace refresh 只能更新投影内容，不能让 RuntimeSession identity 在 UI 树中短暂消失。

标题契约同样收在后端 workspace shell：`AgentRunWorkspaceShell.display_title` 是前端列表和详情的统一显示标题，存在 delivery RuntimeSession meta 时应直接表达 trace/session title。前端不再在 header、shortcut list、active list 中各自选择 title 来源。

## Runtime State Refresh

`useAgentRunWorkspaceState` 需要区分两种加载：

- Initial load / route identity change: 可以清空为 loading，因为当前页面还没有可用 projection，或者路由身份已经变化。
- Refresh same identity: 保留上一帧 `workspace`、`runtime_session_id`、`frame`、`runtime_surface` 和相关可用数据，只更新 `status`、`error`、`runtime_surface_error`；请求成功后原子替换为最新 projection；请求失败时保留上一帧并进入 error/ready-with-error 表达，不能使聊天组件失去 `sessionId`。

关键不变量：

- `run_id + agent_id + source_key` 未变化时，refresh 不得将 `runtime_session_id` 从非空置为 `null`。
- `SessionChatView` 的 `sessionId` 只应在真实 runtime 切换、路由切换或 delivery runtime 消失时变化。
- refresh 可以触发 workspace panel/runtime surface 更新，但不应驱动会话流生命周期重置。

## Title Contract

后端 `build_agent_run_workspace_view()` 已经读取了 runtime `SessionMeta` 并生成 `delivery_trace_meta`。构造 `AgentRunWorkspaceShell` 时应选择：

1. 如果存在 `SessionMeta`，`display_title = meta.title`，`title_source = serialized_string(meta.title_source)`。
2. 如果不存在 delivery meta，使用 AgentRun/workspace title resolver。

`resolve_workspace_title()` 可以保留为无 delivery meta 的 workspace title resolver，但不再覆盖 delivery trace title。列表 entry 由 `AgentRunWorkspaceView` 派生，因此修复 workspace shell 后列表自动一致。

## Contract Generation

如 Rust contract 字段未变化，可能不需要重新生成 TS 类型；如果新增字段或调整 `RuntimeSessionTraceMeta` 时间字段，则必须同步生成 `packages/app-web/src/generated/*`。本任务优先不新增字段，仅修复现有 title 和 refresh 行为。

## Tests

前端：

- 新增或补充 `useAgentRunWorkspaceState` hook 测试，模拟初始加载成功后触发 refresh，在 refresh promise pending 期间断言 `runtime_session_id` 和 `workspace` 仍保留。
- 断言 refresh 成功后 projection 更新，refresh 失败时不会清空已存在 runtime identity。

后端：

- 覆盖 workspace shell title resolver：存在 `SessionMeta` 时 shell title/source 来自 session meta；缺少 meta 时仍使用 AgentRun/workspace resolver。
- 如果现有 route 测试成本过高，可优先提取纯函数测试 title selection，避免为了测试而搭建完整 API 状态。

## Risk Notes

- `refreshWorkspaceState()` 当前同时服务 hook runtime refresh alias，修改时要确认 `refreshHookRuntime` 不依赖清空状态触发 UI loading。
- `runtime_surface` 刷新失败不应误伤会话聊天；但错误仍需通过 `runtime_surface_error` 暴露给 workspace panel。
- 前端状态类型中 `status: "error"` 如果配合保留 workspace，UI 消费端应能接受；必要时选择保留 status 为 `ready` 并设置 error 字段，但需与现有状态语义一致。
