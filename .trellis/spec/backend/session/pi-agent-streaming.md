# Pi Agent 流式映射协议

> Pi Agent 的 `stream_mapper` 将内部 `AgentEvent` 映射为 `BackboneEnvelope`/`BackboneEvent` 的契约。

---

## 发信层契约

`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` 的 `convert_event_to_envelopes` 必须遵循：

### 1. TextDelta / ThinkingDelta

- 按增量发 `BackboneEvent::AgentMessageDelta` 或 `ReasoningTextDelta`；
- 为同一 `(turn_id, entry_index, chunk_kind)` 复用同一个合成 `item_id`（格式 `{turn_id}:{entry_index}:{suffix}`）；
- 记录已发送文本（`ChunkEmitState.emitted_text`）。

### 2. MessageEnd（拆分逻辑）

- 若该消息此前已发过 delta：
  - `final_text` 以已发送文本为前缀：只发送"尾差量"（suffix）；
  - `final_text == emitted_text`：不再发送 chunk（避免重复）；
  - 二者不兼容：记录 warning，不再发送兜底快照（保持单路径约束）。
- 若此前未发过 delta：发送完整文本 chunk（首发即全量）。

### 3. ToolCall 映射

- `ToolCall` 起始映射为 `BackboneEvent::ItemStarted`，item 类型为
  `AgentDashThreadItem`；
- `AssistantStreamEvent::ToolCallDelta` 表示工具输入生成阶段的更新：
  - 对非 `fs_apply_patch` 工具，只有当 `draft` 是完整 JSON 时才更新该工具 item 的
    `arguments`，并以同一 `item_id` 发送 `ItemStarted(..., status=in_progress)`；
  - 对 `fs_apply_patch`，可以从未闭合 JSON `draft` 中提取已完成转义的 `patch`
    字符串，解析出 `FileChangeSpec` 后用同一 `item_id` 发送
    `ItemStarted(fileChange, status=in_progress)`；
  - 无法解析出可信输入时只更新 `ToolCallEmitState`，不发送 UI 事件。
- `AgentEvent::ToolExecutionUpdate` 表示工具执行阶段的输出/进度更新，不能作为工具输入
  生成阶段的事实源。shell 输出继续映射为 `CommandOutputDelta`，普通工具输出继续更新
  同一 `dynamicToolCall.contentItems`。
- `ToolCallResult` 完成映射为 `BackboneEvent::ItemCompleted`；
- `ToolCallEmitState` 追踪每个 `tool_call_id` 的 `entry_index` 和元数据。
- PiAgent 进入 `ToolExecutionEnd`、`ToolExecutionUpdate` 和 `AgentMessage::ToolResult`
  前必须先对 `AgentToolResult` 做有界化；`stream_mapper` 只消费已经有界的
  `AgentToolResult.content`，不能在映射阶段重新读取 `lifecycle_path` 或恢复原始 body。
- 工具名称到 item 的映射：
  - `shell_exec` -> Codex `ThreadItem::CommandExecution`
  - `fs_apply_patch` -> Codex `ThreadItem::FileChange`
  - `fs_read` -> `AgentDashNativeThreadItem::FsRead`
  - `fs_grep` -> `AgentDashNativeThreadItem::FsGrep`
  - `fs_glob` -> `AgentDashNativeThreadItem::FsGlob`
  - 其他工具 -> Codex `ThreadItem::DynamicToolCall`

### 4. Turn 生命周期

- Turn 开始产出 `BackboneEvent::TurnStarted`；
- Turn 结束产出 `BackboneEvent::TurnCompleted`。

### 5. Token usage 更新

- `AgentEvent::MessageEnd` 携带 provider usage 时，映射为 `BackboneEvent::TokenUsageUpdated`。
- `NormalizedContextUsage.provider_context_tokens/current_context_tokens` 使用 provider 可见输入压力：`input + cache_read_input + cache_creation_input`。
- `model_context_window/effective_context_window` 来自本次执行解析出的 provider model profile，供前端比例显示与压缩统计使用。

### 6. 上下文压缩 lifecycle

- `AgentEvent::ContextCompactionStarted` 映射为 `BackboneEvent::ItemStarted`，item 为 `ThreadItem::ContextCompaction`。
- `AgentEvent::ContextCompacted` 先映射为 `PlatformEvent::SessionMetaUpdate(key="context_compacted")`，再映射为 `BackboneEvent::ItemCompleted`。应用层使用 `context_compacted` metadata 提交 checkpoint / projection，再让 completed marker 进入普通事件流。
- `AgentEvent::ContextCompactionFailed` 映射为 `PlatformEvent::SessionMetaUpdate(key="context_compaction_failed")` 与 `BackboneEvent::Error`。结构化 diagnostic 服务审计和熔断；Error 服务现有错误消费路径。

### 7. entry_index 递增

- 保持原契约：本条 assistant 消息处理完成后再递增 `entry_index`。

---

## Scenario: PiAgent Provider Retry And Error Classification

### 1. Scope / Trigger

- Trigger: PiAgent provider request/stream errors cross the bridge, agent loop, Backbone, session
  event store and frontend feed. The retry boundary must be executable and testable because a
  transient upstream failure must not become assistant text or pollute the next provider request.
- Scope: `BridgeError::provider(...)`, PiAgent bridge HTTP/SSE helpers, agent loop provider attempts,
  `AgentEvent::ProviderAttemptStatus`, stream mapper platform events and failed-turn recovery.

### 2. Signatures

```rust
pub enum BridgeError {
    Provider {
        message: String,
        classification: ProviderErrorClassification,
    },
    // existing variants...
}

pub struct ProviderErrorClassification {
    pub kind: ProviderErrorKind, // Retryable | Fatal | Aborted
    pub http_status: Option<u16>,
    pub provider_code: Option<String>,
    pub retry_after_ms: Option<u64>,
}

pub enum ProviderAttemptPhase {
    Connecting,
    ConnectedWaitingFirstDelta,
    Streaming,
    RetryScheduled,
    Retrying,
    Failed,
    Succeeded,
}
```

PiAgent bridge helpers live in `crates/agentdash-executor/src/connectors/pi_agent/bridges/mod.rs`:

```rust
check_http_response(response, api_label)
provider_transport_error(context, reqwest_error)
provider_stream_read_error(context, reqwest_error)
provider_event_error(message, raw_provider_body)
provider_fatal_error(message, code)
```

### 3. Contracts

- Provider bridges must preserve structured classification facts instead of returning only
  `BridgeError::CompletionFailed` for HTTP/transport/provider failures.
- HTTP status classification:
  - `429`, `408`, and `5xx` are retryable unless provider body/code indicates usage limit, quota,
    auth, context window or invalid request.
  - `400`, `401`, `403`, `404`, and `422` are fatal unless a provider-specific implementation has
    a stricter typed reason to do otherwise.
  - `Retry-After`, `x-ratelimit-reset-after`, and `x-ratelimit-reset` populate `retry_after_ms`.
- OpenAI Responses, OpenAI Chat Completions, Anthropic and OpenAI Codex Responses must convert
  request transport errors and response stream read errors to retryable provider errors.
- Empty 2xx streams before any visible text, reasoning or tool output are retryable
  `provider_code="empty_stream"` errors.
- Codex Responses keeps ChatGPT usage/rate-limit friendly errors fatal even when the HTTP status is
  `429`; the friendly message is display text, while classification controls retry behavior.
- Agent loop retry is only allowed before the first visible delta. A visible text/reasoning/tool
  delta makes the attempt non-replayable; subsequent provider failure becomes the terminal failed
  path and recovery marker flow.
- Aborted provider errors are never retried and must produce `StopReason::Aborted`.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| HTTP `429` with `Retry-After: 5` | Retryable provider error with `http_status=429`, `retry_after_ms=5000` |
| HTTP `408` or `5xx` | Retryable provider error with provider code `timeout` or `provider_5xx` |
| HTTP body/code contains auth, quota, usage limit, context window or invalid request | Fatal provider error even when status is `429` |
| Codex API `rate_limit_exceeded` / `usage_limit_*` | Fatal provider error and preserved friendly Codex message |
| reqwest send/stream read failure | Retryable provider error with transport/stream provider code |
| 2xx SSE stream ends before visible output | Retryable `empty_stream` provider error |
| Retryable error before first visible delta | Retry attempt; no assistant error message or polluted provider request context |
| Retryable error after visible delta | Do not retry; terminal failed/recovery path |
| Aborted error | Do not retry; assistant stop reason is `Aborted` |
| Fatal error | Do not retry; assistant stop reason is `Error` |

### 5. Good/Base/Bad Cases

- Good: First attempt receives `503` before any delta, emits provider retry status, sleeps using
  provider/backoff delay, retries with the same clean user context, and succeeds.
- Good: Three pre-delta `503` attempts exhaust retry; only the final failure assistant is present in
  runtime messages, and earlier transient errors were never sent back to the provider.
- Good: A ChatGPT Codex usage-limit response remains fatal, so the agent does not loop on a user
  quota condition.
- Base: JSON parse failure in provider payload can remain a normal completion failure unless the
  bridge can classify it as a provider HTTP/transport/runtime condition.
- Bad: Replaying a request after an assistant text/reasoning/tool delta has already been emitted.
- Bad: Encoding retryability only in a localized text message and forcing agent/session/frontend
  code to parse that text.

### 6. Tests Required

- Agent tests assert pre-delta retry success, pre-delta retry exhaustion, post-delta no retry,
  aborted no retry and fatal no retry.
- Agent tests assert pre-delta transient errors are not added to provider request snapshots or
  assistant context before the final failure.
- Bridge tests assert HTTP `429`/`408`/`5xx`, fatal body codes, Codex usage/rate-limit fatal behavior,
  and rate-limit headers populate `ProviderErrorClassification`.
- Executor tests assert `AgentEvent::ProviderAttemptStatus` maps to
  `PlatformEvent::ProviderAttemptStatus`.
- Session/application tests assert failed turns append `SessionRewound` and next projected transcript
  excludes the failed turn.

### 7. Wrong vs Correct

#### Wrong

```text
provider 503 before first token -> Assistant("HTTP 503") -> next provider request includes "HTTP 503"
```

#### Correct

```text
provider 503 before first token -> ProviderAttemptStatus(retry_scheduled)
  -> clean retry request -> success or final terminal failure
```

#### Wrong

```text
assistant delta emitted -> provider stream error -> retry same prompt
```

#### Correct

```text
assistant delta emitted -> provider stream error -> failed turn -> SessionRewound stable boundary
```

---

## Scenario: PiAgent 工具与终端大输出有界化

### 1. Scope / Trigger

- Trigger: PiAgent 工具、MCP、`shell_exec`、relay terminal live output 都会进入模型上下文、
  Backbone、SessionEvent、NDJSON 与前端 feed；这些 producer 必须在事实流入口保持 bounded。
- Scope: `AgentToolResult` final/update、`shell_exec` final/live、`PlatformEvent::TerminalOutput`、
  `SessionEventingService` append guard、`lifecycle_vfs` 回看面。

### 2. Signatures

- `AgentToolResult.details.truncation`:
  `truncated: bool`、`original_bytes: usize`、`inline_bytes: usize`、
  `omitted_bytes: usize`、`policy: string`。
- PiAgent readable runtime address:
  session scoped `ReadableIdRegistry` 为 raw `turn_id`、raw `tool_call_id` 和 raw `terminal_id`
  分配 `turn_001`、`tool_001` / `cmd_001`、`term_001` 这类可见 alias。工具结果
  `item_id = {turn_alias}:{body_alias}`，该 id 必须同时用于 `AgentToolResult` lifecycle ref、
  `SessionToolResultCache` key、Backbone ThreadItem id 和 lifecycle VFS 分段路径。`entry_index`
  只属于 stream mapper 的展示/排序状态，不能放进 tool result lifecycle ref，原因是 producer
  边界需要在进入模型上下文前生成同一个可读地址。
- `AgentToolResult.details.lifecycle_path`:
  `lifecycle://session/tool-results/{turn_alias}/{body_alias}/result.txt`。
- `AgentToolResult.details.readable_ref` 保存可见 alias；`details.raw_trace` 保存 raw
  `turn_id`、raw `tool_call_id` 和 tool name，用于诊断但不进入默认正文。
- lifecycle VFS paths:
  `session/tool-results/{turn_alias}/{body_alias}/metadata.json`、
  `session/tool-results/{turn_alias}/{body_alias}/result.txt`、
  `session/terminal/{terminal_alias}.metadata.json`、
  `session/terminal/{terminal_alias}.log`。
- shell details retain:
  `state`、`exit_code`、`session_id`、`terminal_id`、`next_seq`、
  `truncated`、`omitted_bytes`。

### 3. Contracts

- Producer boundary owns bounding. Final result, partial update and terminal live output must be
  bounded before they become `AgentEvent` / `BackboneEnvelope`.
- PiAgent 每轮 prompt 必须刷新 `ToolResultRefContext(session_id, raw_turn_id, readable_ids, cache_writer)`。
  hot agent 复用时也使用当前 turn 的 context，避免 lifecycle ref 和 cache write 落到上一轮
  raw `turn_id`，同时复用同一 session 的 alias registry。
- Oversized `AgentToolResult` 写入 `SessionToolResultCache` 时，cache key 使用
  `(session_id, readable_item_id)`；bounded preview 与 `details.lifecycle_path` 使用同一个
  readable item id。cache metadata 保留 raw trace。lifecycle provider 必须读取同一个共享 cache 实例。
- `SessionEventingService` append guard is a persistence and stream safety net. It may replace
  known oversized output fields with `session_eventing_append_guard` diagnostics while preserving
  `turn_id`、`entry_index`、item id and event kind.
- `lifecycle_path` is a readable runtime address, not durable truth. Missing or expired bodies return
  bounded status text through lifecycle VFS.
- `fs_read` remains the controlled reader for lifecycle bodies and keeps its existing full-read and
  `offset/limit` behavior.

### 4. Validation & Error Matrix

| Condition | Expected behavior |
| --- | --- |
| Tool result text exceeds inline cap | Write bounded preview, set `details.truncation`, attach `lifecycle_path` |
| Non-text tool content serializes above inline cap | Replace content with bounded text preview and ref metadata |
| PiAgent hot agent starts a new turn | Refresh `ToolResultRefContext`; new lifecycle paths use the session registry's next readable `turn_###` alias |
| Stream mapper maps tool start/update/end | ThreadItem id equals the item id embedded in `details.lifecycle_path` |
| Lifecycle provider reads tool result body | Use shared `SessionToolResultCache` keyed by `(session_id, {turn_alias}:{body_alias})` |
| Cache body missing or expired | lifecycle read returns bounded miss/expired status |
| Terminal live output exceeds event budget | Relay/platform event carries bounded data and truncation status |
| Oversized Backbone envelope reaches append | Known output fields are replaced before store/broadcast |

### 5. Good/Base/Bad Cases

- Good: A large dynamic tool result persists as bounded preview plus `lifecycle_path`; model resume sees
  the same preview and ref text.
- Base: A small tool result remains unchanged and has no truncation metadata.
- Base: A cache-available lifecycle `result.txt` read returns the cached original body through
  lifecycle VFS / `fs_read`, while persisted events still contain only bounded preview.
- Bad: A sentinel embedded in tool/terminal output appears in `session_events.notification_json`,
  NDJSON backlog, projected transcript, or frontend `rawEvents`.
- Bad: ThreadItem id and the id embedded in `lifecycle_path` differ.

### 6. Tests Required

- Agent loop tests assert final/update/immediate/rejected tool result sentinel does not reach events
  or next provider request; oversized final/update paths also assert cache writer receives the
  original body with `(session_id, {turn_alias}:{body_alias})` and raw trace metadata.
- Executor mapping tests assert bounded content remains bounded after `stream_mapper`, and parse
  `lifecycle_path` to prove the embedded item id equals ThreadItem id.
- Application tests assert append/backlog, lifecycle VFS read, projection, continuation and repository
  rehydrate do not re-inline sentinel.
- Local/relay/API tests assert shell and terminal live output are bounded before cloud SessionEvent.
- Frontend tests assert terminal store caps buffers and tool/command cards display truncation state.

### 7. Wrong vs Correct

#### Wrong

```text
tool body -> AgentToolResult -> BackboneEnvelope -> SessionEvent -> projection/resume
```

#### Correct

```text
tool body -> bounded AgentToolResult + lifecycle_path -> BackboneEnvelope -> SessionEvent
          -> projection/resume uses persisted bounded content only
```

---

## 消费层契约（前端）

前端 `useSessionFeed` 事件聚合优先级：

1. `BackboneEvent` 变体类型匹配（主路径）
2. `trace.turn_id + trace.entry_index`（同轮归并锚点）
3. `item_id` 命中（工具调用关联）

---

## 关键文件

- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` — 事件映射与 ChunkEmitState
- `packages/app-web/src/features/session/model/useSessionStream.ts` — 流管理 hook
- `packages/app-web/src/features/session/model/useSessionFeed.ts` — 事件聚合消费
