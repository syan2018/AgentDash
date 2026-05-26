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
    TurnPlanUpdated(codex::TurnPlanUpdatedNotification),
    PlanDelta(codex::PlanDeltaNotification),
    TokenUsageUpdated(codex::ThreadTokenUsageUpdatedNotification),
    ThreadStatusChanged(codex::ThreadStatusChangedNotification),
    ExecutorContextCompacted(codex::ContextCompactedNotification),
    ApprovalRequest(ApprovalRequest),
    Error(codex::ErrorNotification),
    Platform(PlatformEvent),
}
```

序列化采用 `#[serde(tag = "type", content = "payload", rename_all = "snake_case")]`。

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

## TypeScript Binding

生成命令：

```powershell
cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts
```

输出：`packages/app-web/src/generated/backbone-protocol.ts`

前端消费入口：`packages/app-web/src/features/session/model/types.ts`

## Persisted Session Event

定义在 `agentdash-application/src/session/persistence.rs`。

`PersistedSessionEvent.notification` 字段即 `BackboneEnvelope`。`session_update_type`、`turn_id`、`entry_index`、`tool_call_id` 是从 envelope 提取的便利索引字段。

## NDJSON Session Stream

`GET /api/acp/sessions/{id}/stream/ndjson`

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
- Body 组件位于 `features/session/ui/bodies/`，每个 item type 对应一个 body，未注册的走 `GenericJsonBody` 兜底。
- Codex 已有 item 直接使用 Codex Protocol type；AgentDash 仅在 Codex 不足时通过 `AgentDashNativeThreadItem` 做加法扩展。
