# 会话事件仓储粒度收敛 — PRD

## 背景与问题

`session_events` 当前同时承担三种职责：审计事实、实时 stream payload、模型上下文投影输入。流式/进度态事件被无差别 durable 持久化，并在重放/投影路径被全量读取，带来 DB 吞吐与存储压力。

经代码核对，确认三类现状：

1. **写入无过滤**：`PostgresSessionRepository::append_event`（`session_repository.rs:319`）对事件类型零过滤。文本 delta（`AgentMessageDelta`/`ReasoningTextDelta`/`ReasoningSummaryDelta`）、过程 delta（`CommandOutputDelta`/`FileChangeDelta`/`McpToolCallProgress`）与高频 in-progress `ItemStarted` 全部整条序列化入 `notification_json`，且每条事件在同一事务内额外 `UPDATE sessions`（递增 `last_event_seq` + 状态投影），形成 sessions 行锁热点 + WAL 放大。压力主因是**事件条数**，不是单条大小。

2. **读取全量扫描**：`list_all_events`（`session_repository.rs:495`）永远 SELECT 整个 session 全部行并反序列化，之后才在 Rust 内 `filter(event_seq <= head)`（`context_projector.rs:206`）。即使存在 projection head、逻辑上只需 suffix，物理上仍把全量 delta 拉回反序列化。该函数被 `context_projector`、`turn_processor`、`runtime_control`、`branching`、lifecycle surface 等热路径每轮调用。这是当前主要瓶颈（读放大）。

3. **item_started 爆量**：`stream_mapper` 把 `ItemStarted` 复用为 item 更新通道，对同一 `item_id` 在 `ToolCallDelta`（`stream_mapper.rs:814`）与 `ToolExecutionUpdate`（`stream_mapper.rs:1198`）逐 delta 重发 in-progress `ItemStarted`，全部 durable 持久化。详见 `research/item-started-explosion.md`。

## 关键约束（重放正确性）

当前 raw projection 中**助手正文与 reasoning 的唯一来源是 text delta**：`continuation.rs:270-321` 累加 `AgentMessageDelta`/`ReasoningTextDelta`/`ReasoningSummaryDelta`；`ItemCompleted` 对 AssistantMessage item 走 `_ => None`（`continuation.rs:745`），**不参与助手文本重建**。

因此"直接不持久化 delta"会导致重放后助手正文与 reasoning 整段丢失。任何减少 delta 持久化的改动，**必须先让终态（completed / turn 收尾 final message）承载助手正文**，否则就是数据丢失。

## 目标

- 降低 `session_events` 写入条数与存储占用，去除进度态事件对 durable 主日志的污染。
- 消除重放/投影路径的全量读放大，使每轮投影读成本从 O(全量事件) 降到 O(suffix)。
- 收敛 `item_started` 进度态发射的持久化，使其与 text delta 同属 broadcast-only / 短保留语义。
- 全程保持重放正确性：助手正文、reasoning、tool call、terminal result 在任何裁剪策略下都能被完整重建。

## 非目标

- 不改大 payload guard（`bound_envelope_for_append` 已存在，治"单条大"）。
- 不改前端 live UI 的渐进展示体验（reducer 仍按 item_id upsert）。
- 不重写 compaction/projection 的语义模型；本任务只改"读取粒度"与"durable 粒度"，不改投影正确性契约。
- 不引入外部存储/cache 系统；本任务限定在 session_events 读写粒度与事件落库策略。

## 验收标准

### M1：读放大止血（可独立交付）
- 存在 projection head 时，投影构建只从 DB 读取 suffix（`event_seq >= suffix_start`），不再 `SELECT` 全量行。
- `context_projector` 三个入口（`build_model_context` / `build_model_context_at_event` / `build_model_context_from_compaction`）的产物与改造前等价（同一 session 投影 entries 一致）。
- 新增/改造的仓储读接口有 Postgres 与 in-memory 两套实现，且 hub 重放测试通过。
- 基准：一个含大量 delta 的长会话，单次 `build_model_context` 的 DB 读取行数显著下降（用 EXPLAIN / 计数断言或日志佐证）。

### M2：进度态事件 durable 收敛
- turn 收尾后，助手正文与 reasoning 的重放不再依赖逐条 text delta（终态事件可独立重建助手消息）。
- text delta 与 in-progress `ItemStarted`/过程 delta 改为 broadcast-only 或短保留，不进入长期 `session_events` 主日志（具体形态见 design）。
- 重放正确性回归：对包含助手文本 + reasoning + 多工具调用 + terminal result 的会话，裁剪策略前后投影 entries 等价。
- 异常中断（turn 未收尾即断开）有 partial snapshot / 明确降级，不出现"工具调用在但助手正文丢失"。
- `item_started` 在典型工具调用会话中的持久化条数从 O(参数 delta 数) 降到 O(1) 量级（首条 + 终态）。

## 风险与回滚

- M1 与 M2 解耦：M1 不动 delta 语义，可单独上线验证收益。M2 牵涉重放正确性，必须带迁移期回放测试。
- 兼容已有历史会话：历史 `session_events` 中仍残留大量 delta；读取侧改造必须对"旧数据全量 delta + 新数据稀疏"两种形态都正确重放。
- 详细回滚点见 implement.md。
