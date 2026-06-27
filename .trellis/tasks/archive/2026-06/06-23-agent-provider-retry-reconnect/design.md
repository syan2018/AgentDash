# Agent Provider Retry, Reconnect, And Turn Recovery Design

## Scope

本设计覆盖 PiAgent provider 波动处理、AgentRun/session 运行状态事件、失败轮次恢复、turn 耗时记录与前端展示。目标是采用 pi-mono 的运行机制、Codex 的前端提示语义，并保持 AgentDash Backbone / session event store / AgentRun mailbox 边界一致。

不在本任务中做 mid-stream retry。任何 assistant delta、reasoning delta 或 tool delta 一旦进入 Backbone，当前 provider attempt 就不可静默重放。

## Architecture

### Layer Responsibilities

| Layer | Responsibility |
| --- | --- |
| Bridge / provider | 识别 provider HTTP/transport/SSE 暂态错误；在首个可见 delta 前做有限 retry；保留 retry classification、attempt、delay、provider details。 |
| Agent loop | 将 provider error 规范化为可分类的 assistant error 或 runtime error；追踪当前 attempt 是否已经产生可见 delta；发出 provider status / retry lifecycle AgentEvent。 |
| PiAgent connector mapper | 把 AgentEvent 映射为 Backbone `Error(will_retry=true)` 与结构化 platform status，不把重连状态写成 assistant message。 |
| Session / AgentRun data layer | 记录 turn start ms、terminal ms、duration ms；失败后丢弃最后未稳定轮次对下一次上下文的影响；恢复到上一稳定提交边界。 |
| Frontend feed | 使用 Codex 风格展示 thinking/reconnecting/retry exhausted；消费 `durationMs` 显示 turn elapsed；后端回滚或修剪后刷新权威 projection。 |

### Event Model

优先复用 Codex wire shape：

- `BackboneEvent::Error(codex::ErrorNotification { will_retry: true })` 表达 retry/reconnect 中间态。
- `BackboneEvent::TurnStarted(codex::TurnStartedNotification)` 表达 turn 已开始，带 `startedAt`。
- `BackboneEvent::TurnCompleted(codex::TurnCompletedNotification)` 表达 terminal，必须带 `durationMs`；现有 `Platform(SessionMetaUpdate key="turn_terminal")` 同步写入 terminal diagnostics，供现有刷新链路和诊断读取。

Codex 没有 public `stream_connected` / `waiting_for_first_token` / durable `thinking` 一等事件，只有 `TurnStatus::InProgress`、首个 agent/reasoning delta、terminal `duration_ms` / `time_to_first_token_ms` 与 TUI 本地 `Working/Thinking` 状态。因此 AgentDash 自有 provider 状态使用一等结构化 `PlatformEvent::ProviderAttemptStatus`，不写自由文本业务语义：

```json
{
  "turn_id": "...",
  "attempt": 1,
  "max_attempts": 3,
  "phase": "connecting|connected_waiting_first_delta|streaming|retry_scheduled|retrying|failed|succeeded",
  "will_retry": true,
  "delay_ms": 2000,
  "message": "Reconnecting... 1/3",
  "provider": "...",
  "model": "...",
  "reason_code": "stream_disconnected"
}
```

失败轮次恢复同样使用一等 `PlatformEvent::SessionRewound`，而不是把关键业务状态塞进自由 key。这样 Rust/TS 绑定、前端 reducer 和测试都能围绕稳定类型表达语义。

Retry/reconnect UI 采用 Codex 规则：

- `ErrorNotification { will_retry: true }` 是非终态中间事件，不更新 turn terminal summary，不进入普通红色 error 历史。
- attempt/max/delay/provider/source 不从 `ErrorNotification.error.message` 解析；这些细节由结构化 provider status event 承载。
- `ErrorNotification { will_retry: false }` 或 terminal failed/lost 才是终态错误。

### Retry Classification

Bridge/provider 层应提供结构化分类：

- retryable transient: 429、5xx、timeout、connection reset/refused、fetch/reqwest transport、SSE empty-before-delta、stream ended before first visible delta、provider overloaded/service unavailable。
- fatal: invalid API key/auth expired without refresh path、quota/usage limit、context window exceeded、invalid request/schema/tool payload。
- abort/cancel: 用户取消或 runtime cancel，不计 retry。

延迟来源：

- 优先使用 provider `Retry-After`、rate-limit reset header 或 body 中 retry delay。
- 再使用指数退避。
- server-requested delay 超过 cap 时转为可见失败，让用户理解为什么没有长时间挂起。

### First Visible Delta Boundary

Agent loop 在每次 provider attempt 内维护 `has_visible_delta`：

- `TextDelta`、`ReasoningDelta`、`ToolCallDelta`、`ToolCall` 任一非空可见事件出现后设为 true。
- `Done` 前没有任何可见事件但 provider 返回空响应时，视为 first-delta-before failure，可 retry。
- `Error` 发生且 `has_visible_delta=false`：可进入 retry path。
- `Error` 发生且 `has_visible_delta=true`：不 retry，触发失败轮次恢复。

### Session Retry Flow

对齐 pi-mono：

```text
agent prompt
  -> provider attempt fails before visible delta
  -> AgentEvent::ProviderRetryStarted
  -> ErrorNotification(will_retry=true)
  -> remove runtime error assistant / no persisted assistant pollution
  -> sleep retry delay
  -> agent continue / restart provider request
  -> retry succeeds or exhausts
  -> AgentEvent::ProviderRetryEnded
  -> prompt/run waits until full loop settles
```

在 AgentDash 中，是否使用 `agent.continue()` 还是重新进入 `stream_assistant_response` 内部 loop，需要实现阶段根据当前 Rust agent loop结构决定。机制目标是：retry 不新增用户输入、不重复持久化 user input、不让错误 assistant 留在下一次 provider request context。

### Failed Turn Recovery

恢复原则：最后一个未稳定完成的 turn 失败后，数据层恢复到上一稳定边界，让下一次 AgentRun 可以重新开始。

稳定边界：

- 已持久化 `turn_completed` / `turn_terminal: turn_completed` 的 turn 是稳定边界。
- 当前 active turn 的 `turn_started` 后、terminal failed/lost/interrupted 前产生的 assistant/tool/context 增量属于未稳定轮次。
- failed turn 中的 `UserInputSubmitted` 是用户曾提交的审计事实，但不应作为下一次 provider request 的模型上下文输入自动保留；真正下一次提交由 AgentRun mailbox/command receipt 重新进入 launch/steer 路径。

建议实现分两级：

1. **模型上下文恢复**：repository restore / continuation / projection builder 排除最后 failed/lost/interrupted turn 的 provider-produced events，确保下一次请求上下文干净。
2. **可见 feed 恢复**：前端在收到 terminal failed/lost 或 rollback marker 后刷新 projection/feed；如果后端提供修剪后的 snapshot，以 snapshot 为准丢弃 rawEvents 中最后未稳定 turn。

持久化事件不做物理删除作为第一版策略。当前 `SessionEventStore` 没有 tail truncate / rollback API，`event_seq` 是 NDJSON `x-stream-since-id` 与前端 `lastAppliedSeq` 的恢复游标，`SessionMeta.save` 使用 `GREATEST(last_event_seq)` 也不能靠普通 meta save 回退 head。更稳的事实模型是追加结构化 rollback/stable-boundary marker，让 projection/read model 以稳定边界过滤。

`PlatformEvent::SessionRewound` 建议 shape：

```json
{
  "discarded_turn_id": "...",
  "stable_event_seq": 120,
  "stable_turn_id": "...",
  "reason": "provider_retry|provider_failure|runtime_failure",
  "replacement_turn_id": null,
  "message": "已丢弃失败轮次，恢复到上一稳定状态"
}
```

前端看到 marker 后有两种实现策略：

- 快速路径：full rehydrate，从 `after_seq=0` 重新拉取权威 history/projection。
- 精细路径：新增 reducer action，按 `stable_event_seq` / `discarded_turn_id` 裁剪 `rawEvents` 并 replay。

第一版推荐 full rehydrate，代码风险更低；后续再优化为本地 replay 以保持滚动和折叠状态。

### Mailbox Recovery Policy

Provider retry 耗尽、stream 中途失败、connector stream `Err`、runtime delegate error 等路径在写入 terminal failed/lost/interrupted 与 `SessionRewound` 后，应把 session 恢复到可再次提交的状态。用户需要看到失败诊断，但 composer / mailbox 不应因为一个已完成恢复的失败轮次继续停在 paused 状态。

策略：

- retryable provider failure 在自动 retry 成功时不产生 terminal failed。
- retry 耗尽或 post-delta failure 产生 terminal failed，并追加 `SessionRewound`。
- recovery marker 成功写入后清理 active turn / runtime inflight 状态，使下一次 user prompt 走新的 launch/steer 路径。
- cancel/abort 不自动 retry，但同样不把半截 provider 输出带入下一次模型上下文。
- fatal provider error 保留用户可读诊断，同时恢复到可重新开始提交的状态；下一次是否仍然失败由新的请求和 provider 配置决定。

### Turn Elapsed Time

现有 Codex `Turn` 字段已经包含 `startedAt`、`completedAt`、`durationMs`，前端 `segmentByTurn` 已消费 `durationMs`。

设计要求：

- `TurnExecution` 增加 `started_at_ms`，在 claim/commit turn start 时写入。
- terminal 时计算 `completed_at_ms - started_at_ms`。
- `TurnStartedNotification.turn.startedAt` 使用秒级字段保持 Codex shape；内部计算保留毫秒。
- terminal event 或 `TurnCompletedNotification` 必须携带 `durationMs`。
- `turn_terminal` platform payload 也带 `started_at_ms`、`completed_at_ms`、`duration_ms`，用于现有 refresh path 和诊断。

### Connected Waiting First Delta

新增状态用于填补“connector/provider 已经连上，模型正在思考但还没吐字”的体验空白。Codex 没有 public 一等事件，TUI 是从 turn running 与 stream/reasoning状态本地渲染 `Working/Thinking`；AgentDash 可把这层状态做成结构化 event，避免前端猜测：

```text
turn_started -> provider_connecting -> provider_connected_waiting_first_delta
    -> first visible delta | retrying | failed
```

Bridge 能确认 HTTP/SSE response ok 或 websocket response.create accepted 时发 connected status。若 provider API 不暴露显式 connected，只在 response body/stream reader 建立后发。

前端展示：

- waiting first delta 显示为 Codex 风格的 Thinking/正在思考状态。
- reconnecting 显示 `Reconnecting... attempt/max`。
- retry 成功后恢复 Thinking 或进入正常 streaming。
- retry 耗尽显示错误系统事件，不显示 assistant error。

首字时间可参考 Codex core `time_to_first_token_ms`：AgentDash 可在 first visible delta 时记录 `time_to_first_delta_ms`，terminal 时放入 provider status diagnostics 或 turn terminal payload。它不是必须展示字段，但能帮助后续诊断 provider 延迟。

## Data Flow

```text
Provider bridge
  -> StreamChunk / ProviderAttemptEvent
  -> AgentEvent
  -> PiAgent stream_mapper
  -> BackboneEnvelope
  -> SessionEventingService append/broadcast
  -> session projection + NDJSON
  -> frontend reducer/feed/turn segment
```

失败恢复路径：

```text
Terminal failed/lost/interrupted
  -> SessionMeta terminal summary
  -> append session_rewound/stable-boundary marker
  -> projection stable-boundary filter
  -> AgentRun mailbox pause/resume policy
  -> AgentRun workspace snapshot refresh / frontend full rehydrate
  -> next composer-submit starts from clean context
```

## Compatibility And Migration

项目仍处于预研期，不保留旧错误 assistant 污染行为。需要数据库 migration 时直接把模型调整到正确状态。

如果新增持久化字段：

- `TurnExecution` 是内存态，不需要 migration。
- 第一版不要求 `SessionMeta.last_stable_event_seq` migration；稳定边界可由 append-only marker 或 projection scan 得出。
- 若后续增加 last stable turn/event seq，需要 migration 初始化为空或从现有 terminal facts 回填。
- 若 event store 支持 rollback marker，只需新增 Backbone/platform payload，无数据库字段变更。

## Risks And Trade-offs

- 物理删除 session_events 尾部会影响 `event_seq` resume 语义；第一版不走物理删除。
- 只过滤模型上下文、不刷新前端 rawEvents，会让用户仍看到半截失败轮次；前端需要 terminal/rollback 后刷新或消费 rollback marker。
- 首包前 transparent retry 如果完全静默，用户会误以为卡住；建议第一次 retry 起发 `will_retry=true` 状态。
- 中途失败统一丢弃最后轮次可能也会丢弃部分有用工具结果；这是保持一致可恢复状态的代价。

## Research Incorporated

- `research/codex-turn-status.md`：Codex 有 `duration_ms` / `started_at` / `completed_at` / `time_to_first_token_ms`、`TurnStatus::InProgress`、`ErrorNotification { will_retry: true }` 和 `Reconnecting... n/max`，但没有 public connected/waiting-first-token/durable-thinking 事件。
- `research/agentdash-stable-boundary.md`：AgentDash 当前 store 是 append-only，无 tail truncate API；推荐 append-only rollback/stable-boundary marker + projection filter。
- `research/frontend-retry-feed.md`：前端当前不消费 `willRetry`，`rawEvents` reducer append-only；失败轮次恢复需要显式 rehydrate/rewind 事件。
