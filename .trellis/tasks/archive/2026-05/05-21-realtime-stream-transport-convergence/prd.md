# 实时流通路收敛

## Goal

将浏览器前端使用的实时流通路收敛为 NDJSON over HTTP，保留 WebSocket 作为云端后端与本机后端之间的 relay 通道，移除浏览器侧 SSE 作为主路径或备用路径的复杂度。

## User Value

- 前端流式请求统一通过 `fetch` / `authenticatedFetch` 管理鉴权、Abort、重连与 HMR 关闭，减少 `EventSource` query token 与 header 鉴权不一致的问题。
- 用户界面不再暴露 `SSE` 这类实现细节，只表达项目同步或后端状态。
- 代码库内的实时通路职责更清晰：项目级状态同步、会话事件流、执行器选项发现、本机 relay 分别有单一传输策略。

## Confirmed Facts

- 项目级事件流前端当前通过 `EventSource` 连接 `/api/events/stream`，UI 在 Backend 面板底部显示 `事件流 · ...` 与 `SSE`。
- 后端项目级事件流同时提供 `/api/events/stream`、`/api/events/stream/ndjson`、`/api/events/since/{since_id}`；前端只发现 `/events/stream` 被使用，`fetchEventsSince()` 没有调用点。
- 会话流前端默认使用 `/api/acp/sessions/{id}/stream/ndjson`，只有 `VITE_ACP_STREAM_TRANSPORT=sse` 时才使用 `EventSourceTransport`。
- 执行器发现选项已经使用 NDJSON over HTTP。
- `/ws/backend` 是本机后端主动连接云端后端的 relay WebSocket，不属于浏览器前端实时状态通路。
- 项目规则要求预研阶段不保留兼容性方案和回退方案，应让项目保持最正确的状态。

## Requirements

- 浏览器前端项目级事件流必须改为消费 `/api/events/stream/ndjson`。
- 项目级事件流前端应使用 `authenticatedFetch`、`AbortController`、显式重连与 `last_event_id` 游标。
- 前端不得再创建项目级 `EventSource`。
- 会话流前端不得再保留 SSE fallback；只保留 NDJSON transport。
- 后端浏览器侧不再使用的 SSE 路由应从 API router 与实现中移除。
- 项目级 NDJSON 事件格式应支持 `Connected`、`StateChanged`、`BackendRuntimeChanged`、`Heartbeat` 当前语义。
- UI 不应显示 `SSE` 传输细节；项目同步连接状态如仍展示，应以弱化的业务语义呈现。
- Dev proxy 注释与配置应反映 NDJSON 长连接，不再为 `/api/events/stream` 单独维护 SSE 代理规则。
- 相关 spec/docs 应更新为“为什么统一 NDJSON”，不记录旧方案的纠错叙述。

## Acceptance Criteria

- [ ] `rg "new EventSource|EventSourceTransport|VITE_ACP_STREAM_TRANSPORT|/events/stream\"|/acp/sessions/{id}/stream\""` 不再发现浏览器前端主代码中的 SSE 传输路径。
- [ ] 前端项目级事件流通过 NDJSON 建立连接，收到 `StateChanged` 时仍更新 Story/Task store。
- [ ] 前端收到 `BackendRuntimeChanged` 或项目流连接确认后仍触发后端状态刷新。
- [ ] 会话页仍能通过 NDJSON 渲染历史补发、实时消息、终端输出与心跳。
- [ ] 后端 router 不再暴露浏览器侧 SSE stream endpoint。
- [ ] TypeScript 检查、前端相关测试与 Rust 相关检查通过，或记录阻塞原因。
- [ ] `.trellis/spec` 中流式协议契约与实现收敛后的传输策略一致。

## Out of Scope

- 不改变 WebSocket relay 的协议和职责。
- 不重写 BackboneEnvelope / session eventing 的领域模型。
- 不改变 StateChange 的存储结构或数据库 schema。
- 不引入新的实时协议抽象层或多传输 fallback。

## Open Questions

- 无阻塞问题；按项目“预研期不保留兼容方案”的规则，默认删除未使用 SSE 主路径与 fallback。
