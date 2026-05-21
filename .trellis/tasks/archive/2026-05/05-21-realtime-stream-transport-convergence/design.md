# 实时流通路收敛技术设计

## Architecture

浏览器前端只保留一种 HTTP 长连接传输：NDJSON over `fetch`。

- Project realtime bus：`GET /api/events/stream/ndjson`
- Session event stream：`GET /api/acp/sessions/{id}/stream/ndjson`
- Executor discovered options：`GET /api/agents/discovered-options/stream`
- Cloud/local relay：`GET /ws/backend` WebSocket，仅供本机后端连接云端

## Boundaries

- 前端 `eventStore` 负责项目级事件流连接、重连、游标与 store 分发。
- 前端 `streamTransport.ts` 负责 session NDJSON 消费，不再包含 SSE transport 分支。
- 后端 `stream.rs` 只负责项目级 NDJSON 与按需的状态变更查询逻辑。
- 后端 `acp_sessions.rs` 只暴露 session NDJSON stream。
- WebSocket relay 不参与浏览器 UI 状态同步，不纳入此次改造。

## Data Flow

### Project Stream

1. `AppContent` 根据 `currentProjectId` 调用 `useEventStore.connect(projectId)`。
2. `eventStore` 使用 `authenticatedFetch` 请求 `/api/events/stream/ndjson`，请求头携带 `Accept: application/x-ndjson` 与 `x-stream-since-id`。
3. 后端先发送 `Connected { last_event_id }`，再推送 `StateChanged`、`BackendRuntimeChanged`、`Heartbeat`。
4. 前端逐行解析 NDJSON：
   - `Connected`：更新游标、标记连接成功、刷新后端列表。
   - `StateChanged`：更新游标、调用 `storyStore.handleStateChange()`。
   - `BackendRuntimeChanged`：刷新后端列表。
   - `Heartbeat`：保持连接，不触发业务更新。
5. 连接结束或异常时，前端按指数退避重连，并用游标补发缺失事件。

### Session Stream

Session stream 保持当前 NDJSON 消费格式：

- `connected`
- `event` / `notification`
- `heartbeat`

删除 `EventSourceTransport` 后，`createSessionStreamTransport()` 直接返回 `FetchNdjsonTransport`。

## API Contracts

### Project NDJSON

`GET /api/events/stream/ndjson?project_id=<uuid>`

Headers:

- `Accept: application/x-ndjson`
- `x-stream-since-id: <i64>` 可选

Response:

- `Content-Type: application/x-ndjson; charset=utf-8`
- 每行一个 `StreamEvent` JSON，保持 `#[serde(tag = "type", content = "data")]`。

### Removed Browser SSE Endpoints

- `GET /api/events/stream`
- `GET /api/acp/sessions/{id}/stream`

## Compatibility And Migration

本项目处于预研期，前端与后端同步更新，不保留 SSE fallback、不提供 API 兼容迁移窗口。文档与代码同步收敛到 NDJSON。

## Operational Notes

- HMR dispose 继续通过 `streamRegistry` 关闭所有长连接。
- Dev proxy 保留 `/api` 的长连接超时关闭配置；删除 `/api/events/stream` 专用 SSE 规则。
- 由于 project stream 从 `EventSource` 转为 `fetch`，鉴权从 query token 回到统一 `authenticatedFetch` header 体系。

## Rollback Considerations

如果 NDJSON project stream 出现问题，优先修复 NDJSON 消费、重连或后端游标逻辑；不恢复 SSE fallback。
