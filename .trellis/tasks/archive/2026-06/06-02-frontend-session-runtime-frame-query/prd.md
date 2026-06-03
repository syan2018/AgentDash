# 前端 Session Runtime 查询改为后端 Frame Read Model

## Goal

让前端 session runtime state 通过后端 frame read model 查询，而不是遍历本地 lifecycle store 的 frames / agents cache 来猜测 session 对应 frame。

同时为当前 `SessionPage` 补齐可用 Prompt 入口：页面可以从 runtime session route 进入，但发送 prompt 时必须先解析到 Agent / Lifecycle 锚点，再通过对应的 lifecycle、subject 或 agent 派发入口继续执行。

## User Value

- WorkspacePanel 与 SessionPage 展示的 runtime surface 与后端控制面事实一致。
- 前端不再因为 cache 未 hydrate、agent current frame fallback、多个 runtime session refs 而展示错误 frame。
- 用户从 SessionPage 查看运行 trace 时仍能继续输入 prompt，且 prompt 会回到正确的 Agent / Lifecycle 执行入口。
- 后续 FrameLaunchEnvelope 和 runtime anchor 收敛后，前端可直接消费稳定 read model。

## Confirmed Facts

- `useSessionRuntimeState` 当前先遍历 `frames.runtime_session_refs`，找不到就返回任意 agent 的 `current_frame_id`。
- `services/lifecycle.ts` 已有 `fetchAgentFrameRuntime(frameId)` 和 `fetchRuntimeTrace(runtimeSessionId)`。
- 后端 `get_session_trace` 已经能通过 runtime session 找 frame 并返回 trace view。
- `SessionChatView` 已提供 `customSend` 扩展点；有 `sessionId` 但未提供 `customSend` 时不会直接发送 prompt。
- `SessionPage` 当前向 `SessionChatView` 传入 runtime state 和 owner binding，但尚未提供基于 Agent / Lifecycle 锚点的 `customSend` prompt 派发。

## Requirements

- 后端提供 `session_id -> AgentFrameRuntimeView` 的直接 endpoint，推荐 `GET /sessions/{runtime_session_id}/frame-runtime`。
- 前端 `useSessionRuntimeState` 必须调用后端查询，不再本地推断 frame id。
- lifecycle store 可以缓存返回的 frame view，但不能作为事实推断入口。
- freeform session、activity session、missing frame session 都要有清晰 UI state。
- API contract 和 generated TS 同步。
- SessionPage 的 prompt 发送入口应依赖 `session_id -> frame runtime / runtime anchor` 查询结果，得到 run、agent、frame、assignment 或 subject context 后再派发。
- Prompt 派发服务应复用 Agent / Lifecycle / Subject 入口；Session route 只负责定位当前 runtime view 和展示 trace。
- prompt 发送成功后，SessionPage 应刷新 frame runtime、hook runtime 和 trace feed，并保留现有 workspace/context 面板体验。

## Acceptance Criteria

- [ ] 新增或扩展 endpoint：给定 runtime session id 返回 frame runtime view。
- [ ] `useSessionRuntimeState` 删除 `findFrameIdForSession` fallback。
- [ ] 前端 tests 覆盖 session id 查询成功、missing frame、refresh。
- [ ] lifecycle store 缓存 frame，但不通过 cache 选择 frame。
- [ ] SessionPage / WorkspacePanel 运行态展示仍能刷新 context/hook runtime。
- [ ] SessionPage 提供可用 Prompt 输入发送：发送路径使用 Agent / Lifecycle 锚点，不创建 session-first prompt API。
- [ ] SessionChatView 在 SessionPage 中通过 `customSend` 工作，发送后刷新 runtime state 与 hook runtime。

## Out Of Scope

- 不重做整体 SessionPage UI。
- 不在本任务中实现后端 anchor 查询；依赖 anchor task。

## Dependency Notes

- 应在 `runtime-session-frame-assignment-anchor` 后实施，或至少复用其 endpoint/service 设计。
