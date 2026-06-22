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
- `AgentToolResult.details.lifecycle_path`:
  `lifecycle://session/tool-results/{item_id}/result.txt`。
- lifecycle VFS paths:
  `session/tool-results/{item_id}/metadata.json`、
  `session/tool-results/{item_id}/result.txt`、
  `session/terminal/{terminal_id}.metadata.json`、
  `session/terminal/{terminal_id}.log`。
- shell details retain:
  `state`、`exit_code`、`session_id`、`terminal_id`、`next_seq`、
  `truncated`、`omitted_bytes`。

### 3. Contracts

- Producer boundary owns bounding. Final result, partial update and terminal live output must be
  bounded before they become `AgentEvent` / `BackboneEnvelope`.
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
| Cache body missing or expired | lifecycle read returns bounded miss/expired status |
| Terminal live output exceeds event budget | Relay/platform event carries bounded data and truncation status |
| Oversized Backbone envelope reaches append | Known output fields are replaced before store/broadcast |

### 5. Good/Base/Bad Cases

- Good: A large dynamic tool result persists as bounded preview plus `lifecycle_path`; model resume sees
  the same preview and ref text.
- Base: A small tool result remains unchanged and has no truncation metadata.
- Bad: A sentinel embedded in tool/terminal output appears in `session_events.notification_json`,
  NDJSON backlog, projected transcript, or frontend `rawEvents`.

### 6. Tests Required

- Agent loop tests assert final/update/immediate/rejected tool result sentinel does not reach events
  or next provider request.
- Executor mapping tests assert bounded content remains bounded after `stream_mapper`.
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
