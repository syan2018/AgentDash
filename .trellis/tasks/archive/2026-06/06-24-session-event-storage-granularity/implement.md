# 会话事件仓储粒度收敛 — 执行计划

里程碑顺序固定：**M1 先行并可独立交付**，M2 在 M1 验证收益后再做。M2 内部 Step 0 → Step 1 顺序不可颠倒（先终态承载正文，再裁 delta）。

## 执行编排与并行方案

依据文件冲突 + 依赖，真正可干净并行的只有 **M1 ∥ Step 0.5**（文件集不相交）。Step 0 / 0.5 / Step 1 共享 `continuation.rs` / `stream_mapper.rs` / `eventing.rs`，且 Step 1 依赖 Step 0 + Step 0.5，必须串行收尾。

```
Wave 1 (并行):  Track A = M1        (隔离 worktree A)
                Track B = Step 0.5  (worktree B / 主)
Wave 2 (串行):  Track B = Step 0    (复用 B，避免抢 continuation/stream_mapper)
Wave 3 (join):  Step 1              (B 上做，需 Step 0 + Step 0.5 均在)
M1 绿后独立合回，不阻塞 B。
```

文件归属（互斥，保证 Wave 1 无冲突）：
- **Track A / M1**：`spi/session_persistence.rs`、`postgres/session_repository.rs`、`test-support/session_memory_persistence.rs`、`session/persistence.rs`、`context_projector.rs`、`turn_processor.rs` 内的 store mock。
- **Track B / Step 0.5**：`backbone/event.rs`、`backbone/item.rs`、`session_core.rs`、`continuation.rs`、`stream_mapper.rs`、`eventing.rs`、`codex_bridge.rs`、`provider_lifecycle.rs`、`session_items.rs`、generated TS、`sessionStreamReducer.ts`。

分阶段提交（4 点，各自独立可回退）：
1. `feat(session): suffix-only 投影读取`（M1 绿后）
2. `refactor(protocol): 新增 ItemUpdated 变体`（Step 0.5 绿后，行为保持）
3. `feat(session): turn 终态承载助手正文`（Step 0 绿后）
4. `feat(session): delta/ItemUpdated 转 ephemeral`（Step 1 绿后，默认开关关）

## Milestone 1：suffix-only 读取（低风险，高 ROI）

### Checklist
- [ ] SPI 新增 `SessionEventStore::list_events_from(session_id, from_seq)`（`agentdash-spi/src/session_persistence.rs:797`）。
- [ ] Postgres 实现：`WHERE session_id=$1 AND event_seq>=$2 ORDER BY event_seq ASC`（`session_repository.rs`），确认走 `(session_id, event_seq)` 索引。
- [ ] in-memory 实现同义（`test-support/session_memory_persistence.rs:196` 附近）。
- [ ] `ContextProjector` 三入口改读取顺序：先解析 head/compaction，算出 `suffix_start_event_seq`，再按需读区间；有 active compaction 时 `build_from_projection_head` 只 `list_events_from(suffix_start)`（`context_projector.rs:154-220`）。
- [ ] 保留无 head / 无 compaction 的全量路径（`list_events_from(.., 0)`），不回归。
- [ ] 评估其余 `list_all_events` 调用点是否迁移：`turn_processor:515`、`runtime_control:74`、`branching:290/644`、`lifecycle/surface/journey/mod.rs:96`、`eventing.rs:364/394/433/490/1658/1789`。逐个判定"是否真的需要全量"，本里程碑只迁移确认安全的热路径，其余记录结论。
- [ ] 等价性测试：同一 session（含 compaction + suffix delta）改造前后 `build_model_context().messages` 一致。
- [ ] 读量佐证：长会话单次投影 DB 读取行数下降（计数断言或 EXPLAIN/日志）。

### Validation
- `cargo test -p agentdash-application session` / `context_projector` / hub 重放相关用例
- `cargo test -p agentdash-infrastructure session_repository`
- `cargo build -p agentdash-spi`

### Rollback Points
- `list_events_from` 仅新增、不改 `list_all_events`；若 projector 改造出问题，单点回退 projector 读取调用即可，接口保留无害。

---

## Milestone 2：进度态事件 durable 收敛

### Step 0 — 终态承载助手正文（前置硬依赖）
- [ ] `stream_mapper` `MessageEnd`（`stream_mapper.rs:923`）在 turn 收尾落终态助手消息事件（优先 `ItemCompleted(assistant message item)`，bounded final text + reasoning）。
- [ ] `continuation.rs` `ItemCompleted` 分支提取 assistant message 文本/reasoning（移除该形态的 `_ => None`），作为助手正文权威来源；按 `turn_id+entry_index` 与 delta 去重（`continuation.rs:350-377`、`extract_tool_call_from_thread_item:631`）。
- [ ] turn abort/error 收尾路径落 partial assistant snapshot（`partial=true`），覆盖未收尾中断。
- [ ] 回归：仅有终态事件（无 delta）的会话能完整重建助手正文 + reasoning + tool call + terminal result。

### Step 0.5 — 引入 `ItemUpdated` 协议变体（行为保持重构，可独立上线）
- [ ] `agentdash-agent-protocol/src/backbone/event.rs:28` 新增 `ItemUpdated(ItemUpdatedNotification)`；`backbone/item.rs` 加 `ItemUpdatedNotification`（镜像 `ItemStartedNotification`）。
- [ ] `session_core.rs:738` `backbone_event_type_name` 增 `"item_updated"`；`:769` tool_call_id 提取覆盖 `ItemUpdated`。
- [ ] 补齐所有 `match BackboneEvent` 分支（编译期会报漏改）：`continuation.rs`、`stream_mapper.rs`、`eventing.rs`、`codex_bridge.rs`、`provider_lifecycle.rs`、`lifecycle/surface/journey/session_items.rs`。
- [ ] `stream_mapper` 刷新型发射改 `ItemUpdated`：`ToolCallDelta`（`:863`）、`ToolExecutionUpdate`（`:1245`）；create-once 保持 `ItemStarted`（`:806/:1032/:1189/:1071`）。
- [ ] 跑 generated TS 生成器更新 `backbone-protocol.ts`。
- [ ] 前端 reducer（`sessionStreamReducer.ts:34/247/441`）：`item_updated` 走与重复 `item_started` 等价的 upsert；`:441` 可重放谓词不纳入 `item_updated`。
- [ ] continuation 重放：`ItemUpdated` 与 `ItemStarted` 一致提取 tool call（in-flight 可见），不影响终态。
- [ ] 本步保持 `ItemUpdated` 仍 durable + 前端行为不变（纯重构），单独验证回归后再进 Step 1。

### Step 1 — delta / ItemUpdated 改 ephemeral
- [ ] 定义事件持久化分类（白名单 durable，默认 durable；ephemeral = text delta + 过程 delta + `ItemUpdated`）。
- [ ] `SessionEventingService::persist_notification_inner`（`eventing.rs:154`）按分类路由：durable→`append_event`；ephemeral→仅广播、不 append、不推进 projection head。
- [ ] ephemeral 信封不污染可重放 backlog：确认 NDJSON `read_backlog` / `/events` 不返回 ephemeral；前端 `rawEvents` 不写入 ephemeral（或接受 mid-stream 刷新降级）。
- [ ] （optional）若需 mid-stream 刷新保真：引入短保留 `session_stream_events`，turn 收尾/TTL 清理，NDJSON backlog 合并读取。默认不做。
- [ ] 回归：含助手文本 + reasoning + 多工具 + terminal 的会话，裁剪前后投影 entries 等价；`item_updated`/delta 不再进 `session_events`，单工具调用持久化 item 事件降到 O(1)（started + completed）。

### Validation
- `cargo test -p agentdash-executor stream_mapper` / `connector_tests`
- `cargo test -p agentdash-application`（重放、eventing、hub）
- `pnpm --filter app-web test -- sessionStreamReducer`（确认 ephemeral 不破坏前端 upsert）
- 如改 generated TS：`cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts -- --check`

### Rollback Points
- Step 0 独立可上线（只增终态事件 + 重放优先级），不裁 delta 也安全；若 Step 1 出问题回退到"Step 0 已落、delta 仍 durable"的稳定态。
- ephemeral 分类用开关控制（默认全 durable），灰度开启；异常时一键回到全 durable。

---

## 跨里程碑验收门
- 长会话写入条数 / 存储占用对比基线下降（M2）。
- 长会话单轮投影 DB 读取行数下降（M1）。
- 重放正确性：助手正文/reasoning/tool call/terminal 在新旧数据形态下都完整。
- 无 append-only 语义破坏（不物理删历史事件）。
