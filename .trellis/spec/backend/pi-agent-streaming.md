# Pi Agent 流式合并协议

> Pi Agent 在 streaming 模式下的 chunk 合并契约。
> 从 `execution-hook-runtime.md` 拆分，独立维护。

---

## Scenario: agent_message_chunk 合并协议（Pi Agent 流模式）

### 背景

Pi Agent 在 streaming 模式下同时产生两类 `agent_message_chunk`：

1. **TextDelta 增量 chunk** — `AgentEvent::MessageUpdate::TextDelta` 触发，每次只含新增文字片段
2. **MessageEnd 全量 chunk** — `AgentEvent::MessageEnd` 触发，含完整的消息文本快照

两类 chunk 共享相同的 `_meta.agentdash.trace.entryIndex`（`entry_index` 在 `MessageEnd` 之后才递增），因此 `(turnId, entryIndex, sessionUpdate)` 三元组可以唯一标识"同一条消息"。

### 前端合并契约

前端 `useAcpStream.applyNotification` 在处理 chunk 时必须按以下顺序：

**Step 1：entryIndex upsert（优先）**

若 incoming chunk 携带 `turnId` + `entryIndex`，先向后遍历 entries 查找同 `(turnId, entryIndex, sessionUpdate)` 的已有 entry：
- **找到** → 用 incoming 文本**直接覆盖**（全量快照覆盖增量累积版本），不拼接，不新建
- **找不到** → 进入 Step 2

**Step 2：相邻合并（次选，正常 delta 场景）**

`(turnId, sessionUpdate)` 相同且相邻的 chunk → `mergeStreamChunk` 累积拼接。

**禁止行为**：不得在 entryIndex upsert 命中时走 `${previous}${incoming}` 拼接，这会导致重复渲染。

### MessageEnd 行为说明

`pi_agent.rs::convert_event_to_notifications` 对 `AgentEvent::MessageEnd` 的处理：
- 正常路径：发出包含完整文本的 `AgentMessageChunk` + 递增 `entry_index`
- 错误路径（`error_message` 非空且无实际 TextDelta content）：同上，发出错误文本 chunk

**MessageEnd 发全量快照是正确且必要的行为**，不应修改。前端负责通过 entryIndex 识别并正确覆盖。

### 关键文件

- `crates/agentdash-executor/src/connectors/pi_agent.rs` — `convert_event_to_notifications`
- `frontend/src/features/acp-session/model/useAcpStream.ts` — `applyNotification` chunk 合并
