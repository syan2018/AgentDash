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

- `ToolCall` 起始映射为 `BackboneEvent::ItemStarted`（`ThreadItem::DynamicToolCall`）；
- `ToolCallResult` 完成映射为 `BackboneEvent::ItemCompleted`；
- `ToolCallEmitState` 追踪每个 `tool_call_id` 的 `entry_index` 和元数据。

### 4. Turn 生命周期

- Turn 开始产出 `BackboneEvent::TurnStarted`；
- Turn 结束产出 `BackboneEvent::TurnCompleted`。

### 5. 上下文压缩 lifecycle

- `AgentEvent::ContextCompactionStarted` 映射为 `BackboneEvent::ItemStarted`，item 为 `ThreadItem::ContextCompaction`。
- `AgentEvent::ContextCompacted` 先映射为 `PlatformEvent::SessionMetaUpdate(key="context_compacted")`，再映射为 `BackboneEvent::ItemCompleted`。应用层使用 `context_compacted` metadata 提交 checkpoint / projection，再让 completed marker 进入普通事件流。
- `AgentEvent::ContextCompactionFailed` 映射为 `PlatformEvent::SessionMetaUpdate(key="context_compaction_failed")` 与 `BackboneEvent::Error`。结构化 diagnostic 服务审计和熔断；Error 服务现有错误消费路径。

### 6. entry_index 递增

- 保持原契约：本条 assistant 消息处理完成后再递增 `entry_index`。

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
