# Research: SessionEvent projection risk

- Query: SessionEvent 持久化、ContextProjector、前端 stream/rawEvents 的大 payload 风险；BackboneEnvelope 到 Postgres、API NDJSON、前端 rawEvents、ContextProjector/continuation 的完整链路；规划大工具返回的事件形状与投影策略。
- Scope: internal
- Date: 2026-06-22

## Findings

### Files Found

- `crates/agentdash-agent-protocol/src/backbone/envelope.rs` - `BackboneEnvelope` 的 wire 形状，包含 `event/session_id/source/trace/observed_at`。
- `crates/agentdash-application/src/session/turn_processor.rs` - turn 内 notification 入口，connector/relay 事件进入持久化链路。
- `crates/agentdash-application/src/session/eventing.rs` - session eventing service，负责 append、projection head 推进、broadcast、compaction commit。
- `crates/agentdash-application/src/session/persistence.rs` - application 层 `SessionEventStore` 适配器，`append_event` 直接转给 persistence。
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` - Postgres `append_event/read_backlog/list_event_page/list_all_events` 实现。
- `crates/agentdash-infrastructure/src/persistence/session_core.rs` - Postgres row 解析、`notification_json` 反序列化、从 envelope 提取索引字段。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - `session_events` 表定义，`notification_json text NOT NULL`。
- `crates/agentdash-contracts/src/runtime/session.rs` - `SessionEventResponse`/`SessionNdjsonEnvelope` 合同，直接暴露 `BackboneEnvelope`。
- `crates/agentdash-api/src/routes/sessions.rs` - `/sessions/{id}/events` 和 `/stream/ndjson` route，原样返回 persisted event。
- `crates/agentdash-application/src/session/context_projector.rs` - `ContextProjector` 从持久化事件或 projection head 构建模型上下文。
- `crates/agentdash-application/src/session/continuation.rs` - 从 `session_events` 重建 transcript 与 continuation context frame。
- `crates/agentdash-application/src/session/launch/planner.rs` - repository rehydrate 时把 projected transcript 转成 `RestoredSessionState.messages`。
- `crates/agentdash-application/src/session/launch/preparation.rs` - continuation context frame 会作为 connector-facing context 注入 turn。
- `packages/app-web/src/features/session/model/streamTransport.ts` - NDJSON fetch parser，校验 envelope 结构后原样交给 hook。
- `packages/app-web/src/features/session/model/useSessionStream.ts` - 先 HTTP hydrate 历史，再连接 NDJSON；`rawEvents` 是前端事实源。
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts` - reducer 把完整 event push 到 `rawEvents`，并派生 entries。
- `packages/app-web/src/features/session/model/useSessionFeed.ts` / `packages/app-web/src/features/session/ui/SessionChatView.tsx` - `rawEvents` 继续驱动 turn segment、projection refresh、system event 与 task tool refresh。

### Data Flow Steps

1. Connector/relay 产出 `BackboneEnvelope`。`BackboneEnvelope` 本身没有 payload size 边界，只包装 `BackboneEvent`、session、source、trace 和 observed time（`crates/agentdash-agent-protocol/src/backbone/envelope.rs:27`）。
2. `SessionTurnProcessor` 收到 `TurnEvent::Notification(Box<BackboneEnvelope>)`，调用 `post_turn_handler.on_event` 后把 `envelope.clone()` 传给 `SessionEventingService::persist_notification`（`crates/agentdash-application/src/session/turn_processor.rs:20`, `crates/agentdash-application/src/session/turn_processor.rs:182`）。
3. `SessionEventingService::persist_notification_inner` 先尝试 compact projection commit，否则直接 `stores.events.append_event(session_id, &envelope)`；普通路径 append 后推进 model projection head，再 broadcast persisted event（`crates/agentdash-application/src/session/eventing.rs:133`, `crates/agentdash-application/src/session/eventing.rs:164`）。
4. `PostgresSessionRepository::append_event` 在事务中递增 `sessions.last_event_seq`，从 envelope 派生 `session_update_type/turn_id/entry_index/tool_call_id`，然后把 `persisted.notification` 通过 `json_string` 序列化成 `notification_json` 插入 `session_events`（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:319`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:347`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:359`）。
5. schema 中 `session_events.notification_json` 是 `text NOT NULL`，没有 CHECK 或长度约束（`crates/agentdash-infrastructure/migrations/0001_init.sql:575`）。
6. 读取历史时，Postgres `read_backlog/list_event_page/list_all_events` 都 SELECT `notification_json`，再由 `persisted_event_from_row` 反序列化为完整 `BackboneEnvelope`（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:423`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:461`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:499`, `crates/agentdash-infrastructure/src/persistence/session_core.rs:62`）。
7. HTTP 历史 API `/sessions/{id}/events` 默认拉 500 条、最大 2000 条，并把每条 `PersistedSessionEvent` 直接 `into()` 成 `SessionEventResponse`（`crates/agentdash-api/src/routes/sessions.rs:418`, `crates/agentdash-api/src/routes/sessions.rs:424`, `crates/agentdash-api/src/routes/sessions.rs:289`）。
8. NDJSON stream 建连时先从 `subscribe_after` 拿 backlog，逐条 `to_ndjson_line(&SessionNdjsonEnvelope::event(event))`；实时 broadcast 也走同一个序列化函数（`crates/agentdash-api/src/routes/sessions.rs:994`, `crates/agentdash-api/src/routes/sessions.rs:1008`, `crates/agentdash-api/src/routes/sessions.rs:1027`, `crates/agentdash-api/src/routes/sessions.rs:1098`）。
9. contract 层 `SessionEventResponse.notification: BackboneEnvelope`，NDJSON `Event` 分支 flatten 同一 `SessionEventResponse`，没有细分 summary/detail payload（`crates/agentdash-contracts/src/runtime/session.rs:12`, `crates/agentdash-contracts/src/runtime/session.rs:63`）。
10. 前端 `streamTransport` 对 `notification` 只检查它像 `BackboneEnvelope`，之后把完整 object 放入 `SessionEventEnvelope`（`packages/app-web/src/features/session/model/streamTransport.ts:58`, `packages/app-web/src/features/session/model/streamTransport.ts:84`）。
11. `useSessionStream` 启动时分页 fetch 历史事件，再连接 NDJSON；hook 注释明确 `rawEvents` 是事实源（`packages/app-web/src/features/session/model/useSessionStream.ts:1`, `packages/app-web/src/features/session/model/useSessionStream.ts:196`, `packages/app-web/src/features/session/model/useSessionStream.ts:215`）。
12. reducer 对每条新事件执行 `rawEvents = [...rawEvents, event]`，同时用完整 event 派生 entries/token usage（`packages/app-web/src/features/session/model/sessionStreamReducer.ts:263`, `packages/app-web/src/features/session/model/sessionStreamReducer.ts:278`）。
13. `ContextProjector` 每次 build model context 都 `list_all_events(session_id)`；无 active projection head 时从所有事件重建 raw transcript，有 head 时仍读取全量事件并投影 suffix（`crates/agentdash-application/src/session/context_projector.rs:29`, `crates/agentdash-application/src/session/context_projector.rs:154`）。
14. continuation 从事件里的 `ItemStarted/ItemCompleted` 提取工具调用，terminal tool result 会进入 `ProjectedEntry::ToolResult`；dynamic/native tool 的 `content_items`、command 的 `aggregated_output`、MCP 的 `result/error` 都会变成模型可见 tool result content/details（`crates/agentdash-application/src/session/continuation.rs:316`, `crates/agentdash-application/src/session/continuation.rs:478`, `crates/agentdash-application/src/session/continuation.rs:632`）。
15. repository rehydrate 的 executor-state 路径会把 `build_projected_transcript` 的 entries 直接转成 `RestoredSessionState.messages`，下一轮继续执行前交给 connector（`crates/agentdash-application/src/session/launch/planner.rs:177`）。
16. system-context rehydrate/continuation frame 会把 projected transcript 渲染成 markdown，并在 launch preparation 阶段加入 connector context frames（`crates/agentdash-application/src/session/continuation.rs:23`, `crates/agentdash-application/src/session/launch/preparation.rs:268`）。

### Code Patterns

- 当前主链路是 append-only fact log：`SessionEventingService` append event 后推进 projection head 并 broadcast，compact 只影响 projection head/segments，不改写历史事件（`crates/agentdash-application/src/session/eventing.rs:164`）。
- Postgres repository 在 `append_event` 里先 clone 完整 envelope 到 `PersistedSessionEvent.notification`，再把它整体序列化进 `notification_json`（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:348`）。
- `session_update_type`、`turn_id`、`entry_index`、`tool_call_id` 只是索引字段；真实 payload 仍是完整 `notification_json`（`crates/agentdash-infrastructure/src/persistence/session_core.rs:660`）。
- `ContextProjector` 的输入不是 UI entries，而是 durable `session_events`；因此前端裁切不能阻止 resume/context projection 读到大 payload（`crates/agentdash-application/src/session/context_projector.rs:16`）。
- `continuation.rs` 只有 `json_preview` 对无法解析的 raw JSON 做 320 字符 display fallback；如果 tool result 能解码成 `AgentToolResult` 或存在 `content_parts/aggregated_output`，内容会原样进入 `ContentPart::Text`（`crates/agentdash-application/src/session/continuation.rs:509`, `crates/agentdash-application/src/session/continuation.rs:707`, `crates/agentdash-application/src/session/continuation.rs:809`）。
- 前端 `rawEvents` 是完整 event 数组，不是轻量索引；多个 UI effect 会遍历同一个数组判断 turn lifecycle、platform events、task tool events（`packages/app-web/src/features/session/ui/SessionChatView.tsx:351`）。

### Large Payload Risk Points

- Producer/channel risk: `TurnEvent::Notification(Box<BackboneEnvelope>)` 使用 unbounded mpsc，若巨大 envelope 在工具返回前未裁掉，单条消息会占用大量 heap，并在 `post_turn_handler` 与 persist clone 中放大一次（`crates/agentdash-application/src/session/turn_processor.rs:62`, `crates/agentdash-application/src/session/turn_processor.rs:188`）。
- Persistence write risk: `serde_json::to_string`/`json_string` 需要把完整 `BackboneEnvelope` 渲染成一个 String，随后作为 `text` 绑定入 Postgres；大 payload 会造成应用内存峰值、DB write latency、transaction 持锁时间、WAL/TOAST/存储膨胀（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:359`）。
- Persistence read risk: backlog/page/list_all 都会把 `notification_json` 全量 SELECT 回来并反序列化；单个大事件会拖慢历史加载、NDJSON reconnect、ContextProjector、branch/fork/rollback 的 projection 构造（`crates/agentdash-infrastructure/src/persistence/session_core.rs:71`）。
- API/stream risk: `/events` 一页最多 2000 条，NDJSON backlog 连接时也补发历史；如果其中有大事件，HTTP response、NDJSON line、browser JSON.parse 都要承受完整 payload（`crates/agentdash-api/src/routes/sessions.rs:424`, `crates/agentdash-api/src/routes/sessions.rs:1010`）。
- Frontend memory/render risk: `rawEvents = [...rawEvents, event]` 在追加时复制数组并保留完整 event object；entries 又保存 `event: bbEvent`，tool item completed 可能同时在 raw event 和 display entry 中保留大 item（`packages/app-web/src/features/session/model/sessionStreamReducer.ts:77`, `packages/app-web/src/features/session/model/sessionStreamReducer.ts:282`）。
- Model context risk: `ContextProjector` 没有 active head 时会从所有事件构建完整 transcript；有 head 时 suffix 仍从 raw events 重建。若大 tool result 进入 suffix，它会进入下一轮模型输入或 continuation frame（`crates/agentdash-application/src/session/context_projector.rs:42`, `crates/agentdash-application/src/session/context_projector.rs:205`）。
- Rehydrate risk: executor-state rehydrate 会把 projected transcript messages 作为 `RestoredSessionState.messages` 交回 connector；大 tool result 会导致 provider request 超上下文或请求体过大（`crates/agentdash-application/src/session/launch/planner.rs:192`）。
- Terminal/tool shape risk: command `aggregated_output`、dynamic/native `content_items`、MCP `result` 都是可能的大 payload 入口；这些字段在 current continuation path 中可成为模型可见内容（`crates/agentdash-application/src/session/continuation.rs:647`, `crates/agentdash-application/src/session/continuation.rs:675`, `crates/agentdash-application/src/session/continuation.rs:707`, `crates/agentdash-application/src/session/continuation.rs:750`）。

### Why A Pre-Persistence Guard Is Necessary

- guard 必须在 producer/tool result 返回路径或最迟在 `SessionEventingService::persist_notification_inner` 调用 `append_event` 前发生。原因是 `append_event` 之后，同一 payload 已经进入 Postgres、broadcast backlog、API history、NDJSON、frontend rawEvents、ContextProjector 和 continuation/repository rehydrate。
- 仅靠 ContextProjector 裁切不够：它只能保护模型输入，不能阻止 `notification_json` 写入数据库、历史 API 读取、前端 raw event 保留，也不能降低 NDJSON replay 的传输成本。
- 仅靠前端 lazy/hydration 不够：大 payload 已经经由 Postgres 和 API 传到浏览器；前端最多避免 UI 渲染爆炸，不能解决 DB/后端恢复和模型上下文污染。
- 仅靠 Postgres schema 限制不够：如果 DB 拒绝写入，会导致 terminal event/turn lifecycle 丢失或 turn 卡住；正确做法是把大 tool result 转成 bounded event，再持久化一个可接受的事实事件。
- 当前 `SessionEvent` 同时承担 audit fact、stream payload 和 projection input。大 payload guard 的目标不是丢事实，而是把事实形状改成：持久化“模型实际看到的 bounded preview + truncation metadata + lifecycle/cache ref”，不持久化大原文。

### Recommended Event Shape

Session event 中的 tool/terminal completed payload 应保持 bounded，并显式表达原文外置：

```json
{
  "type": "tool_result",
  "tool_call_id": "call-1",
  "tool_name": "fs_read",
  "content": [
    { "type": "text", "text": "bounded preview/head-tail shown to model" }
  ],
  "truncated": true,
  "truncation": {
    "reason": "tool_result_too_large",
    "policy": "head_tail_v1",
    "original_bytes": 18422391,
    "inline_bytes": 32768
  },
  "lifecycle_ref": "lifecycle://sessions/{session_id}/tool-results/{tool_call_id}.txt",
  "cache": {
    "kind": "cloud_cache",
    "ref": "cloud-cache://tool-results/...",
    "expires_at_ms": 1790000000000,
    "digest": "sha256:..."
  }
}
```

Shape requirements:

- `content` / `content_items` / `aggregated_output` / MCP `result` 中的 model-visible text 必须已经是 bounded preview。
- truncation metadata 必须足够让 UI、ContextProjector 和 lifecycle VFS 解释“还有外置原文/可能过期”，但不能要求自动 hydration。
- `lifecycle_ref` 是模型后续按需读取大内容的 surface；读取 ref 仍必须经过 read offset/limit 防御。
- cache miss/expired 是合法结果，返回明确提示，不重新把全量原文写回 `session_events`。
- terminal 事件应以 bounded delta/phase summary/ref 表达，避免 terminal output 通过 `command_output_delta`、`aggregated_output` 或 platform terminal output 绕过同一 guard。

### Projection Strategy

- `ContextProjector` 应只消费 bounded preview 和 structured truncation metadata；它不应自动 follow `lifecycle_ref` 读取原文。
- continuation markdown 可以展示 preview 和 ref/过期说明；不要把 ref 全量内容内联进 continuation context frame。
- 对 completed tool result，projection 中的 `AgentMessage::ToolResult.content` 应等于模型当时实际看到的 preview。`details` 只能保存小型 metadata，不能保存原始大 JSON。
- 对 compaction，summary/projection segment 可总结 preview 与 ref，不应把 source event 中的外置 payload rehydrate 回 segment。
- fork/rollback/branch 使用当前 projection head 与 suffix event 继续重建；suffix event 的工具结果必须已经 bounded，否则 branch 恢复仍会复发大 payload 风险。

### Frontend Lazy/Hydration Assessment

- 前端不应作为第一道防线；后端必须保证 `SessionEventResponse.notification` 是 bounded。
- 在 bounded event shape 落地后，当前 `rawEvents` 可以继续作为事实源，短期不必把所有 session event 改成 lazy hydrate；因为 raw event 不再携带大原文。
- 如果产品需要“展开查看完整工具输出”，应新增按 `lifecycle_ref` 或 cache ref 的显式读取路径，UI 只在用户展开时 lazy load，并按 offset/limit 分页，不把完整原文并回 `rawEvents`。
- 对历史 hydrate，可以考虑未来把 `rawEvents` 拆成 bounded event log + optional detail cache，但这是优化而不是防爆核心。核心验收应先确保 `/events` 和 NDJSON 永远不承载大原文。
- `SessionChatView` 和 `useSessionFeed` 依赖 `rawEvents` 做 turn/system/task refresh，说明如果改成 metadata-only event，必须保留 `event_seq/session_update_type/turn_id/tool_call_id/notification.event.type` 这些轻量事实字段。

### High-Risk Test Points

- Producer guard unit/integration: 构造超大 dynamic tool/MCP/command result，断言进入 `persist_notification` 前的 event 已被替换为 preview + truncation metadata + ref。
- Persistence guard test: 对超大 result append 后，直接查询 `session_events.notification_json` 长度低于策略阈值，且 JSON 中不包含完整原文 sentinel。
- ContextProjector test: 无 compaction head 和有 active projection head/suffix 两种路径都只生成 bounded `AgentMessage::ToolResult.content`，不 follow ref。
- Repository rehydrate test: `PromptLaunchPath::RepositoryRehydrate(ExecutorState)` 下 `RestoredSessionState.messages` 不包含完整大文本，且 tool result 保留 preview/ref 说明。
- NDJSON/backlog test: `session_stream_ndjson` 历史补发包含 bounded event；单行 NDJSON 长度受控，连接不会因序列化大 payload 卡住。
- Frontend reducer test: 超大工具结果事件使用 bounded payload 时 `rawEvents` 与 entries 都不保存原始 sentinel；`item_completed` flush 仍正常。
- Terminal bypass test: `command_output_delta`、`CommandExecution.aggregated_output`、platform terminal output 三条路径都不能把大输出原样写入 event/rawEvents/context。
- Cache miss test: lifecycle ref 过期或不存在时，read 返回稳定 cache miss/expired 提示，且不会写入新的大 session event。
- Compaction interaction test: compact source range 覆盖大-result preview 后，projection segment summary 不包含原始大文本；rollback 到 suffix 后也不恢复原文。
- API page test: `/sessions/{id}/events?limit=2000` 在包含大工具结果的 session 上响应体仍受 bounded policy 控制。

## External References

- No external references consulted. This research is based on repository code, current Trellis specs, and the active task discussion draft.

## Related Specs

- `.trellis/spec/cross-layer/backbone-protocol.md` - BackboneEnvelope 是平台内部持久化、NDJSON 推送和前端消费的事件 envelope；`PersistedSessionEvent.notification` 是 `BackboneEnvelope`。
- `.trellis/spec/backend/session/streaming-protocol.md` - session NDJSON event 的 `notification` 字段是 `BackboneEnvelope`，前端通过 `x-stream-since-id` 续传。
- `.trellis/spec/backend/session/context-compaction-projection.md` - `session_events` 是事实源；ContextProjector 从 projection head、segments 和 suffix events 构建模型上下文。
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession 拥有 turn/tool/event/resume/debug/projection/trace lineage；runtime map、active turn、connector live session 分离。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - Rust contract -> generated TS 是前后端 DTO 事实源，NDJSON envelope 属于 contract。
- `.trellis/spec/frontend/hook-guidelines.md` - `useSessionStream`/NDJSON hook contract 与 `useSessionFeed` 聚合规则。
- `.trellis/spec/frontend/state-management.md` - store/hook 消费 generated DTO，不在前端重新定义协议字段。
- `.trellis/tasks/06-22-tool-large-output-lifecycle-draft/discussion-draft.md` - 本 task 对大工具返回、短期 cache、lifecycle ref、SessionEvent bounded preview 的初步共识。

## Caveats / Not Found

- 未发现当前 Postgres `session_events.notification_json` 有长度限制或 guard。
- 未发现 `SessionEventingService` 或 `PostgresSessionRepository::append_event` 在持久化前对 `BackboneEnvelope` 做 payload 裁切。
- 未发现 `ContextProjector` 对 tool result content 做全局大小限制；`continuation.rs` 的 `json_preview` 只覆盖无法解析的 raw JSON fallback，不覆盖已解码的 `AgentToolResult.content`、dynamic content items 或 command aggregated output。
- 未发现前端 `rawEvents` 有容量限制、payload 脱水或 lazy detail 机制。
- 本次未研究具体 PiAgent tool producer/terminal owner storage/cache provider 的实现位置；这里只给出 SessionEvent/stream/projection 链路风险与所需事件形状。
