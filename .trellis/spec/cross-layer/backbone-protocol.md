# Backbone Protocol

Backbone Protocol 是 AgentDash 内部 session 事件流的统一传输协议。所有 connector 输出都必须映射为同一套 `BackboneEnvelope` / `BackboneEvent`。

## Positioning

`BackboneEnvelope` 是平台内部持久化、NDJSON 推送和前端消费的事件 envelope。外部协议 adapter 可以存在，但进入 AgentDash 主链路前必须转换为 Backbone。

## BackboneEnvelope

定义位置：`crates/agentdash-agent-protocol/src/backbone/envelope.rs`

字段：

- `event: BackboneEvent`
- `session_id`
- `source: SourceInfo`
- `trace: TraceInfo`
- `observed_at`

`SourceInfo` 包含 connector_id / connector_type / executor_id。`TraceInfo` 包含 turn_id / entry_index。

## BackboneEvent

定义位置：`crates/agentdash-agent-protocol/src/backbone/event.rs`

```rust
pub enum BackboneEvent {
    AgentMessageDelta(codex::AgentMessageDeltaNotification),
    ReasoningTextDelta(codex::ReasoningTextDeltaNotification),
    ReasoningSummaryDelta(codex::ReasoningSummaryTextDeltaNotification),
    ItemStarted(ItemStartedNotification),
    ItemCompleted(ItemCompletedNotification),
    CommandOutputDelta(codex::CommandExecutionOutputDeltaNotification),
    FileChangeDelta(codex::FileChangeOutputDeltaNotification),
    McpToolCallProgress(codex::McpToolCallProgressNotification),
    TurnStarted(codex::TurnStartedNotification),
    TurnCompleted(codex::TurnCompletedNotification),
    TurnDiffUpdated(codex::TurnDiffUpdatedNotification),
    UserInputSubmitted(UserInputSubmittedNotification),
    TurnPlanUpdated(codex::TurnPlanUpdatedNotification),
    PlanDelta(codex::PlanDeltaNotification),
    TokenUsageUpdated(ThreadTokenUsageUpdatedNotification),
    ThreadStatusChanged(codex::ThreadStatusChangedNotification),
    ExecutorContextCompacted(codex::ContextCompactedNotification),
    ApprovalRequest(ApprovalRequest),
    Error(codex::ErrorNotification),
    Platform(PlatformEvent),
}
```

序列化采用 `#[serde(tag = "type", content = "payload", rename_all = "snake_case")]`。

## User Input Facts

用户提交到 session 的输入使用 `BackboneEvent::UserInputSubmitted` 表达。payload 携带 Codex app-server protocol 的 `Vec<UserInput>`、`turn_id`、稳定 `item_id` 与 AgentDash 的 `submission_kind`（`prompt` / `steer`）。

这个事件是普通 prompt 与运行中 steer 的共同事实来源，原因是 Codex thread history 通过显式 turn boundary 和同 turn 内多个 user message 表达 mid-turn steering。AgentDash 在 Backbone 层保留同样的 `UserInput` 形态，projection、NDJSON、前端 feed 和 recall surface 才能用同一个 item 坐标区分“开启 turn 的输入”和“运行中 steer 输入”。

ACP 或其他外部 adapter 进入主链路时需要先转换为 `UserInputSubmitted`。`PlatformEvent` 只承载 Codex 原生协议没有覆盖的平台能力；用户输入属于 turn/thread 语义，不属于 platform metadata。

## Token Usage Semantics

`TokenUsageUpdated` 使用 AgentDash 自有的 normalized payload 包装 provider usage。该 payload 保留 Codex `ThreadTokenUsage.last` 与 `ThreadTokenUsage.total` 的差异，并额外给出 `context`：

- `provider_context_tokens` 表示最近一次 provider usage 可确认的模型可见输入压力。
- `pending_estimate_tokens` 表示最近一次 provider usage 之后新增上下文的本地估算。
- `current_context_tokens` 是状态栏、上下文环和压缩判断共同使用的当前压力值。
- `cumulative_total_tokens` 表示 session 累计消耗，只服务统计与成本类展示。
- `model_context_window` 表示 provider/model 暴露的原始窗口。
- `effective_context_window` 表示扣除策略预算后用于判断的窗口。
- `reserve_tokens` 表示输出、工具调用或摘要预留预算。

这些字段在 Backbone 层拆开，是因为 provider usage 同时承载 billing、cache、最近一次请求和累计 session 信息。进入主事件流后，展示层和决策层必须能选择正确口径，而不是从累计值反推当前上下文压力。

## Thread Items

Backbone item lifecycle 使用 `AgentDashThreadItem`：

```rust
pub enum AgentDashThreadItem {
    Codex(codex::ThreadItem),
    AgentDash(AgentDashNativeThreadItem),
}
```

Codex 已有的 item 语义保持原生 `ThreadItem` wire shape。AgentDash 自有 item 当前覆盖
`fsRead`、`fsGrep`、`fsGlob`，用于表达 Codex Protocol 尚未提供一等 variant 的
read/search/list 工具事实。

Codex `fileChange` 是文件修改的统一 item 语义；AgentDash `fs_apply_patch` 进入
Backbone 时映射为该 Codex variant。

`ItemStartedNotification` / `ItemCompletedNotification` 在 Backbone 中携带
`AgentDashThreadItem`，同时保留 `thread_id`、`turn_id` 与毫秒时间戳。Codex bridge
接入 Codex 原生事件时包装为 `AgentDashThreadItem::Codex`；AgentDash 自有 connector
可直接产出 native item。

## PlatformEvent

Codex 原生协议没有覆盖的平台能力通过 `PlatformEvent` 扩展。Platform event 必须保持结构化 payload，不把业务语义塞入自由文本。

来源执行器提供会话标题时使用 `PlatformEvent::SourceSessionTitleUpdated`，字段为 `executor_session_id`、`title`、`preview`、`source`。应用层负责把该事件投影为统一的 `session_meta_updated`，并按 `user > source > auto` 的标题来源优先级写入 `SessionMeta`。

上下文压缩使用 Codex `ThreadItem::contextCompaction` 作为 lifecycle item。平台自有 runtime 的成功 compact 通过 `PlatformEvent::SessionMetaUpdate(key = "context_compacted")` 提供 summary、`compacted_until_ref` 和 `first_kept_ref`，这些字段构成 AgentDash-owned projection commit 的可信来源；失败 compact 通过 `context_compaction_failed` platform payload 提供结构化 diagnostic，并同时发送标准 `Error` 事件。外部 executor 的 compact marker 映射为 `executor_context_compacted`，它表达外部 executor 发生过压缩，但没有 replacement provenance，因此语义上属于遥测与审计事件。

前端模型上下文面板的 refresh key 来自 `turn_completed`、内部 platform `context_compacted` 和 `ContextFrame(kind="compaction_summary")`。`executor_context_compacted` 只影响时间线/状态展示语义，因为内部 projection store 没有发生 commit。

### Scenario: Runtime Delivery And PTY Terminal Boundaries

#### 1. Scope / Trigger

Relay disconnect、remote runtime terminal 和浏览器交互式终端会同时影响“执行交付状态”和“终端资源状态”。这两个状态必须使用不同 wire discriminant 与 Backbone payload，原因是 RuntimeSession terminal 驱动 AgentRun delivery / control effects，而 PTY terminal state 只更新 terminal resource projection。

#### 2. Signatures

Relay runtime delivery terminal event:

```json
{
  "type": "event.runtime_session_state_changed",
  "payload": {
    "runtime_session_id": "runtime-session-1",
    "turn_id": "turn-1",
    "state": "completed | failed | cancelled | started",
    "message": "optional terminal detail"
  }
}
```

Relay PTY terminal resource state event:

```json
{
  "type": "event.pty_terminal.state_changed",
  "payload": {
    "terminal_id": "terminal-1",
    "state": "running | exited | lost | killed",
    "exit_code": null,
    "message": "optional resource detail"
  }
}
```

Backbone PTY terminal projection event:

```rust
PlatformEvent::PtyTerminalStateChanged {
    terminal_id,
    state,
    exit_code,
    message,
}
```

#### 3. Contracts

- `event.runtime_session_state_changed.payload.runtime_session_id` is the delivery RuntimeSession id. Terminal states other than `started` are routed to `RelaySessionEvent::Terminal` and then into RuntimeSession terminal processing.
- `event.pty_terminal.state_changed.payload.terminal_id` is a terminal resource id. It updates `AgentRunTerminalRegistry` and injects `PlatformEvent::PtyTerminalStateChanged` into the relevant session stream when an active session can be resolved.
- Frontend terminal projection consumes `PlatformEvent.kind === "pty_terminal_state_changed"` for terminal state and `terminal_output` for terminal output. The session feed reducer filters both resource projection events out of chat entries.
- Backend disconnect may emit both runtime delivery terminal and PTY terminal lost facts; the shared value `lost` is interpreted only after checking the event discriminant.

#### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Runtime delivery event state is `started` | Relay records no terminal transition. |
| Runtime delivery event state is `completed` / `failed` / `cancelled` | Relay feeds `RelaySessionEvent::Terminal` for `runtime_session_id`. |
| PTY terminal event state is `lost` | Terminal registry marks the terminal lost and emits `pty_terminal_state_changed`. |
| Frontend receives `pty_terminal_state_changed` with an unknown state | Dispatcher reports a terminal-state validation error and does not update terminal state. |
| Same stream identity and event sequence is projected twice | Terminal store keeps the first projection and ignores the duplicate. |

#### 5. Good/Base/Bad Cases

- Good: backend disconnect produces a delivery runtime lost fact and one or more PTY terminal lost facts; AgentRun delivery terminal and terminal tab resource state update independently.
- Base: a PTY terminal exits without a runtime terminal event; only terminal registry and terminal tab state change.
- Base: a runtime delivery fails without an interactive terminal; RuntimeSession terminal processing still runs through the runtime terminal boundary.

#### 6. Tests Required

- Relay protocol tests assert `event.runtime_session_state_changed` serializes `runtime_session_id` and `event.pty_terminal.state_changed` serializes `terminal_id`.
- `cargo check -p agentdash-api` covers relay handler routing and Backbone `PlatformEvent` variants.
- Frontend session model tests assert `pty_terminal_state_changed` updates terminal store, invalid states are rejected, duplicate projections are idempotent, and terminal resource events do not become chat feed entries.

#### 7. Canonical Mapping

| Fact | Canonical event / payload | Projection owner |
| --- | --- | --- |
| Delivery RuntimeSession terminal | `event.runtime_session_state_changed` / `runtime_session_id` | RuntimeSession terminal processor -> AgentRun control plane |
| PTY terminal resource lifecycle | `event.pty_terminal.state_changed` / `terminal_id` | Terminal registry + frontend terminal store |

Provider retry/reconnect 与失败轮次恢复使用一等 `PlatformEvent` variants，而不是
`SessionMetaUpdate` 自由 key：

```rust
PlatformEvent::ProviderAttemptStatus(ProviderAttemptStatus)
PlatformEvent::SessionRewound(SessionRewound)
```

Agent run 运行失败使用结构化 `AgentEvent::RunError` 进入 Backbone：

```rust
BackboneEvent::Error(codex::ErrorNotification)
```

`RunError` 的稳定 catch 字段是 `kind`、`code`、`http_status`、`retryable` 与 `aborted`。
Provider 失败只是 `kind=provider` 的 run error；Hook 阻止、runtime delegate 失败、工具运行
失败也应收束到同一错误事实模型，再由 session/前端消费统一的 `ErrorNotification`。

`ProviderAttemptStatus` 字段：

- `turn_id: String`
- `phase: connecting | connected_waiting_first_delta | streaming | retry_scheduled | retrying | failed | succeeded`
- `attempt: u32`
- `max_attempts: u32`
- `will_retry: bool`
- `delay_ms?: u64`
- `reason_code?: String`
- `message?: String`
- `provider?: String`
- `model?: String`

`SessionRewound` 字段：

- `discarded_turn_id: String`
- `discarded_entry_index?: u32`
- `stable_event_seq: u64`
- `stable_turn_id?: String`
- `reason: provider_retry | provider_failure | runtime_failure`
- `replacement_turn_id?: String`
- `message?: String`

消费规则：

- `ProviderAttemptStatus` 是运行状态，不是 assistant message。前端可以渲染 Thinking /
  Reconnecting / retry exhausted，但不能把该文案写入模型上下文。
- `ErrorNotification` 是错误事实展示事件，不是 assistant message。前端按 `TurnError` 渲染
  错误块；运行终态由 AgentRun/RuntimeSession terminal 决定，不从 assistant transcript 推断。
- `ErrorNotification { will_retry: true }` 是 Codex-style intermediate state；它不是 terminal
  failed，也不更新 turn summary。attempt/max/delay/provider 等细节来自
  `ProviderAttemptStatus`。
- `SessionRewound` 是 append-only agent-context rewind marker。事件流不物理删除尾部事件；
  前端 reducer 不能按该事件裁剪 timeline/rawEvents；model context projection 只按
  `discarded_turn_id + discarded_entry_index` 排除失败 AgentLoop 子轮次中的 agent 产物。
- `stable_event_seq` 只保留为稳定边界诊断信息，不表达前端或上下文应裁到该事件序号。
- 新增或修改 `PlatformEvent` 一等 variant 后必须重新生成 TypeScript binding：

```powershell
cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts
```

## TypeScript Binding

生成命令：

```powershell
cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts
```

输出：`packages/app-web/src/generated/backbone-protocol.ts`

前端消费入口：`packages/app-web/src/features/session/model/types.ts`

## Persisted Session Event

定义在 `agentdash-application/src/session/persistence.rs`。

`PersistedSessionEvent.notification` 字段即 `BackboneEnvelope`。`session_events` 持久化只保存 event sequence、时间戳与 `notification_json`；`session_update_type`、`turn_id`、`entry_index`、`tool_call_id` 是 repository 从 envelope 派生出的传输/内存视图字段。领域判断必须回到 `notification` 中的 typed event / trace 解析，派生字段只服务分页响应、NDJSON 和展示定位，不能落成独立事实源。

AgentRun journal、Lifecycle journey 和前端 replay 都必须保留 `PersistedSessionEvent.notification.event` 的 typed `BackboneEvent`。重放、fork prefix、VFS journey 或 NDJSON stream 只允许改写外层坐标字段（例如 product-facing `session_id` 与 AgentRun journal `event_seq`），不能根据 `session_update_type`、tool 名称或 payload 文本重新构造默认 tool / system event。原因是工具卡片、thread item kind、approval、context frame 和 platform event 都以 typed Backbone variant 为唯一渲染与归并入口。

Persisted `BackboneEnvelope` 必须表达模型和前端实际消费的 bounded fact。工具、MCP、shell、
terminal 等 producer 进入 Backbone 前应完成有界化；`SessionEventingService` 在 append/broadcast 前
仍测量 envelope size，并对已知 oversized output 字段写入小型
`session_eventing_append_guard` diagnostic。该 guard 保留 `session_id`、`turn_id`、
`entry_index`、item id、event kind 等索引事实，原因是 Postgres、NDJSON backlog、frontend
`rawEvents` 和后续 projection 都共享这条持久化事实流。

工具大结果的正文读取不属于 Backbone 事件合同。事件中只保留 bounded preview、
`details.truncation` 与 `lifecycle_path`；读取 `lifecycle_path` 必须通过 lifecycle VFS + `fs_read`
的受控路径完成，读取失败返回有界状态，而不是把原始 body 写回 `SessionEvent`。

PiAgent 工具结果的 ThreadItem id 与 `lifecycle_path` item id 必须同源，形状为
`{turn_alias}:{body_alias}`，例如 `turn_001:tool_001` 或 `turn_001:cmd_001`。`lifecycle_path`
使用同一坐标的分段地址：
`lifecycle://session/tool-results/{turn_alias}/{body_alias}/result.txt`。`entry_index` 可以继续作为
envelope trace / ordering 字段存在，但不能参与 tool result body ref，原因是 producer 在进入模型
上下文前需要生成与 Backbone ThreadItem、`SessionToolResultCache` 和 lifecycle VFS 一致的可读地址。
raw `turn_id`、raw `tool_call_id` 和 provider trace 留在结构化 metadata / trace 中，前端和模型主上下文默认消费 readable address。
这些 readable address 的分配事实源是 session runtime identity allocator；AgentLoop、stream mapper
和 cache writer 只消费同一个 allocator/provider 返回的 typed ref，原因是 Backbone projection、
cache key 与 lifecycle VFS 路径必须共享一个 session scoped item identity。

## AgentRun Journal Stream

Product workspace stream:

```text
GET /agent-runs/{run_id}/agents/{agent_id}/journal/stream/ndjson
```

Runtime trace detail:

```text
GET /runtime-traces/{runtime_session_id}
```

The product stream resolves the current delivery RuntimeSession from AgentRun refs, then projects durable Backbone events through AgentRun journal sequence. Runtime trace detail is a read-only inspection view and must pass the same Project `Use` permission through `RuntimeSessionExecutionAnchor`.

Product AgentRun stream 使用 AgentRun journal cursor，而不是 raw RuntimeSession cursor。Fork 后的父级可见事件、子 session fork marker 与子 session 后续事件在后端合并为一个单调 AgentRun journal sequence；前端只消费 NDJSON envelope，不根据 fork lineage、runtime session id 或 event seq 做 prefix/replay 特例。

`session_branch_forked` 是 child RuntimeSession 在 fork initial projection commit 中持久化的 `PlatformEvent::SessionMetaUpdate`。AgentRun journal 读取这条 child event 并映射到 AgentRun journal sequence；journal 本身不再合成第二条 fork marker。其 payload 至少包含 `child_session_id`、`parent_session_id`、`fork_point_event_seq`、`relation_kind`，可包含 compaction/projection provenance 字段。

每行 JSON：

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

连接确认：

```json
{"type":"connected","last_event_id":42}
```

心跳：

```json
{"type":"heartbeat","timestamp":0}
```

## Connector Output Contract

所有 connector 必须产出或转换为 `BackboneEnvelope`：

| Connector | 产出方式 |
| --- | --- |
| `pi_agent` | `stream_mapper.rs` 将 `AgentEvent` 映射为 `BackboneEvent` |
| `codex_bridge` | 解析 `codex-app-server-protocol` 事件，映射为 `BackboneEvent` |
| `vibe_kanban` | adapter 将外部 session notification 转为 Backbone |
| relay | 云端接收远端事件后转入 Backbone 主链路 |

## Frontend Consumption

```text
BackboneEnvelope (NDJSON)
  -> streamTransport.ts
  -> useSessionStream.ts
  -> useSessionFeed.ts
  -> SessionEntry.tsx / SessionChatView.tsx
```

前端直接消费 `BackboneEnvelope` / `BackboneEvent` 类型，不在主路径经过外部 SDK 解析。

### Tool Card Rendering

工具调用卡片以 `AgentDashThreadItem` 为唯一输入契约，通过 `ToolCallCardShell` + `toolCardRegistry` 统一渲染：

```text
AgentDashThreadItem
  -> toolCardRegistry.renderToolCallCard(item, ctx) → { kind, title, body, status }
  -> ToolCallCardShell(kind, title, status, children=body)
```

- `ToolCallCardShell`：统一承载 header（badge/title/status/elapsed）、折叠、审批操作、错误展示。
- `toolCardRegistry`：按 `item.type` 一级分发到专用 renderer body；`dynamicToolCall` 内部按 `tool` 名做二级摘要。
- `threadItemKind.ts`：kind 元数据（badge/label/summaryVerb）的单一来源。
- Body 组件位于 `features/session/ui/bodies/`，每个 item type 对应一个 body，未注册的使用 `GenericJsonBody` 默认渲染。
- Codex 已有 item 直接使用 Codex Protocol type；AgentDash 仅在 Codex 不足时通过 `AgentDashNativeThreadItem` 做加法扩展。
- Tool / command body 展示裁切摘要时优先消费 bounded preview、`details.truncation`、shell truncation
  details 或文本中的 `lifecycle_path` marker；完整输出展开需要走 lifecycle VFS 读取面。
