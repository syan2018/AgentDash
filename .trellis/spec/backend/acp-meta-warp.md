# Scenario: ACP `_meta.agentdash` Warp Layer（跨层契约）

### 1. Scope / Trigger
- **Trigger**: 会话模块跨层数据流新增“可控的 AgentDash 语义层”，要求**对外仍输出标准 ACP**，但需要前后端一致地解析扩展信息、并支持多连接器来源标注与回放/续接定位。
- **影响面**: Rust 连接器适配 → ACP `SessionNotification` → SSE/NDJSON → 前端渲染/聚合与后续扩展 UI（usage/系统事件等）。

---

### 2. Signatures（API/类型/生成物）
- **SSE**: `GET /api/acp/sessions/{id}/stream`
  - `data`: `SessionNotification` 的 JSON
- **NDJSON**: `GET /api/acp/sessions/{id}/stream/ndjson`
  - `{"type":"notification","id":<u64>,"notification":<SessionNotification>}`
  - `{"type":"connected","last_event_id":<u64>}` / `{"type":"heartbeat",...}`
- **Rust 类型（ACP）**: `agent_client_protocol::{SessionNotification, SessionUpdate, Meta}`
- **TS 类型（ACP）**: `@agentclientprotocol/sdk` 的 `SessionNotification/SessionUpdate`
- **AgentDash meta schema（共享）**:
  - Rust: `crates/agentdash-acp-meta/src/lib.rs`
  - 生成 TS: `frontend/src/generated/agentdash-acp-meta.ts`
  - 生成命令: `cargo run -p agentdash-acp-meta --bin generate_agentdash_acp_meta_ts`

---

### 3. Contracts（请求/响应/字段约束）

#### 3.1 ACP `_meta` 的扩展命名空间
- 所有 AgentDash 扩展数据必须放在 **`_meta.agentdash`** 下（避免与其他实现冲突）。
- 版本字段: `_meta.agentdash.v = 1`

#### 3.2 `AgentDashMetaV1`（v1）
- **source**:
  - `connectorId: string`
  - `connectorType: string`（建议取值：`local_executor` / `remote_acp_backend`）
  - `executorId?: string | null`
  - `variant?: string | null`
- **trace**:
  - `turnId?: string | null`
  - `entryIndex?: number | null`
  - `sessionEventId? / parentId?` 预留
- **event**（可选）:
  - `type: string`（例如：`system_message` / `error` / `user_feedback` / `user_answered_questions`）
  - `message?: string | null`
  - `data?: any | null`（后续可结构化）

#### 3.3 写入位置（ACP 标准字段，不改 SessionUpdate 枚举）
- `*_message_chunk` / `agent_thought_chunk`: 写入 `ContentChunk._meta.agentdash`
- `tool_call` / `tool_call_update`: 写入 `ToolCall._meta.agentdash` / `ToolCallUpdate._meta.agentdash`
- `plan`: 写入 `Plan._meta.agentdash`
- `usage_update`（unstable）: 写入 `UsageUpdate._meta.agentdash`
- **非 ACP 等价物（来自 vibe-kanban）**:
  - 使用 `SessionUpdate::SessionInfoUpdate(SessionInfoUpdate)`（unstable），并把事件写入 `SessionInfoUpdate._meta.agentdash.event`
  - 禁止把这些内容伪装成 `AgentMessageChunk`/`UserMessageChunk`

---

### 4. Validation & Error Matrix
- **meta 缺失**: 前端解析 `parseAgentDashMeta` 返回 `null`，渲染逻辑不应崩溃。
- **版本不匹配**: `_meta.agentdash.v != 1` → 视为未知版本，返回 `null`（后续版本升级需显式适配）。
- **字段不完整**: `source`/`trace`/`event` 任意缺失都允许；但 `v` 必须为 1。
- **unstable 未启用**:
  - Rust 若未启用 `agent-client-protocol` 的 `unstable`，将无法使用 `UsageUpdate/SessionInfoUpdate`（编译期错误）。
  - TS SDK 若版本不含 `usage_update/session_info_update`，前端应至少对未知 `sessionUpdate` `return null`（保持兼容）。

---

### 5. Good / Base / Bad Cases
- **Good**: `agent_message_chunk` 的 `content._meta.agentdash` 带 `source+trace`，UI 正常渲染文本；后续可用 meta 做聚合/续接。
- **Base**: 没有 `_meta`（例如历史数据/第三方 ACP 客户端）→ UI 仍正常显示基础消息。
- **Bad**: 把系统/错误文本拼进 `agent_message_chunk`（例如 `[系统]...`）→ UI 会把它当成普通对话文本渲染，破坏体验；应改为 `session_info_update` + `_meta.agentdash.event`。

---

### 6. Tests Required（断言点）
- **Rust**（`crates/agentdash-executor/src/adapters/normalized_to_acp.rs`）:
  - `AssistantMessage` → `AgentMessageChunk` 且 `chunk.meta` 存在且可解析 `agentdash`（v=1）
  - `SystemMessage` → `SessionInfoUpdate` 且 meta.event.type = `system_message`
  - `TokenUsageInfo` → `UsageUpdate` 且 `update.meta` 可解析
- **Frontend**（`frontend/src/features/acp-session/model/agentdashMeta.test.ts`）:
  - `parseAgentDashMeta` 版本判断
  - `extractAgentDashMetaFromUpdate` 能从 `update._meta` 或 `update.content._meta` 提取

---

### 7. Wrong vs Correct

#### Wrong（把扩展信息塞进文本）
```json
{
  "sessionUpdate": "agent_message_chunk",
  "content": { "type": "text", "text": "[系统] hook_started ..." }
}
```

#### Correct（用 ACP 标准变体 + `_meta.agentdash`）
```json
{
  "sessionUpdate": "session_info_update",
  "_meta": {
    "agentdash": {
      "v": 1,
      "event": { "type": "system_message", "message": "hook_started" },
      "source": { "connectorId": "vibe_kanban", "connectorType": "local_executor" },
      "trace": { "turnId": "t1700000000000", "entryIndex": 12 }
    }
  }
}
```

