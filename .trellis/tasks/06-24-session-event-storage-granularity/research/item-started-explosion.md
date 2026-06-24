# Research: item_started 爆量根因

- Query: 为什么 `session_events` 中 `item_started` 事件数量异常多。
- Scope: internal
- Date: 2026-06-24

## 结论

`ItemStarted` 在当前协议里被**复用为"item 进度/更新"通道**（协议没有 `ItemUpdated`/`ItemProgress` 变体，只有 `ItemStarted` 与 `ItemCompleted`，见 `crates/agentdash-agent-protocol/src/backbone/event.rs:19-62`）。`pi_agent::stream_mapper` 在多条流式上游里对同一 `item_id` **反复发射 `ItemStarted(status=in_progress)`**，每次携带逐渐完整的 args / content_items，且每条都被 `append_event` durable 持久化。这既是 item_started 爆量的直接原因，也是 `session_events` 写吞吐 / 存储压力的一部分。

## 放大点（stream_mapper.rs）

1. **`AssistantStreamEvent::ToolCallDelta`（`stream_mapper.rs:814-870`）**
   模型流式输出 tool-call arguments 期间，每条 delta 只要 `parse_tool_call_args_from_draft` / `apply_patch_preview_args_from_draft` 解析出 args，就发一条新的 `ItemStarted(in_progress)`。一个 tool call 的参数跨 N 条 delta 流出 → 最多 N 条 `ItemStarted`（同一 item_id）。`fs_apply_patch` 流式预览最严重：每个 patch draft 增量都会产出带"逐渐变长 patch 预览"的 ItemStarted（见 `06-23-apply-patch-streaming-preview` 任务，这是有意设计）。

2. **`AgentEvent::ToolExecutionUpdate`（`stream_mapper.rs:1198-1253`）**
   非 shell 工具执行期每次 `partial_result` 更新（`on_update` 回调）都重发 `ItemStarted(in_progress, content_items=partial output)`。工具执行中间态输出越多 → ItemStarted 越多。

3. 单次 `ItemStarted`（语义上"开始"）的正常发射点：`ToolCallStart`（仅 `created` 时）、`MessageEnd` 补发新 tool_call（仅 `created` 时）、`ToolExecutionStart`、`ContextCompactionStarted`。这些是合理的一次性发射，不是爆量来源。

## 与重放的关系

`continuation.rs` raw projection 对 `ItemStarted`/`ItemCompleted` 都只走 `extract_tool_call_from_thread_item` 提取 tool call + terminal result（`continuation.rs:322-377`）。
- 首条 `ItemStarted` 已建立 tool call 存在性；
- 中间多条 `ItemStarted` 只是 args/preview 精化，对重放**冗余**；
- 终态以 `ItemCompleted`（或 terminal ItemStarted）给出最终 args/result。

即：中间 in-progress `ItemStarted` 仅服务 live UI 渐进展示，对 durable 重放是冗余的——与 text delta 同属"进度态事件"。

## 设计含义

不应"取消 ItemStarted"（会丢 live UI 渐进展示与首条建立语义），而应把 in-progress 刷新归入与 delta 相同的 **broadcast-only / 短保留** 策略：实时广播给 UI，但不进 `session_events`（或只保留首条 started + 终态 completed）。前端 reducer 已按 `item_id` upsert，去掉中间持久化不影响最终卡片重建。

## 相关文件

- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:814`, `:1198` — 两个放大点。
- `crates/agentdash-agent-protocol/src/backbone/event.rs:19` — 缺 ItemUpdated 变体。
- `crates/agentdash-application/src/session/continuation.rs:322` — 重放只取 tool call / terminal。
- `.trellis/tasks/06-23-apply-patch-streaming-preview/design.md` — 重复 ItemStarted 作为更新通道是既有设计。
