# SSE/NDJSON 流式协议

> AgentDash 服务端流式推送协议的跨层契约。
> 从 `error-handling.md` 拆分，独立维护。

---

## Scenario: SSE/NDJSON 流式契约

### 1. Scope / Trigger

- 触发条件：
  - 新增 API 签名：`/api/events/stream/ndjson`、`/api/acp/sessions/{id}/stream/ndjson`
  - 变更跨层契约：服务端流式 envelope + 前端 transport（fetch ndjson -> sse fallback）
  - 变更 resume 行为：全局流 `Last-Event-ID`，会话流 `x-stream-since-id`
- 影响层：
  - Backend route/stream implementation
  - Frontend stream hook/transport
  - Dev proxy/HMR 连接生命周期

### 2. Signatures

- 全局 SSE：
  - `GET /api/events/stream`
  - Header: `Last-Event-ID: <i64>`（可选）
- 全局 NDJSON：
  - `GET /api/events/stream/ndjson`
  - Header: `Last-Event-ID: <i64>`（可选）
- ACP 会话 SSE：
  - `GET /api/acp/sessions/{id}/stream`
  - Header: `Last-Event-ID: <u64>`（可选）
  - `data`: `SessionEventEnvelope` JSON
- ACP 会话 NDJSON：
  - `GET /api/acp/sessions/{id}/stream/ndjson`
  - Header: `x-stream-since-id: <u64>`（主方案）
  - Query: `?since_id=<u64>`（兼容）

### 3. Contracts

- `GET /api/events/stream`（SSE）：
  - 每条 `StateChanged` 事件都必须包含 `Event.id(...)`（值来源 `state_changes.id`）
  - 首次连接与重连后，都发送 `Connected { last_event_id }`
  - 心跳：`Heartbeat { timestamp }`
- `GET /api/events/stream/ndjson`：
  - `Content-Type: application/x-ndjson; charset=utf-8`
  - 行内容为 `StreamEvent` JSON，每行一个对象
- `GET /api/acp/sessions/{id}/stream`（SSE）：
  - `Event.id = event_seq`
  - `data` 必须输出 `SessionEventEnvelope`
- `GET /api/acp/sessions/{id}/stream/ndjson`：
  - 连接确认行：`{"type":"connected","last_event_id":<u64>}`
  - 消息行：`{"type":"event","session_id":<string>,"event_seq":<u64>,"occurred_at_ms":<i64>,"committed_at_ms":<i64>,"session_update_type":<string>,"turn_id":<string|null>,"entry_index":<u32|null>,"tool_call_id":<string|null>,"notification":<SessionNotification>}`
  - 心跳行：`{"type":"heartbeat","timestamp":<i64>}`
- `SessionEventEnvelope` 字段语义：
  - `session_update_type`：后端归档的更新类型，前端不应自行猜测
  - `turn_id / entry_index`：chunk 合并与同轮归并锚点
  - `tool_call_id`：tool start/update/end 的稳定归并锚点；不能只依赖 `notification.update.toolCallId`
- Header/缓存契约：
  - 必须返回 `Cache-Control: no-cache, no-transform`
  - 必须返回 `X-Content-Type-Options: nosniff`

### 4. Validation & Error Matrix

| 条件 | 服务端行为 | 客户端行为 |
|------|------------|------------|
| `Last-Event-ID` 非法或缺失 | 按 `0` 处理，不返回 4xx | 正常建立连接 |
| `x-stream-since-id` 非法 | 按 `0` 处理 | 使用全量历史 + 实时 |
| `get_changes_since` 失败 | 记录 `tracing::error!`，连接保持 | 标记重连中并重试 |
| broadcast `Lagged(n)` | 记录 `tracing::warn!` | 不致命，等待后续消息 |
| broadcast `Closed` | 记录关闭日志并结束流 | 触发重连策略 |
| JSON 序列化失败 | 记录 `tracing::error!`，跳过该条 | 不中断整条连接 |

### 5. Good/Base/Bad Cases

- Good：
  - 客户端携带合法 `Last-Event-ID` 或 `x-stream-since-id`
  - 服务端补发缺失消息后进入实时流
  - 前端 UI 显示 `connected`
- Base：
  - 客户端不带 resume header
  - 服务端从当前可读历史开始推送，随后实时流
- Bad：
  - 代理/HMR 频繁断连导致大量 `ECONNRESET`
  - 处理：前端统一连接注册 + HMR dispose close，全局状态显示 `reconnecting` 而非 fatal

### 6. Tests Required

- Backend：
  - `events/stream` 在带 `Last-Event-ID` 时，返回事件 `id` 必须单调递增
  - `events/stream/ndjson` Content-Type 必须是 `application/x-ndjson`
  - `acp/.../stream` 与 `acp/.../stream/ndjson` 都必须输出带完整 trace/tool 锚点的 `SessionEventEnvelope`
  - `x-stream-since-id` 与 `since_id` 同时存在时，header 优先
- Frontend：
  - NDJSON 首次连接失败时，必须自动降级到 SSE
  - 断流后状态应进入 `reconnecting`，恢复后进入 `connected`
  - HMR dispose 时，注册表中的流连接必须全部 close
  - tool 生命周期事件连续到达时，开始态不能在同一批次里被完成态吞掉

### 7. Wrong vs Correct

#### Wrong

- 全局 SSE 只 `data(json)` 不带 `Event.id`，重连后无法准确补发
- Hook 直接绑定 `EventSource` 且无统一连接注册，HMR 后容易泄漏连接

#### Correct

- 全局 SSE 用 `state_changes.id` 作为稳定 `Event.id`，并读取 `Last-Event-ID` 先补发后实时
- 前端通过 transport 抽象（`FetchNdjsonTransport` + `EventSourceTransport` fallback），并接入全局 stream registry
