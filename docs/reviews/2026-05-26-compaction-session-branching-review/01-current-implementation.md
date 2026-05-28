# 当前实现

本文件记录主项目实现事实。结论以代码证据为准；reference 对照与建议见 `02-reference-comparison.md`、`03-findings.md`。

## Compaction 数据流

### 1. 触发入口在 provider 请求前

`crates/agentdash-agent/src/agent_loop/streaming.rs` 在发起 provider request 前构造 `EvaluateCompactionInput`，用当前 `messages_for_llm` 与 provider-visible token 估算调用 `runtime_delegate.evaluate_compaction(...)`。只有 delegate 返回 `CompactionParams` 且 `should_execute_compaction(...)` 为真时，才发出 `ContextCompactionStarted`，执行 `execute_compaction(...)`，并把 `messages_for_llm`、`context.messages`、`request.messages` 全部替换成压缩结果。

这说明 compaction 是运行时 model context 投影替换，不是后台任务，也不是 DB 先行驱动。失败会发 `ContextCompactionFailed`，非取消错误会进入 delegate 的 `after_compaction_failed(...)`。

### 2. 策略由 hook 决定

`crates/agentdash-application/src/session/hook_delegate.rs` 的 `evaluate_compaction` 负责更新 token stats、读取 context window、运行 `HookTrigger::BeforeCompact`。内置 preset `crates/agentdash-application/scripts/hook-presets/context_compaction_trigger.rhai` 的默认策略是：

- `reserve_tokens = 16384`
- `keep_last_n = 20`
- 当 `last_input_tokens > context_window - reserve_tokens` 时触发压缩

hook contract 支持 `cancel`、`reserve_tokens`、`keep_last_n`、`custom_summary`、`custom_prompt`。连续失败达到熔断阈值后，delegate 会停止自动重试并写 diagnostics。这个熔断状态目前是 runtime memory 状态，不是 durable projection 状态。

### 3. 执行算法是 summary-prefix + tail

`crates/agentdash-agent/src/compaction/mod.rs` 的 `execute_compaction` 做三件事：

- 从 `first_uncompacted_message_index` 跳过已有 `CompactionSummary`。
- 用 `find_cut_point` 在 token budget 与 `keep_last_n` 下选择 cut point，并避免切断 tool call / tool result。
- 生成或使用 summary，输出 `[CompactionSummary] + messages[cut_index..]`。

二次 compaction 会读取已有 summary，并使用 update prompt 合并新消息。当前 `AgentMessage::compaction_summary(...)` 默认 `compacted_until_ref: None`，boundary 后续由 application eventing 从事件和 `messages_compacted` 推导。

token 估算是粗粒度的 chars/4 加固定项；tool arguments、tool result、image data 也只做简化估算。它足够支撑预研阶段的保守触发，但不应被当作 provider token truth。

### 4. `pi_agent` 链路会落为 projection checkpoint

`crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` 把 `AgentEvent::ContextCompacted` 转成 platform `SessionMetaUpdate { key: "context_compacted" }`，value 中包含：

- `lifecycle_item_id`
- `summary`
- `tokens_before`
- `messages_compacted`
- `newly_compacted_messages`
- `compacted_until_ref`
- `timestamp_ms`

`crates/agentdash-application/src/session/eventing.rs` 的 `persist_notification` 先 enrich `context_compacted`，再调用 `maybe_commit_compaction_projection`。只有 platform meta event 且 `summary` 非空时，才会创建 projection commit；否则走普通事件 append。

commit 会：

- 读取当前 session 全量事件并推导 base head。
- 计算新的 projection version。
- 推导 `source_start_event_seq`、`source_end_event_seq`、`first_kept_event_seq`。
- 创建 `session_compactions` row、`summary_chunk` segment、`session_projection_heads` head。
- enrich 原事件 payload，写入 `compaction_id`、projection version、source range 等字段。

SQLite/Postgres `commit_compaction_projection` 在一个 DB transaction 中同时递增 session seq、插入 event、更新 session meta、插入 compaction、插入 segments、upsert projection head。

### 5. Codex 原生 compaction 目前只是通知

`crates/agentdash-executor/src/connectors/codex_bridge.rs` 把 `thread/compacted` 映射为 `BackboneEvent::ContextCompacted`。生成的前端协议 `packages/app-web/src/generated/backbone-protocol.ts` 中 `ContextCompactedNotification` 只有 `threadId` 和 `turnId`。

`eventing.rs` 的 projection commit 入口只识别 platform `SessionMetaUpdate { key: "context_compacted" }`，不识别 `BackboneEvent::ContextCompacted`。因此当前证据显示：真实 Codex `thread/compacted` 能让前端收到刷新信号，但不会写入 `session_compactions` / `session_projection_segments` / `session_projection_heads`。

### 6. Projection 重建走 head + checkpoint + suffix

`crates/agentdash-application/src/session/context_projector.rs` 的 `build_model_context(session_id, branch_id)` 优先读取 `model_context` projection head。若存在 active compaction：

- 加载 compaction record。
- 加载同 projection version 的 segments。
- `compaction_checkpoint.rs` 将 `summary_chunk` 或 `context_envelope` 转成 projected entries。
- 再追加 `first_kept_event_seq..head_event_seq` 的 raw suffix。

没有 head 时，projector 会从 raw events 构建 transcript。rollback 只移动 projection head，不删除 raw audit events。

### 7. UI 有 observability 闭环

后端会把 `context_compacted` value 渲染为 compaction context frame；前端 `contextFrame.ts` 与 `SectionRenderers.tsx` 能展示 summary、tokens、projection、checkpoint 等信息。`SessionProjectionView.tsx` 通过 `/sessions/{id}/context/projection` 展示当前模型上下文 projection。`SessionChatView.tsx` 遇到 `turn_completed`、`context_compacted`、compaction summary frame 会刷新 projection 面板。

## Session Branching / Lineage 数据流

### 1. 当前 branch 是跨 session fork

`crates/agentdash-application/src/session/branching.rs` 的 `fork_session` 表达的是 “新建 child session + lineage edge + child 初始 projection”，不是同一个 session 内的多 leaf 树。

流程是：

- 读取 parent session meta。
- 解析 fork point。
- 用 `ContextProjector` 构建 parent 当时的 model context。
- 创建 child session meta，复制 executor config，但 `executor_session_id = None`。
- 写入 `session_lineage` parent-child edge。
- 对 child 提交 `fork_initial_projection`，segment type 为 `context_envelope`，content 中保存 `parent_context.messages` 与 provenance。

child 后续运行时仍通过 `build_model_context(child_id, None)` 读取当前 projection head，因此不需要 runtime 特判 “这是 fork child”。

### 2. Lineage edge 是一 child 一个 parent

`crates/agentdash-infrastructure/migrations/0060_session_lineage.sql` 定义 `session_lineage`：

- `child_session_id` 是 primary key。
- `parent_session_id` 指向 parent。
- `relation_kind` 支持 `fork`、`companion`、`spawned_agent`、`rollback_branch`。
- `fork_point_event_seq`、`fork_point_ref_json`、`fork_point_compaction_id` 保留 fork provenance。
- `status` 支持 `open`、`closed`、`archived`。

SQLite/Postgres repository 有自环检查和 recursive CTE 防环；SPI 也提供 parent、children、ancestors、descendants、status update 能力。

### 3. Fork point 解析顺序

`resolve_fork_point` 的优先级是：

1. `fork_point_event_seq`
2. `fork_point_ref`
3. `fork_point_compaction_id`
4. 当前 `model_context` projection head

解析后会拒绝超过当前模型可见 head 的 event seq。若指定 compaction，会校验 compaction 状态为 `ProjectionCommitted` 且能覆盖目标 head。

当前实现没有强制 fork point 落在 user turn boundary 或完整 tool interaction boundary；裸 event seq 可以成为 fork point。

### 4. Rollback 是 projection head rollback，不是 rollback branch

`rollback_model_projection` 会追加 `session_projection_rolled_back` platform event，并 upsert 当前 session 的 `model_context` projection head 到目标 event seq。它不创建 child session，也不写 lineage edge。虽然 `SessionLineageRelationKind` 里有 `RollbackBranch`，当前 rollback API 没有使用它。

### 5. API 与 bindings

`POST /sessions/{id}/fork` 在 `crates/agentdash-api/src/routes/acp_sessions.rs` 中校验 parent session 的 Edit 权限，调用 `SessionBranchingService::fork_session`，然后复制 parent session bindings 到 child。若复制失败，API 会 best-effort 删除 child session。

这不是单一 repository transaction。`fork_session` 内部也采用 create child -> upsert lineage -> commit projection 的补偿式流程。

`relation_kind_from_dto` 默认 `fork`，但允许客户端提交 `companion`、`spawned_agent`、`rollback_branch`。这让普通 fork endpoint 目前可以写入其他业务语义的 lineage edge。

### 6. 前端展示

`SessionChatView.tsx` 有 “分支” 和 “模型上下文” 按钮，分别打开 `SessionLineageView` 与 `SessionProjectionView`。lineage 面板显示 parent summary、ancestor/child count 和 direct children；它偏诊断视图，不是完整可操作分支树。

项目 session 列表通过 `ProjectSessionEntry.parent_session_id` 与 `parent_relation_kind` 把 relation children 挂到 `linkedChildren`。`active-session-list.tsx` 对普通 story/task children 递归，但对 `linkedChildren` 展开时只渲染一层 `SessionRow`，因此 fork 的 fork 在列表里不会形成完整 relation subtree。

## 已确认 Solid 点

- Compaction 是真实 provider 前执行路径，触发、执行、事件、after hook 形成闭环。
- DB projection store 分层清楚：`session_compactions` 记录 checkpoint 元数据，`session_projection_segments` 记录可 materialize 的投影片段，`session_projection_heads` 记录当前模型可见 head。
- Projection commit 在 SQLite/Postgres 中是事务化的，事件、checkpoint、segment、head 不会正常半提交。
- `compaction_checkpoint.rs` 对 segment / compaction projection kind、version、generated_by_compaction_id 做 materialization 前校验，利于发现坏数据。
- child fork 不复制父原始事件流，而是 materialize 父当时 model context；父会话后续变化不会影响 child 初始上下文。
- rollback 不删除事件，只移动 projection head，保留 audit trail。
- 前端已经有 context frame、projection panel、lineage panel 和 session grouping 基础，具备 observability。

## 已确认薄弱点

- Codex 原生 `thread/compacted` 与项目 projection commit 链路断开。
- `validate_commit_session` 只校验 session_id，没有在 commit 边界校验 branch/kind/version/head/segment ownership invariant。
- `context_compacted` projection commit 依赖 application 层从历史事件推导 boundary，且每次全量 `list_all_events`；无法推导时会直接返回 `None`，不会写 projection commit。
- `AgentMessage::CompactionSummary` 运行时没有携带明确 source boundary，内存压缩上下文与持久化 checkpoint 的 boundary 元数据不同步。
- fork API 的 `relation_kind` 边界过宽，普通 fork 可写入 companion/spawned_agent/rollback_branch。
- fork point 接受裸 event seq，没有强制 turn/message/tool boundary。
- `branch_id` 已进入 projection store，但 live compaction、fork、rollback 均使用 `None`；这需要被明确为 “projection namespace 预留”，避免和跨 session lineage branch 混淆。
- fork / binding copy 是补偿式流程，不是严格单事务。
- lineage UI 与 session list relation UI 仍偏观测，未覆盖完整树、跳转、fork/rollback actions、nested relation children。
