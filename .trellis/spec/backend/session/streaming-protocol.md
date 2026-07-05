# NDJSON Streaming Protocol

AgentDash 浏览器前端使用 NDJSON over HTTP 作为统一实时推送协议。跨层不变量见 [Cross-layer Architecture](../../cross-layer/architecture.md)。

## Endpoints

Project 级事件流：

```text
GET /api/events/stream/ndjson?project_id=<uuid>
Header: x-stream-since-id: <i64> (optional)
```

AgentRun journal 事件流：

```text
GET /agent-runs/{run_id}/agents/{agent_id}/journal/stream/ndjson
Header: x-stream-since-id: <u64>
```

Runtime trace 诊断读取通过 `GET /runtime-traces/{runtime_session_id}` 返回只读 trace view；它服务事件、turn 与 frame 诊断，不承担实时产品流。

## Project Stream Contract

- `Content-Type: application/x-ndjson; charset=utf-8`
- 每行一个 `StreamEvent` JSON object。
- `Connected { last_event_id }` 表示连接建立及当前 project-scoped `state_changes.id` 游标。
- `StateChanged(StateChange)` 使用 `state_changes.id` 推进游标。
- `BackendRuntimeChanged { backend_id }` 只触发后端 runtime 状态刷新，不推进 `state_changes` 游标。
- `Heartbeat { timestamp }` 只用于保活。

## Runtime Stream Contract

AgentRun runtime stream 从 run / agent refs 解析当前 delivery RuntimeSession 后，返回同一组 NDJSON envelope。诊断 session stream 从 runtime trace identity 出发，必须通过 `RuntimeSessionExecutionAnchor` 完成 Project `Use` 校验。两条入口共享 event envelope，产品前端优先使用 AgentRun runtime stream。

Product AgentRun stream 的事件读取必须经过 `AgentRunJournalService::subscribe_visible_journal_stream`。该服务负责从 AgentRun 当前 delivery RuntimeSession 读取父级可见 lineage 切片、当前 delivery backlog、ephemeral backlog 与 live broadcast，并把所有 durable event 映射为 AgentRun journal sequence。API route 只负责鉴权、resume header 解析、NDJSON envelope 序列化和连接日志，不计算 fork prefix、runtime resume cursor 或跨 RuntimeSession event sequence。

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

## Scenario: AgentRun Journal Stream Contract

### 1. Scope / Trigger

- Trigger: AgentRun 产品会话流需要同时支持普通会话、fork 后父级可见 transcript、当前子 run backlog、ephemeral 进度态与 live 增量。
- Scope: `/agent-runs/{run_id}/agents/{agent_id}/journal/stream/ndjson`、`AgentRunJournalService`、`RuntimeSessionEventingPort::subscribe_after`、前端 `streamTransport` / session reducer。

### 2. Signatures

```rust
pub struct AgentRunJournalQuery {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub delivery_runtime_session_id: Option<String>,
}

pub struct AgentRunJournalStreamSubscription {
    pub state: AgentRunJournalStreamState,
    pub live_events: broadcast::Receiver<PersistedSessionEvent>,
}

pub struct AgentRunJournalStreamState {
    pub journal_session_id: String,
    pub delivery_runtime_session_id: String,
    pub resume_from: u64,
    pub connected_seq: u64,
    pub ephemeral_epoch: u64,
    pub prefix_events: Vec<AgentRunJournalEvent>,
    pub backlog_events: Vec<AgentRunJournalEvent>,
    pub ephemeral_backlog_events: Vec<AgentRunJournalEvent>,
}

pub enum AgentRunJournalLiveEvent {
    Durable(AgentRunJournalEvent),
    Ephemeral(AgentRunJournalEvent),
    StaleDurable,
}
```

### 3. Contracts

- `journal_session_id` is `agentrun:{run_id}:{agent_id}` and is written into streamed `PersistedSessionEvent.session_id` and `BackboneEnvelope.session_id`.
- Parent lineage events and child delivery events share one monotonic `journal_seq`; raw RuntimeSession `event_seq` remains source metadata only.
- `x-stream-since-id` / `since_id` are AgentRun journal cursors. The service translates them to the current delivery runtime cursor after resolving prefix length.
- `connected.last_event_id` is an AgentRun journal cursor and must include any inherited prefix plus current delivery snapshot.
- Ephemeral events keep ephemeral envelope semantics; they are projected into AgentRun coordinates but do not advance durable cursor.
- Fork markers are not synthesized by the journal. The fork event comes from the child RuntimeSession durable `session_branch_forked` Backbone event created by runtime branching.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| AgentRun has no delivery RuntimeSession | Return application not-found/conflict style error before streaming. |
| `x-stream-since-id` is invalid | Return 400 before opening the stream. |
| Resume cursor lands inside inherited prefix | Replay remaining prefix events, then current delivery backlog. |
| Resume cursor is after prefix | Subscribe to current delivery after `resume_from - prefix_len`. |
| Live durable event sequence is at or before subscription snapshot | Drop as `StaleDurable`; do not duplicate backlog. |
| Live event is ephemeral | Emit `ephemeral_event`; do not advance durable cursor. |
| Broadcast receiver lags | Log warning and keep the stream alive. |

### 5. Good/Base/Bad Cases

- Good: Forked AgentRun opens with parent visible transcript, exactly one `session_branch_forked` event from child event seq 1, and child events continue as journal seq N+1.
- Good: Refresh with `x-stream-since-id` equal to the last parent prefix event replays child event seq 1 as the next AgentRun journal event.
- Base: Plain AgentRun has no inherited prefix; journal seq equals current delivery event seq.
- Bad: API route computes `prefix_len + event.event_seq`, because route code would become a second journal implementation.
- Bad: Frontend deduplicates fork markers by payload shape, because fork visibility belongs to the AgentRun journal service.

### 6. Tests Required

- `agent_run::journal` unit tests assert parent prefix + child delivery produce monotonic journal seq.
- Forked journal tests assert `session_branch_forked` appears exactly once and is sourced from child runtime event seq 1.
- Stream tests assert `subscribe_visible_journal_stream` maps resume cursor to runtime cursor and returns AgentRun journal seq for backlog/live events.
- API check must compile without route-local prefix/runtime sequence helpers in AgentRun journal stream.

### 7. Wrong vs Correct

#### Wrong

```rust
let prefix = journal.load_inherited_prefix(query).await?;
let prefix_len = prefix.events.last().map(|event| event.journal_seq).unwrap_or(0);
let subscription = session_eventing.subscribe_after(runtime_session_id, resume_from - prefix_len).await?;
let journal_seq = prefix_len + event.event_seq;
```

#### Correct

```rust
let subscription = agent_run_journal
    .subscribe_visible_journal_stream(query, resume_from)
    .await?;

match subscription.state.project_live_event(event) {
    AgentRunJournalLiveEvent::Durable(event) => emit_event(event),
    AgentRunJournalLiveEvent::Ephemeral(event) => emit_ephemeral(event),
    AgentRunJournalLiveEvent::StaleDurable => {}
}
```

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
