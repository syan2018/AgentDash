# NDJSON 流式协议

> AgentDash 浏览器前端使用 NDJSON over HTTP 作为统一实时推送协议。

---

## Scenario: NDJSON 流式契约

### 1. Scope / Trigger

- 触发条件：
  - 新增或变更 `/api/events/stream/ndjson`
  - 新增或变更 `/api/acp/sessions/{id}/stream/ndjson`
  - 变更服务端流式 envelope、前端 transport、resume 行为或 HMR 连接生命周期
- 影响层：
  - Backend route/stream implementation
  - Frontend stream hook/transport
  - Dev proxy/HMR 连接生命周期

### 2. Signatures

- Project 级事件流：
  - `GET /api/events/stream/ndjson`
  - Query: `project_id=<uuid>`
  - Header: `x-stream-since-id: <i64>`（可选）
- 会话事件流：
  - `GET /api/acp/sessions/{id}/stream/ndjson`
  - Header: `x-stream-since-id: <u64>`（主方案）
  - Query: `?since_id=<u64>`（用于直接调试）

### 3. Contracts

- `GET /api/events/stream/ndjson`：
  - `Content-Type: application/x-ndjson; charset=utf-8`
  - 行内容为 `StreamEvent` JSON，每行一个对象
  - `Connected { last_event_id }` 表示连接建立及当前 project-scoped `state_changes.id` 游标
  - `StateChanged(StateChange)` 使用 `state_changes.id` 推进游标
  - `BackendRuntimeChanged { backend_id }` 只触发后端 runtime 状态刷新，不推进 `state_changes` 游标
  - `Heartbeat { timestamp }` 只用于保活
- `GET /api/acp/sessions/{id}/stream/ndjson`：
  - 连接确认行：`{"type":"connected","last_event_id":<u64>}`
  - 消息行：`{"type":"event","session_id":<string>,"event_seq":<u64>,"occurred_at_ms":<i64>,"committed_at_ms":<i64>,"session_update_type":<string>,"turn_id":<string|null>,"entry_index":<u32|null>,"tool_call_id":<string|null>,"notification":<BackboneEnvelope>}`
  - 心跳行：`{"type":"heartbeat","timestamp":<i64>}`
- 会话事件字段语义：
  - `session_update_type`：后端归档的更新类型标签（如 `agent_message_delta` / `item_completed`），前端不应自行猜测
  - `turn_id / entry_index`：chunk 合并与同轮归并锚点
  - `tool_call_id`：tool start/update/end 的稳定归并锚点
  - `notification`：`BackboneEnvelope`（含 `event: BackboneEvent`、`source: SourceInfo`、`trace: TraceInfo`）
- Header/缓存契约：
  - 必须返回 `Cache-Control: no-cache, no-transform`
  - 必须返回 `X-Content-Type-Options: nosniff`

### 4. Validation & Error Matrix

| 条件 | 服务端行为 | 客户端行为 |
|------|------------|------------|
| `x-stream-since-id` 缺失 | 从当前最新游标建立连接 | 正常建立连接 |
| `x-stream-since-id` 非法 | 返回 400，暴露调用方错误 | 标记重连中并重试，开发期查看错误日志 |
| `get_changes_since` 失败 | 记录 `tracing::error!`，连接保持 | 标记重连中并重试 |
| broadcast `Lagged(n)` | 记录 `tracing::warn!` | 不致命，等待后续消息 |
| broadcast `Closed` | 记录关闭日志并结束流 | 触发重连策略 |
| JSON 序列化失败 | 记录 `tracing::error!`，跳过该条 | 不中断整条连接 |

### 5. 关键要求

- 浏览器前端实时流统一通过 `fetch + ReadableStream` 消费 NDJSON。
- 长连接必须接入前端 stream registry，保证 HMR dispose 和页面切换时关闭连接。
- Project 流和 Session 流都通过 `x-stream-since-id` 恢复缺失事件。
- 鉴权走 `authenticatedFetch` 的 Bearer header 注入，保持与普通 API 一致。

### 6. Good / Base / Bad Cases

- Good：前端用 `authenticatedFetch()` 请求 `/api/events/stream/ndjson`，携带 `Accept: application/x-ndjson` 与 `x-stream-since-id`，逐行解析 `StreamEvent` 后只把业务事件交给 store。
- Base：刷新页面后以 `x-stream-since-id: 0` 建立会话流，后端先回放历史，再发送 `connected` 行和后续实时事件。
- Bad：浏览器前端用 query token 建立长连接，或在项目流、会话流之间保留多套 fallback transport，导致鉴权、关闭和重连语义分叉。

### 7. Tests Required

- Frontend typecheck：确认 project/session transport 类型不依赖浏览器 SSE API。
- Frontend tests：确认 session UI 聚合逻辑仍能消费 NDJSON 事件。
- Backend check/test：确认删除 SSE handler 后 router、imports、NDJSON handlers 仍编译并通过 API 单元测试。
- Reference scan：确认浏览器主代码中没有 `new EventSource`、`EventSourceTransport`、`VITE_ACP_STREAM_TRANSPORT`、`/events/since`。

### 8. Wrong vs Correct

#### Wrong

```typescript
const source = new EventSource(`/api/events/stream?project_id=${projectId}&token=${token}`);
```

#### Correct

```typescript
const response = await authenticatedFetch(`/api/events/stream/ndjson?project_id=${projectId}`, {
  headers: {
    Accept: "application/x-ndjson",
    "x-stream-since-id": String(lastEventId),
  },
});
```
