# 会话事件仓储粒度收敛 — 技术设计

## 现状链路

```text
connector(stream_mapper) → BackboneEnvelope
  → SessionTurnProcessor → SessionEventingService::persist_notification
  → bound_envelope_for_append (单条大 payload guard)
  → SessionEventStore::append_event   ← 无类型过滤，全部 durable
  → advance_model_projection_head + broadcast
重放/投影:
  ContextProjector::build_model_context
  → SessionEventStore::list_all_events  ← 永远全量 SELECT
  → build_raw_projected_transcript_from_filtered_events (Rust 内 filter suffix)
```

三个事实（已核对）：
- `append_event` 对事件类型零过滤（`session_repository.rs:319`）。
- `list_all_events` 永远全量（`session_repository.rs:495`），suffix filter 在 Rust 内（`context_projector.rs:206`）。
- 助手正文/reasoning 重放唯一来源是 text delta（`continuation.rs:270-321`）；`ItemCompleted` 不重建助手文本（`continuation.rs:745` 走 `_ => None`）。

## 设计分层：先读后写

整改拆成两个解耦里程碑，**M1 先行**（低风险、高 ROI、不触重放语义），M2 后做（触重放正确性，需迁移期回放）。

---

## M1：读放大止血（suffix-only 读取）

### 目标
让"有 projection head 时只读 suffix"在 **DB 层**成立，而非把全量拉回 Rust 再 filter。

### 接口改造（SPI）
在 `SessionEventStore`（`agentdash-spi/src/session_persistence.rs:797`）新增按区间读取：

```rust
/// 读取 event_seq >= from_seq 的事件（升序）。from_seq=0 等价全量。
async fn list_events_from(
    &self,
    session_id: &str,
    from_seq: u64,
) -> SessionStoreResult<Vec<PersistedSessionEvent>>;
```

- Postgres 实现：`WHERE session_id = $1 AND event_seq >= $2 ORDER BY event_seq ASC`，复用现有 `(session_id, event_seq)` 索引。
- in-memory 实现（`test-support/session_memory_persistence.rs`）：同义 filter。
- 保留 `list_all_events` 作为 `list_events_from(.., 0)` 的兼容入口，逐步迁移调用点，避免一次性大改。

### ContextProjector 改造
`context_projector.rs` 三个入口当前都先 `list_all_events` 再 filter。改为先解析 head / compaction，再只读所需区间：

- **无 head**（raw 全量）：仍需全量 → `list_events_from(.., 0)`。这是冷会话/未压缩会话，无法避免。
- **有 head 且无 active compaction**：只需 `event_seq <= head.head_event_seq` 且 ≥ 上一压缩边界。当前实现 prefix 也来自 raw events，但**无 compaction 时 prefix 就是 0..head**，无法只读 suffix——除非有更早的 projection 边界。结论：无 compaction 的 head 仍是全量读，收益有限。
- **有 active compaction（主要收益场景）**：prefix 来自 `compaction` + `segments`（已是 materialized checkpoint，不读 raw events），suffix 仅 `[suffix_start_event_seq, head_event_seq]`。把 `build_from_projection_head` 的 `list_all_events` 替换为 `list_events_from(session_id, suffix_start_event_seq)`，并在调用点先算出 `suffix_start`（`suffix_start_event_seq_from_compaction`）再传入。这是读放大最大的来源（长会话必然已压缩），收益最高。

> 注意：`build_from_projection_head` 当前签名接收 `events: &[PersistedSessionEvent]`（由调用方预读）。改造把"读取时机"下沉到确定 suffix_start 之后；需要调整 `build_model_context` / `build_model_context_at_event` / `build_model_context_from_compaction` 三处的读取顺序：先读 head/compaction，再按需读事件区间。

### 其它 list_all_events 调用点
`turn_processor:515`、`runtime_control:74`、`branching:290/644`、`lifecycle/surface/journey/mod.rs:96`、`eventing.rs` 多处。本里程碑**只改 context_projector 热路径**；其余调用点评估后单独迁移（多数是低频或确实需要全量，如 branching 复制父会话）。在 implement.md 标注逐个判定。

### 兼容性
- 旧会话残留大量 delta：M1 不改 durable 内容，只改读取区间。有 compaction 的旧会话 suffix 内可能仍含 delta，但只读 suffix 已显著降量；M2 再处理 durable 粒度。
- 投影产物必须 byte-for-byte 等价：用同一 session 在改造前后对比 `AgentContextEnvelope.messages`。

---

## M2：进度态事件 durable 收敛

### Step 0（前置硬依赖）：终态承载助手正文
当前助手正文/reasoning 仅由 text delta 重建。M2 任何裁剪前，必须先让重放不依赖 delta：

方案 A（推荐）：turn 收尾时由 `stream_mapper` 的 `MessageEnd` 落一条**终态助手消息事件**（bounded final text + reasoning），并让 `continuation.rs` 的 raw projection 优先消费该终态事件重建助手消息；delta 仅在终态缺失时作为 fallback。
- `MessageEnd`（`stream_mapper.rs:923`）已持有完整 `content`（text + reasoning）。当前它只补发"残余 delta"+ TokenUsage，不落终态消息。新增一条 `ItemCompleted(assistant_message item)` 或一个 Platform/专用终态事件承载 final 文本。
- `continuation.rs` 在 `ItemCompleted` 分支增加 assistant message item 的文本提取（不再 `_ => None`），作为助手正文/reasoning 的权威来源；按 `turn_id + entry_index` 与 delta 去重。

方案 B（不推荐）：保留 delta 为权威，但把整段 delta 在 turn 收尾 compact 成单条聚合事件并删除原 delta。物理删除破坏 append-only 审计语义，复杂度更高。采用方案 A。

异常中断：turn 未收尾即断开时，`MessageEnd` 不会到达。需在 turn abort/error 收尾路径落一条 **partial assistant snapshot**（已累积文本 + `partial=true` 标记），保证"工具调用在但助手正文丢失"不发生。

### Step 0.5：引入 `ItemUpdated` 协议变体（机制基础）
不再把 ephemeral 语义塞进 `ItemStarted` 标志位，而是给自有扩展协议正式补上缺失的"进度更新"语义：

```text
ItemStarted    : item 首次出现（create-once）——durable，O(1)/item
ItemUpdated    : item 进度刷新（args/preview/partial output 精化）——ephemeral
ItemCompleted  : item 终态——durable
```

这样持久化策略**由事件类型自解释**，不需要"首条 vs 刷新"运行时判定，也不需要 ephemeral 标志位。`stream_mapper` 把当前所有"刷新型 ItemStarted"改发 `ItemUpdated`：
- `ToolCallStart`（`stream_mapper.rs:806`）/`MessageEnd` 补发新 tool_call（`:1032`）/`ToolExecutionStart`（`:1189`）/`ContextCompactionStarted`（`:1071`）：保持 `ItemStarted`（create-once）。
- `ToolCallDelta`（`:863`）/`ToolExecutionUpdate`（`:1245`）的刷新：改发 `ItemUpdated`。

协议触点（Rust exhaustive match 会编译期兜底漏改）：
- `agentdash-agent-protocol/src/backbone/event.rs:28` 新增 `ItemUpdated(ItemUpdatedNotification)`；`backbone/item.rs` 新增 `ItemUpdatedNotification`（镜像 `ItemStartedNotification`）。
- `session_core.rs:738` `backbone_event_type_name` 增 `"item_updated"`；`:769` tool_call_id 提取增 `ItemUpdated`。
- 所有 `match BackboneEvent` 处：`continuation.rs`、`stream_mapper.rs`、`eventing.rs`、`codex_bridge.rs`、`provider_lifecycle.rs`、`lifecycle/surface/journey/session_items.rs`。
- generated TS：跑 backbone protocol 生成器更新 `backbone-protocol.ts`。
- 前端 reducer（`sessionStreamReducer.ts:34/247/441`）：`item_updated` 走与"重复 item_started"等价的 upsert；`item_started` 仅 create；`:441` 的可重放谓词**不**纳入 `item_updated`（ephemeral）。

> 该步可作为**行为保持**的重构先落地：`ItemUpdated` 仍 durable、前端 upsert 行为不变。等 Step 1 再把 `ItemUpdated` 归入 ephemeral 路由。这样协议变更与持久化变更解耦上线。

### Step 1：delta 与 ItemUpdated 改 broadcast-only / 短保留
Step 0/0.5 落地后，下列事件对重放冗余，改为不进长期主日志：

- text delta：`AgentMessageDelta` / `ReasoningTextDelta` / `ReasoningSummaryDelta`
- 过程 delta：`CommandOutputDelta` / `FileChangeDelta` / `McpToolCallProgress`
- `ItemUpdated`（item 进度刷新）

实现位点：`SessionEventingService::persist_notification_inner`（`eventing.rs:154`）。持久化策略按类型分类（白名单 durable，默认 durable）：

```text
classify(envelope) ->
  Durable      : UserInputSubmitted / ItemStarted / ItemCompleted /
                 TurnStarted/Completed / 终态助手消息 /
                 Platform(meta/status/error) / ApprovalRequest / TokenUsage ...
  BroadcastOnly: text delta / 过程 delta / ItemUpdated
```

- `Durable` → 现有 `append_event` 路径。
- `BroadcastOnly` → 直接走 `broadcast_persisted_event` 的等价广播（live tx），**不 append**、不推进 projection head。广播信封不依赖持久化 `event_seq`（或用临时序号 + `ephemeral` 标记），前端/NDJSON 据此不写入可重放 backlog。

### 协议影响
- 新增一个 BackboneEvent 变体（`ItemUpdated`）。扩面集中在 generated TS + 前端 reducer + 各 exhaustive match，Rust 编译期可枚举漏改。
- 终态助手消息：优先复用既有 `ItemCompleted` + assistant message item（前端 reducer 已能 upsert），避免再加变体。

### 短保留 vs 纯 broadcast（决策点）
mid-stream 刷新页面时，前端 hydrate 走 HTTP `/events`（`list_event_page`）+ NDJSON backlog，二者都读 `session_events`。若 delta 纯 broadcast 不落库：
- 取舍：刷新后看不到"正在生成中的"文本，直到下一条 delta 或 turn 收尾终态事件补齐。一旦 turn 收尾，终态事件可完整重建，无最终损失。
- 若产品要求 mid-stream 刷新保真，引入**短保留表**（如 `session_stream_events`，TTL/turn 收尾后清理），NDJSON backlog 合并读取。本任务默认先做纯 broadcast，短保留作为可选增强（implement 标为 optional，按需启用）。

## 数据流（目标态）

```text
durable 主日志(session_events): user input / turn lifecycle / item_started(create) /
  item_completed / 终态助手消息 / platform / token usage / error
ephemeral 广播(live only): text delta / 过程 delta / item_updated(进度刷新)
重放/投影: 只读 durable 主日志的 suffix(M1) + 终态消息(Step0) → 不再依赖 delta
```

## 约束
- append-only：不物理删除历史事件（方案 A 不删，靠终态优先 + ephemeral 不入库）。
- 投影正确性契约不变：`ContextProjector` 产物语义等价，仅来源从 delta 切换到终态。
- 不改大 payload guard 与 compaction 投影语义。
- 兼容旧会话：读取与重放对"旧全量 delta"与"新稀疏 + 终态"两种形态都正确。

## 风险
- Step 0 去重：终态事件与残余 delta 可能双源；`continuation.rs` 必须按 `turn_id+entry_index`/item_id 去重，避免助手正文重复。
- ephemeral 路由错判：把本应 durable 的事件误判为 ephemeral 会造成重放丢失 → 分类必须白名单 durable、默认 durable，只有明确进度态才 ephemeral。
- NDJSON/前端契约：ephemeral 事件若仍走同一 NDJSON 通道，需保证前端不把它写入可重放 `rawEvents` backlog（或接受刷新降级）。
- M1 读取顺序调整可能影响 `build_model_context_at_event` 的 head 覆盖判断（compaction_covers_head），需保留原判定逻辑。
