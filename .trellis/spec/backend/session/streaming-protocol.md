# NDJSON Streaming Protocol

AgentDash 浏览器前端使用 NDJSON over HTTP 作为统一实时推送协议。跨层不变量见 [Cross-layer Architecture](../../cross-layer/architecture.md)。

## Endpoints

Project 级事件流：

```text
GET /api/events/stream/ndjson?project_id=<uuid>
Header: x-stream-since-id: <i64> (optional)
```

会话事件流：

```text
GET /api/acp/sessions/{id}/stream/ndjson
Header: x-stream-since-id: <u64>
Query: since_id=<u64> (direct debugging only)
```

## Project Stream Contract

- `Content-Type: application/x-ndjson; charset=utf-8`
- 每行一个 `StreamEvent` JSON object。
- `Connected { last_event_id }` 表示连接建立及当前 project-scoped `state_changes.id` 游标。
- `StateChanged(StateChange)` 使用 `state_changes.id` 推进游标。
- `BackendRuntimeChanged { backend_id }` 只触发后端 runtime 状态刷新，不推进 `state_changes` 游标。
- `Heartbeat { timestamp }` 只用于保活。

## Session Stream Contract

连接确认行：

```json
{"type":"connected","last_event_id":42}
```

事件行：

```json
{
  "type": "event",
  "session_id": "...",
  "event_seq": 42,
  "occurred_at_ms": 0,
  "committed_at_ms": 0,
  "session_update_type": "agent_message_delta",
  "turn_id": "...",
  "entry_index": 0,
  "tool_call_id": null,
  "notification": {}
}
```

心跳行：

```json
{"type":"heartbeat","timestamp":0}
```

Field semantics:

- `session_update_type`：后端归档的更新类型标签，前端不自行猜测。
- `turn_id / entry_index`：chunk 合并与同轮归并锚点。
- `tool_call_id`：tool start/update/end 的稳定归并锚点。
- `notification`：`BackboneEnvelope`。

## Headers

必须返回：

- `Cache-Control: no-cache, no-transform`
- `X-Content-Type-Options: nosniff`

前端通过 `authenticatedFetch` 注入 Bearer header，保持与普通 API 一致。

## Validation And Errors

| 条件 | 服务端行为 | 客户端行为 |
| --- | --- | --- |
| `x-stream-since-id` 缺失 | 从当前最新游标建立连接 | 正常建立连接 |
| `x-stream-since-id` 非法 | 返回 400 | 标记重连中并重试，开发期查看错误日志 |
| `get_changes_since` 失败 | 记录 `tracing::error!`，连接保持 | 标记重连中并重试 |
| broadcast `Lagged(n)` | 记录 `tracing::warn!` | 不致命，等待后续消息 |
| broadcast `Closed` | 记录关闭日志并结束流 | 触发重连策略 |
| JSON 序列化失败 | 记录 `tracing::error!`，跳过该条 | 不中断整条连接 |

## Frontend Consumption

- 浏览器实时流统一通过 `fetch + ReadableStream` 消费 NDJSON。
- 长连接必须接入前端 stream registry，保证 HMR dispose 和页面切换时关闭连接。
- Project 流和 Session 流都通过 `x-stream-since-id` 恢复缺失事件。
