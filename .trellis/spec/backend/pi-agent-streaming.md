# Pi Agent 流式合并协议

> Pi Agent 在 streaming 模式下的 chunk 发送与消费契约。
> 从 `execution-hook-runtime.md` 拆分，独立维护。

---

## Scenario: chunk 发送语义与完成边界

### 协议事实（ACP）

1. `session/update` 的 `*_message_chunk`（`agent_message_chunk` / `agent_thought_chunk` / `user_message_chunk`）在 ACP stable 中只表示“片段”，不包含“这是最后一个 chunk”的原生字段。
2. 一次 turn 的完成信号来自 `session/prompt` 响应中的 `stopReason`（如 `end_turn` / `cancelled`），而不是某个 chunk。
3. ACP unstable 提供 `ContentChunk.messageId`，用于标识“哪些 chunk 属于同一条消息”。

### 发信层契约（Pi Agent）

`crates/agentdash-executor/src/connectors/pi_agent.rs::convert_event_to_notifications` 必须遵循：

1. **TextDelta / ThinkingDelta**
   - 按增量发 chunk；
   - 为同一 `(turn_id, entry_index, chunk_kind)` 复用同一个 `messageId`；
   - 记录已发送文本（`chunk_emit_states`）。

2. **MessageEnd（拆分逻辑）**
   - 若该消息此前已发过 delta：
     - `final_text` 以已发送文本为前缀：只发送“尾差量”（suffix）；
     - `final_text == emitted_text`：不再发送 chunk（避免重复）；
     - 二者不兼容：记录 warning，不再发送兜底快照（保持单路径约束）。
   - 若此前未发过 delta：发送完整文本 chunk（首发即全量，但不标记为 snapshot）。

3. **entry_index 递增**
   - 保持原契约：本条 assistant 消息处理完成后再递增 `entry_index`。

### 消费层契约（前端）

前端 `useAcpStream` 合并优先级必须是：

1. `messageId` 命中（优先，协议层锚点）
2. `turn_id + entry_index + sessionUpdate`（回退锚点）
3. 最后才走相邻增量合并

### 为什么要这样拆

- 避免“delta + MessageEnd 全量快照”双发导致重复内容；
- 将“消息边界识别”前移到发信层（messageId + sender state）；
- 保持 turn 结束语义遵循 ACP（`stopReason`）。

### 关键文件

- `crates/agentdash-executor/src/connectors/pi_agent.rs` — 发信层拆分与 messageId
- `frontend/src/features/acp-session/model/useAcpStream.ts` — chunk 合并消费策略
