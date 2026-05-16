# Backbone Protocol 跨层契约

> 平台内部事件流的统一传输协议。取代了历史上的 ACP `_meta.agentdash` Warp Layer。

---

## 1. 定位

`BackboneEnvelope` + `BackboneEvent` 是 AgentDash 内部所有 session 事件流的统一类型。
所有 connector（`codex_bridge` / `pi_agent` / `vibe_kanban` / relay）都必须将输出映射到同一套 `BackboneEvent` 变体。

与历史 ACP `_meta` 方案的区别：

| 维度 | 旧方案（ACP `_meta.agentdash`） | 当前（Backbone Protocol） |
|---|---|---|
| 载体 | 标准 ACP `SessionNotification` + 扩展 `_meta` 命名空间 | 自有 `BackboneEnvelope`，一等字段 |
| source/trace | 嵌入 `_meta.agentdash.source/trace` | `BackboneEnvelope.source: SourceInfo` / `trace: TraceInfo` |
| 事件语义 | 依赖 ACP `SessionUpdate` 枚举 | `BackboneEvent` 变体对齐 `codex-app-server-protocol` |
| 平台扩展 | `SessionInfoUpdate._meta.agentdash.event` | `BackboneEvent::Platform(PlatformEvent)` |
| 兼容层 | — | `compat/mod.rs` 提供双向转换，标记为过渡期（P0.4 移除） |

---

## 2. 类型定义

### 2.1 BackboneEnvelope

定义位置：`crates/agentdash-agent-protocol/src/backbone/envelope.rs`

```rust
pub struct BackboneEnvelope {
    pub event: BackboneEvent,
    pub session_id: String,
    pub source: SourceInfo,
    pub trace: TraceInfo,
    pub observed_at: DateTime<Utc>,
}

pub struct SourceInfo {
    pub connector_id: String,
    pub connector_type: String,
    pub executor_id: Option<String>,
}

pub struct TraceInfo {
    pub turn_id: Option<String>,
    pub entry_index: Option<u32>,
}
```

### 2.2 BackboneEvent

定义位置：`crates/agentdash-agent-protocol/src/backbone/event.rs`

```rust
pub enum BackboneEvent {
    // 文本 / 推理流
    AgentMessageDelta(codex::AgentMessageDeltaNotification),
    ReasoningTextDelta(codex::ReasoningTextDeltaNotification),
    ReasoningSummaryDelta(codex::ReasoningSummaryTextDeltaNotification),

    // Item 生命周期
    ItemStarted(codex::ItemStartedNotification),
    ItemCompleted(codex::ItemCompletedNotification),

    // Item 过程增量
    CommandOutputDelta(codex::CommandExecutionOutputDeltaNotification),
    FileChangeDelta(codex::FileChangeOutputDeltaNotification),
    McpToolCallProgress(codex::McpToolCallProgressNotification),

    // Turn 生命周期
    TurnStarted(codex::TurnStartedNotification),
    TurnCompleted(codex::TurnCompletedNotification),
    TurnDiffUpdated(codex::TurnDiffUpdatedNotification),

    // Plan
    TurnPlanUpdated(codex::TurnPlanUpdatedNotification),
    PlanDelta(codex::PlanDeltaNotification),

    // 资源 / 状态
    TokenUsageUpdated(codex::ThreadTokenUsageUpdatedNotification),
    ThreadStatusChanged(codex::ThreadStatusChangedNotification),
    ContextCompacted(codex::ContextCompactedNotification),

    // 审批请求
    ApprovalRequest(ApprovalRequest),

    // 错误
    Error(codex::ErrorNotification),

    // 平台扩展
    Platform(PlatformEvent),
}
```

序列化采用 `#[serde(tag = "type", content = "payload", rename_all = "snake_case")]`。

### 2.3 PlatformEvent

Codex 原生协议没有覆盖的平台能力，通过 `PlatformEvent` 扩展。

### 2.4 TS 类型

自动生成：`cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts`

输出：`packages/app-web/src/generated/backbone-protocol.ts`

前端消费入口：`packages/app-web/src/features/session/model/types.ts`

---

## 3. 传输契约

### 3.1 持久化 Session Event

```rust
pub struct PersistedSessionEvent {
    pub session_id: String,
    pub event_seq: u64,
    pub occurred_at_ms: i64,
    pub committed_at_ms: i64,
    pub session_update_type: String,
    pub turn_id: Option<String>,
    pub entry_index: Option<u32>,
    pub tool_call_id: Option<String>,
    pub notification: BackboneEnvelope,
}
```

`notification` 字段的内容即 `BackboneEnvelope`。`session_update_type`、`turn_id`、`entry_index`、`tool_call_id` 是从 envelope 提取的便利索引字段。

### 3.2 NDJSON 流

`GET /api/acp/sessions/{id}/stream/ndjson`

每行 JSON：

```json
{"type":"event","session_id":"...","event_seq":42,"occurred_at_ms":...,"committed_at_ms":...,"session_update_type":"agent_message_delta","turn_id":"...","entry_index":0,"tool_call_id":null,"notification":{...BackboneEnvelope...}}
```

连接确认：`{"type":"connected","last_event_id":42}`
心跳：`{"type":"heartbeat","timestamp":...}`

### 3.3 SSE 流

`GET /api/acp/sessions/{id}/stream`

`Event.id` = `event_seq`，`data` 为上述 event JSON。

---

## 4. Connector 产出契约

所有 connector 必须直接产出 `BackboneEnvelope`：

| Connector | 产出方式 |
|---|---|
| `pi_agent` | `stream_mapper.rs` 将 `AgentEvent` 映射为 `BackboneEvent`，包裹 `BackboneEnvelope` |
| `codex_bridge` | 解析 `codex-app-server-protocol` 事件，映射为 `BackboneEvent` |
| `vibe_kanban` | 通过 compat `session_notification_to_envelope()` 从 ACP `SessionNotification` 转换（过渡） |
| relay | 远端发送 ACP `SessionNotification`，云端通过 compat `session_notification_to_envelope()` 转入 |

---

## 5. 兼容层（过渡期）

`crates/agentdash-agent-protocol/src/compat/mod.rs` 提供双向转换：

- `envelope_to_session_notification()`：Backbone → ACP（用于尚未迁移的消费端）
- `session_notification_to_envelope()`：ACP → Backbone（用于 relay 接收远端 ACP 格式）

此兼容层标记为 **P0.4 完成后移除**。

---

## 6. 前端消费链路

```
BackboneEnvelope (NDJSON)
  → streamTransport.ts（fetch + ReadableStream）
  → useSessionStream.ts（流管理 hook）
  → useSessionFeed.ts（事件聚合为 UI entries）
  → SessionEntry.tsx / SessionChatView.tsx 等渲染组件
```

前端直接消费 `BackboneEnvelope` / `BackboneEvent` 类型（来自 `generated/backbone-protocol.ts`），不经过 ACP SDK 解析。

---

*创建：2026-05-16 — 取代历史 acp-meta-warp.md，反映 Backbone Protocol 当前实现*
