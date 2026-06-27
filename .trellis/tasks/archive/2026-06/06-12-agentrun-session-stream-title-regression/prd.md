# AgentRun 会话界面重连与标题回归修复

## Goal

修复 AgentRun 会话界面的两个合并后回归：进入实际会话界面后 NDJSON 会话流持续刷新重连，以及原本由 SessionMeta 承载的会话标题不再作为 AgentRun 工作台和列表的显示标题。修复目标是让 AgentRun 工作台以稳定 RuntimeSession 身份承载交互界面，并让用户可见标题回到 RuntimeSession/trace meta 的权威来源。

## User Report

- 进入实际会话界面后，后端反复打印 `Session trace stream 连接建立（NDJSON）` 和 `历史补发完成（NDJSON）`，同一 `session_id` 在几十毫秒内多次重连，`resume_from` 持续递增。
- 原本挂载在 `sessionMeta` 上的 session title 现在没有承载和显示。

## Confirmed Facts

- 前端 `AgentRunWorkspacePage` 会在 `session_meta_updated`、`context_frame/capability_state_update`、workspace module 展示等系统事件后调用 `refreshAgentRunWorkspaceState()`。
- `useAgentRunWorkspaceState.refreshWorkspaceState()` 会调用同一个 `loadWorkspaceState()`，而 `loadWorkspaceState()` 在请求前会把 state 重置为 `emptyAgentRunWorkspaceState()`。
- 重置期间 `runtime_session_id` 临时变为 `null`，`SessionChatView` 因 `sessionId === null` 将 `useSessionFeed` 置为 disabled，进而触发 `useSessionStream` cleanup 关闭当前 NDJSON transport。
- 请求返回后同一个 `runtime_session_id` 恢复，`SessionChatView` 重新创建流连接。日志中的几十毫秒级重连与这个 React 状态空窗一致，不符合 transport 自身 800ms 起步的指数退避重连节奏。
- 后端 `runtime_trace_meta()` 仍然从 `SessionMeta.title` 生成 `delivery_trace_meta.trace_title`。
- AgentRun 工作台 header、快捷列表和 ActiveAgentRun 列表显示的是 `shell.display_title`。当前后端 `resolve_workspace_title()` 从 ProjectAgent 名称或 `AgentRun {agent_id}` 生成该字段，没有承接 `SessionMeta.title`。

## Requirements

- 刷新 AgentRun workspace projection 时必须保持当前 RuntimeSession identity 稳定，不能在同一 `runId/agentId/sourceKey` 的普通刷新期间清空 `runtime_session_id`、`workspace` 或导致会话聊天组件关闭。
- 初始加载、路由目标切换和错误状态仍要有明确状态表达；但 refresh 失败不能破坏上一帧可用的会话交互界面。
- AgentRun workspace shell 的用户可见标题必须优先承接 delivery RuntimeSession 的 `SessionMeta.title`/title source；没有 delivery runtime meta 时才使用 AgentRun/workspace 自身标题。
- AgentRun 工作台 header、快捷入口、ActiveAgentRun 列表应通过同一后端 shell 契约得到一致标题，不在多个前端入口各自补丁。
- 不做兼容性兜底或旧模型迁就；本项目仍处预研阶段，允许直接调整契约和测试。
- 删除或重写与错误模型绑定的旧测试，不为了维持旧行为而保留伪兼容测试。

## Acceptance Criteria

- [ ] 同一 AgentRun 工作台收到 `session_meta_updated` 或 capability state update 后，`runtime_session_id` 在刷新期间保持稳定，`SessionChatView` 不会因为 `sessionId` 临时为 `null` 而关闭 NDJSON stream。
- [ ] `useAgentRunWorkspaceState` 有覆盖 refresh 期间保留 runtime identity/workspace 的前端测试。
- [ ] AgentRun workspace API 在存在 delivery RuntimeSession meta 时，`shell.display_title` 与 `shell.title_source` 来自 session meta。
- [ ] AgentRun workspace list entry 使用同一 shell 标题契约，列表和详情标题一致。
- [ ] 相关契约类型和生成的前端 TS 类型保持同步。
- [ ] 必要的前端测试、后端测试或轻量验证通过。
- [ ] 修复后没有引入旧 Session 本体兼容层或新的双标题事实源。

## Out Of Scope

- 不处理与本回归无关的 AgentRun/Session 更大规模命名清理。
- 不改造 NDJSON stream 协议本身，除非验证中发现协议层独立缺陷。
- 不新增数据库迁移，除非实现中发现 title 契约需要持久化字段调整。
