# RuntimeSession Internal Model Research

研究约束：本结论只基于代码、tests、migration、contracts 与 `.trellis/spec/`；未读取 `.trellis/tasks/` 下已有规划文档或 references。

## 基本真理

1. RuntimeSession 的唯一硬事实是“一个可独立追加、重放、恢复的 runtime trace”。

   它不是 AgentRun，不是业务 workspace，也不是 Lifecycle 控制面。Session spec 已把目标语义写清楚：当前 `Session` 就是 `RuntimeSession`，只拥有 turn / tool / event / resume / debug / projection / trace lineage，不拥有业务归属、permission scope、Lifecycle progress 或 Agent effective surface（`.trellis/spec/backend/session/architecture.md:5`）。同一 spec 进一步说明 RuntimeSession 是 delivery / trace substrate，AgentRun command 以 AgentRun workspace public identity 为目标，RuntimeSession 只负责 trace refs、event log、connector continuation 与 repository rehydrate（`.trellis/spec/backend/session/architecture.md:30`）。

2. 业务控制面事实必须从 AgentRun / Lifecycle / AgentFrame 闭包得到，RuntimeSession 只提供 trace ref。

   runtime-execution spec 规定 AgentRun lifecycle surface 使用 `run_id + agent_id + frame_id` 作为业务索引，`RuntimeSession` 只以 `MessageStreamProjectionRef` 进入 projector（`.trellis/spec/backend/session/runtime-execution-state.md:26`）。同一节要求 `runtime_session_id` 经 `RuntimeSessionExecutionAnchor` 回到 AgentRun runtime address，再闭合 VFS、MCP servers、CapabilityState、backend anchor、identity/admission context 与 provenance（`.trellis/spec/backend/session/runtime-execution-state.md:31`）。

3. RuntimeSession event log 是事实源；SessionMeta 是 trace-head cache。

   `SessionMeta` 当前包含 `id/title/title_source/created_at/updated_at/last_event_seq/last_delivery_status/last_turn_id/last_terminal_message/executor_session_id`（`crates/agentdash-spi/src/session_persistence.rs:304`）。Postgres `append_event` 先在 `sessions` 上原子递增 `last_event_seq`，再插入 `session_events`，最后把 envelope 派生出的 delivery status、turn id、terminal message、executor id 写回 session cache（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:330`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:358`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:374`）。这说明运行态字段不是源事实，而是事件日志 head 的缓存投影。

4. event log 的最小正确形态是 envelope-only append log。

   `PersistedSessionEvent` 以 `session_id/event_seq/occurred_at_ms/committed_at_ms/notification` 为核心，并把 ephemeral 标记限定为内存语义：只 live 广播，不写 DB、不推进 projection head（`crates/agentdash-spi/src/session_persistence.rs:531`, `crates/agentdash-spi/src/session_persistence.rs:543`）。migration 0040 已删除 `session_events` 的 `session_update_type/turn_id/entry_index/tool_call_id` 冗余列（`crates/agentdash-infrastructure/migrations/0040_session_events_envelope_only.sql:1`），与这个最小模型一致。

5. Projection 是模型输入 checkpoint，不是事件历史、UI timeline 或 session tree。

   compaction spec 明确：`session_events` 保存真实发生事实，compact 不改写历史事件，只提交新的模型上下文 projection（`.trellis/spec/backend/session/context-compaction-projection.md:9`）。成功 compact 用 `session_compactions`、`session_projection_segments`、`session_projection_heads` 三类对象表达可恢复状态（`.trellis/spec/backend/session/context-compaction-projection.md:11`）。Projection head 的 key 是 `(session_id, projection_kind)`；session tree topology 由 `session_lineage` 表达，projection store 只记录某个 session 当前可恢复模型输入（`.trellis/spec/backend/session/context-compaction-projection.md:21`）。

6. terminal fact 与 terminal effect 必须分离。

   runtime-execution spec 要求 terminal fact 先持久化为事件，业务副作用进入 durable outbox，副作用失败不回滚 terminal event（`.trellis/spec/backend/session/architecture.md:35`）。代码里 terminal effect 是独立 outbox record，包含 `terminal_event_seq/effect_type/payload/status/attempt_count`（`crates/agentdash-spi/src/session_persistence.rs:504`），dispatcher 先 insert outbox，再执行，失败只更新 effect 状态和诊断（`crates/agentdash-application-runtime-session/src/session/terminal_effects.rs:211`, `crates/agentdash-application-runtime-session/src/session/terminal_effects.rs:248`）。

7. runtime command 不是用户 command。

   `RuntimeCommandRecord` 当前表达 delivery runtime session 上的 pending runtime context / frame transition 指令（`crates/agentdash-spi/src/session_persistence.rs:390`, `crates/agentdash-spi/src/session_persistence.rs:418`）。spec 也说 pending runtime delivery command 只保存投递指令，`AgentFrameTransitionRecord` 保存可 replay 的 frame surface transition，不保存完整 `CapabilityState` projection（`.trellis/spec/backend/session/architecture.md:36`）。用户 command receipt 属于 AgentRun command projection，并以 run / agent / frame / runtime session / turn refs 表达 accepted result（`.trellis/spec/backend/session/runtime-execution-state.md:178`）。

8. lineage 至少有三层，不能混用。

   `AgentLineage` 是同一 run 内的 agent 控制树；代码注释明确 UI 控制树使用 AgentLineage，RuntimeSessionLineage 只保留 trace/debug 语义（`crates/agentdash-domain/src/workflow/agent_lineage.rs:5`, `crates/agentdash-domain/src/workflow/agent_lineage.rs:7`）。`AgentRunLineage` 是跨 run provenance，链接 forked child AgentRun 到 parent AgentRun/runtime trace boundary（`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:6`）。`session_lineage` 只解释 independently resumable runtime traces 的关系，product control tree 使用 AgentRun workspace projections 与 AgentRun scoped endpoints（`.trellis/spec/backend/session/session-lineage-projection.md:5`, `.trellis/spec/backend/session/session-lineage-projection.md:37`）。

9. RuntimeSessionExecutionAnchor 是 launch evidence，语义上 immutable。

   anchor 注释说明它在 dispatch / orchestrator launch 创建 RuntimeSession 时同步写入，让 runtime trace 能稳定反查 lifecycle 控制面；它是 launch evidence，记录创建时刻的 frame/agent/orchestration node，不被后续 frame revision 覆盖（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:20`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:25`）。这决定 anchor 不能承担“当前 frame/current delivery selection”的可变职责。

## 推荐设计（局部最优）

### 1. RuntimeSession 的唯一职责

RuntimeSession 应定义为：

```text
RuntimeSession = Durable runtime trace
  identity
  append-only BackboneEnvelope event log
  trace-head metadata cache
  connector continuation handle
  projection checkpoints for model-visible context
  internal live-turn coordination hooks
  terminal side-effect outbox
  internal runtime delivery command outbox
  trace lineage for independently resumable child traces
```

RuntimeSession 不应承担：

- AgentRun workspace shell、列表状态、command availability、mailbox、用户可见 composer 行为。
- Project / Subject / Lifecycle / Agent ownership 与 permission scope。
- AgentFrame current surface、VFS/MCP/capability/identity/admission facts。
- AgentRun product lineage、同 run agent 控制树、跨 run navigation。
- 用户 command receipt、client command idempotency、fork-submit mailbox delivery。
- business resource surface，如 Canvas、WorkspaceModule、PermissionGrant、Terminal target、RuntimeGateway MCP access 的 current closure。

### 2. 最小边界

#### Event Log

最小边界：

- `append_event(session_id, BackboneEnvelope) -> PersistedSessionEvent` 是唯一事实写入入口。
- repository 在一个提交单元内分配 `event_seq`、插入 envelope、推进 trace-head cache。
- durable event 不保存可从 envelope 派生的 `turn_id/session_update_type/tool_call_id` 等索引列；需要索引用 projection/read model 生成。
- ephemeral event 只属于 live transport buffer，不进入 durable event log，不推进 projection head。

理由：

- 当前 Postgres append 已经体现“seq 分配 + event insert + meta cache update”事务边界（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:324`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:330`, `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:374`）。
- 0040 已删除 event 表派生列（`crates/agentdash-infrastructure/migrations/0040_session_events_envelope_only.sql:1`）。

#### SessionMeta

最小边界：

- `RuntimeSessionTraceMeta`，不是 product shell。
- 字段只保留 trace/feed/debug/recovery 所需：`runtime_session_id`、`created_at`、`updated_at`、`last_event_seq`、`executor_session_id`、trace title/title source、`last_delivery_status`、`last_turn_id`、`last_terminal_message`。
- title 是 trace title，不是 AgentRun display title 的权威源；AgentRun shell 可以引用 trace title，但必须由 AgentRun projection 决定最终 display。
- 运行状态字段只能由 event append、connector accepted/terminal、recovery maintenance 推进；不允许任意 caller 保存整块 meta。

理由：

- spec 已定义 `SessionMeta` 是 RuntimeSession repository 内部保存的 trace-head metadata，浏览器合同以 `RuntimeSessionTraceMeta` 暴露 trace facts，用于 trace/feed/debug、rehydrate、follow-up、branch/rollback projection、进程重启后的 execution state recovery（`.trellis/spec/backend/session/runtime-execution-state.md:152`）。
- AgentRun workspace shell 的事实来源是 ProjectAgent / Subject association / LifecycleAgent / AgentFrame / active turn / command receipt 等 AgentRun 控制面事实，而不是 RuntimeSession meta（`.trellis/spec/backend/session/runtime-execution-state.md:172`）。

#### Projection

最小边界：

- Projection 是 `event log + compaction checkpoint + projection segments + projection head` 的可恢复模型输入。
- `model_context` 是第一类 projection kind；timeline/audit/handoff 若保留，也应是派生 read model，不应改变 event log。
- rollback 只能移动 projection head 并追加 audit event；不能删除或改写 events。
- fork child 必须 materialize 自己的 initial projection，不能在运行时依赖 parent live projection。

理由：

- ContextProjector 读取顺序是 `session_projection_heads(model_context) -> active session_compactions -> session_projection_segments -> suffix session_events -> AgentContextEnvelope`（`.trellis/spec/backend/session/context-compaction-projection.md:59`）。
- fork / rollback / lineage 消费 checkpoint surface 与 projection heads；Projection head 表示“该 session 当前模型可见到哪里”，lineage edge 表示“该 session 从哪里来”（`.trellis/spec/backend/session/context-compaction-projection.md:124`）。

#### Terminal Effects

最小边界：

- terminal event 是事实；terminal effect 是 outbox。
- outbox record 只能引用 `session_id + turn_id + terminal_event_seq`，表达“terminal fact 之后要执行的副作用”。
- effect status mutation 只属于 outbox worker / replay maintenance：pending/running/succeeded/failed/dead_letter。
- effect 失败只更新 effect record 和诊断，不回滚 terminal event、不修改 AgentRun 状态源事实。

#### Runtime Commands

最小边界：

- 命名应收敛为 `runtime_session_delivery_commands` 或 `runtime_delivery_commands`，避免和 AgentRun 用户 commands 混淆。
- 只服务 active delivery session 的 runtime context/frame transition 应用。
- 状态 mutation 只允许 internal worker 标记 requested/applied/failed。
- 不承载 client command id、request digest、command duplicate/conflict、accepted refs；这些属于 `agent_run_command_receipts`。

#### Lineage

最小边界：

- RuntimeSession lineage 只记录 runtime trace branch provenance：一个 child runtime trace 的 primary parent edge。
- 最小 relation_kind 只有 `fork`。`rollback_branch` 只有在“rollback materializes a new child RuntimeSession”时才需要；当前 rollback 是移动 head，因此不应是 lineage edge。
- `companion` / `spawned_agent` 不应属于 session lineage；它们暗含 lifecycle policy、visibility 和 restore 行为，应由 AgentLineage / AgentRunLineage / Lifecycle services 表达。
- Product fork 必须走 AgentRun scoped fork：先 runtime projection fork，再 materialize LifecycleRun/LifecycleAgent/AgentFrame/mailbox/agent_run_lineages。

理由：

- session lineage spec 明确 RuntimeSession lineage 不是 product interaction surface，因为它不 materialize LifecycleRun、LifecycleAgent、AgentFrame、AgentRun mailbox 或 cross-run AgentRun lineage facts（`.trellis/spec/backend/session/session-lineage-projection.md:35`）。
- AgentRun fork canonical flow 是 `AgentRunForkService -> SessionBranchingService::fork_session -> AgentRunForkMaterializationPort -> AgentRunForkOutcomeView`（`.trellis/spec/backend/session/session-lineage-projection.md:143`）。

### 3. Mutation API

RuntimeSession 不应有通用 mutation API。允许的写入全部应是 narrow internal/maintenance API：

- `create_runtime_session_trace`: 由 launch/fork/orchestration materialization 创建 trace shell。
- `append_event`: connector stream、platform event、context frame、terminal marker 的唯一事实入口。
- `set_trace_title`: 只改 trace title/title_source，且必须遵守 title policy；product rename 应从 AgentRun workspace command 进入，再落到 trace title 或 workspace title projection。
- `commit_compaction_projection`: 只由 `context_compacted` event 的持久化路径触发，并与 completed event 同提交单元。
- `rollback_projection_head`: internal diagnostics / maintenance，只移动 projection head 并追加 audit event。
- `enqueue_terminal_effect` / `mark_terminal_effect_*`: terminal outbox worker。
- `upsert_runtime_delivery_command` / `mark_runtime_commands_*`: live runtime transition worker；建议改名为 insert/request delivery command，减少 upsert 语义。
- `delete_runtime_session_trace`: maintenance cleanup，只用于失败补偿、AgentRun delete cascade 或明确 trace GC；不是 product-level delete。

应删除或改造：

- `SessionCoreService::update_session_meta<F>` 这种泛型整块 updater。当前它能让任意调用方拿到并改完整 `SessionMeta`（`crates/agentdash-application-runtime-session/src/session/core.rs:105`），即使 API 当前只 patch title（`crates/agentdash-api/src/routes/sessions.rs:1040`）。
- `SessionMetaStore::save_session_meta(&SessionMeta)` 作为应用层端口。repository 内部可以有私有 upsert，但 application port 应拆成 `create_trace`、`set_trace_title`、`advance_trace_head_from_event`、`record_executor_session` 等带语义的方法。

### 4. RuntimeSession 与 AgentRun 引用关系

推荐关系：

```text
AgentRun/LifecycleAgent owns current delivery binding:
  run_id
  agent_id
  current_frame_id
  current_delivery.runtime_session_id
  current_delivery.launch_frame_id
  current_delivery.status
  current_delivery.observed_at

RuntimeSessionExecutionAnchor is immutable launch evidence:
  runtime_session_id -> run_id + agent_id + launch_frame_id + optional orchestration node

RuntimeSession owns no AgentRun state:
  event log / projection / trace meta / connector continuation only
```

具体规则：

- AgentRun 引用 RuntimeSession，只存 ref，不嵌入或反向拥有。
- RuntimeSessionExecutionAnchor 创建后不可变；后续 AgentFrame revision、current frame、surface update 不改 anchor。
- current delivery selection 必须从 AgentRun/LifecycleAgent current delivery binding 出发，再用 anchor 校验，不允许用 anchor `updated_at` 推导 current。
- `RuntimeSessionExecutionAnchorRepository` 应从 `upsert` 改为 `create_once` / `insert`；重复写同一 `runtime_session_id` 且内容不同应报 conflict。
- `latest_updated_anchor_for_agent` 不应是 delivery selection policy。若需要“当前 delivery”，读 LifecycleAgent current delivery binding；若需要“历史 runtime traces”，按 run/agent 列 trace refs。

当前正向证据：

- `DeliveryRuntimeSelectionService` 已从 `agent.current_delivery.runtime_session_id` 读取 delivery session，再查 anchor 并校验 run/agent/launch_frame（`crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:151`, `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:155`, `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:268`）。
- selection output 把 runtime session 当作 `MessageStreamProjectionRef`，并把 business address 保持为 run/agent/frame（`crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:254`, `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:259`）。

当前反向证据：

- `resource_surface_for_agent_run` 仍用 `latest_updated_anchor_for_agent(agent_id)` 找 delivery anchor（`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:128`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:133`）。这应收敛到 current delivery binding。
- anchor repository trait 当前暴露 `upsert` 与 `latest_updated_anchor_for_agent`（`crates/agentdash-domain/src/workflow/repository.rs:153`, `crates/agentdash-domain/src/workflow/repository.rs:181`），与 immutable anchor 模型冲突。

### 5. SessionPersistence 是否继续作为聚合接口

不应继续。

当前 `SessionPersistence` 聚合 7 个子 store：meta、events、terminal effects、runtime commands、compactions、projections、lineage（`crates/agentdash-spi/src/session_persistence.rs:949`, `crates/agentdash-spi/src/session_persistence.rs:953`）。`SessionStoreSet::from_persistence` 再把同一个 mega trait adapter clone 成 7 个 store（`crates/agentdash-application-runtime-session/src/session/persistence.rs:20`, `crates/agentdash-application-runtime-session/src/session/persistence.rs:31`）。

这违反最小权限：只需要 event log 的 reader 被迫拿到 terminal effect mutation、lineage mutation、runtime command mutation；只需要 runtime session creation 的 lifecycle dispatcher 被迫依赖整个 SessionPersistence（`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:31`, `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:34`）。

推荐：

- 删除 `SessionPersistence` mega trait。
- 保留窄 trait：`RuntimeSessionMetaRepository`、`RuntimeSessionEventLog`、`RuntimeSessionProjectionRepository`、`RuntimeSessionTerminalEffectOutbox`、`RuntimeSessionDeliveryCommandOutbox`、`RuntimeSessionLineageRepository`。
- `SessionStoreSet` 可作为 runtime-session module 内部依赖包，但构造函数必须显式接收各窄 port，不再从 mega trait 派生。
- 跨 bounded-context 调用只能依赖所需 port。例如 Lifecycle dispatch 创建 trace 只依赖 `RuntimeSessionCreationPort`； Lifecycle VFS journey 只依赖 event/projection read ports； terminal effect worker 只依赖 outbox port。

### 6. 最小仓储 / 表 / port 形态

推荐表形态：

```text
runtime_sessions
  id pk
  title
  title_source
  created_at_ms
  updated_at_ms
  last_event_seq
  last_delivery_status
  last_turn_id
  last_terminal_message
  executor_session_id

runtime_session_events
  session_id fk -> runtime_sessions(id)
  event_seq
  occurred_at_ms
  committed_at_ms
  notification_json
  pk(session_id, event_seq)

runtime_session_compactions
  id pk
  session_id fk
  projection_kind
  projection_version
  lifecycle_item_id
  start_event_seq
  completed_event_seq
  failed_event_seq
  status
  trigger/reason/phase/strategy/budget_scope
  base_head_event_seq
  source_start_event_seq
  source_end_event_seq
  first_kept_event_seq
  summary
  replacement_projection_json
  token_stats_json
  diagnostics_json
  created_by
  created_at_ms
  completed_at_ms

runtime_session_projection_heads
  session_id
  projection_kind
  projection_version
  head_event_seq
  active_compaction_id
  updated_by_event_seq
  updated_at_ms
  pk(session_id, projection_kind)

runtime_session_projection_segments
  id pk
  session_id
  projection_kind
  projection_version
  sort_order
  segment_type
  origin
  synthetic
  source_start_event_seq
  source_end_event_seq
  source_refs_json
  generated_by_compaction_id
  content_json
  token_estimate
  created_at_ms
  unique(session_id, projection_kind, projection_version, sort_order)

runtime_session_terminal_effects
  id pk
  session_id
  turn_id
  terminal_event_seq
  effect_type
  payload_json
  status
  attempt_count
  created_at_ms
  updated_at_ms
  last_error

runtime_session_delivery_commands
  id pk
  session_id
  phase_node
  status
  payload_json
  frame_transition_id
  created_at_ms
  updated_at_ms
  applied_at_ms
  failed_at_ms
  last_error

runtime_session_lineage
  child_session_id pk
  parent_session_id
  relation_kind = 'fork'
  fork_point_event_seq
  fork_point_ref_json
  fork_point_compaction_id
  status
  created_at_ms
  updated_at_ms
  metadata_json

runtime_session_execution_anchors
  runtime_session_id pk fk -> runtime_sessions(id)
  run_id fk -> lifecycle_runs(id)
  agent_id fk -> lifecycle_agents(id)
  launch_frame_id fk -> agent_frames(id)
  orchestration_id
  node_path
  node_attempt
  created_by_kind
  created_at
```

说明：

- 当前 `sessions/session_events/session_*` 表已接近这个结构，但命名仍隐藏 RuntimeSession 语义；如果本次收敛目标允许破坏式 migration，应统一 `runtime_session_*` 命名。
- `runtime_session_execution_anchors` 是 bridge / index 表，不属于 RuntimeSession aggregate 内部可变 state。它应由 AgentRun/Lifecycle materialization 创建并由 AgentRun read model 消费。
- `agent_run_mailbox_messages`、`agent_run_mailbox_states`、`agent_run_command_receipts`、`agent_run_lineages` 保持在 AgentRun bounded context；它们不是 RuntimeSession 表。

推荐 port：

```rust
trait RuntimeSessionMetaRepository {
    async fn create_trace(meta: NewRuntimeSessionTrace) -> Result<RuntimeSessionTraceMeta>;
    async fn get_trace(session_id: &str) -> Result<Option<RuntimeSessionTraceMeta>>;
    async fn list_traces(filter: TraceListFilter) -> Result<Vec<RuntimeSessionTraceMeta>>;
    async fn set_trace_title(session_id: &str, title: TraceTitlePatch) -> Result<RuntimeSessionTraceMeta>;
    async fn delete_trace_for_maintenance(session_id: &str) -> Result<()>;
}

trait RuntimeSessionEventLog {
    async fn append_event(session_id: &str, envelope: BackboneEnvelope) -> Result<PersistedSessionEvent>;
    async fn read_backlog(session_id: &str, after_seq: u64) -> Result<SessionEventBacklog>;
    async fn list_events_from(session_id: &str, from_seq: u64) -> Result<Vec<PersistedSessionEvent>>;
}

trait RuntimeSessionProjectionRepository {
    async fn read_head(session_id: &str, kind: ProjectionKind) -> Result<Option<ProjectionHead>>;
    async fn list_segments(session_id: &str, kind: ProjectionKind, version: u64) -> Result<Vec<ProjectionSegment>>;
    async fn commit_compaction(session_id: &str, commit: CompactionProjectionCommit) -> Result<CommitResult>;
    async fn rollback_head_for_diagnostics(session_id: &str, request: ProjectionRollback) -> Result<ProjectionHead>;
}

trait RuntimeSessionTerminalEffectOutbox {
    async fn enqueue(effect: NewTerminalEffect) -> Result<TerminalEffect>;
    async fn claim_pending(limit: u32) -> Result<Vec<TerminalEffect>>;
    async fn mark_running/succeeded/failed/dead_letter(...);
}

trait RuntimeSessionDeliveryCommandOutbox {
    async fn request_delivery_command(command: NewRuntimeDeliveryCommand) -> Result<RuntimeDeliveryCommandRecord>;
    async fn list_requested(session_id: &str) -> Result<Vec<RuntimeDeliveryCommandRecord>>;
    async fn mark_applied(command_ids: &[Uuid]) -> Result<()>;
    async fn mark_failed(command_ids: &[Uuid], error: String) -> Result<()>;
}

trait RuntimeSessionLineageRepository {
    async fn create_child_edge(edge: RuntimeSessionForkEdge) -> Result<()>;
    async fn find_parent(child_session_id: &str) -> Result<Option<RuntimeSessionForkEdge>>;
    async fn list_children(parent_session_id: &str) -> Result<Vec<RuntimeSessionForkEdge>>;
}

trait RuntimeSessionExecutionAnchorRepository {
    async fn create(anchor: RuntimeSessionExecutionAnchor) -> Result<()>;
    async fn find_by_session(session_id: &str) -> Result<Option<RuntimeSessionExecutionAnchor>>;
    async fn list_by_run(run_id: Uuid) -> Result<Vec<RuntimeSessionExecutionAnchor>>;
    async fn list_by_agent(agent_id: Uuid) -> Result<Vec<RuntimeSessionExecutionAnchor>>;
    async fn delete_by_session_for_maintenance(session_id: &str) -> Result<()>;
}
```

## 删除清单

1. 删除 `SessionPersistence` mega trait 与 `SessionPersistenceStoreAdapter`。

   替换为显式窄 port 注入。当前聚合接口在 `crates/agentdash-spi/src/session_persistence.rs:953` 定义，在 `crates/agentdash-application-runtime-session/src/session/persistence.rs:31` 被转换为 `SessionStoreSet`。

2. 删除/收窄 `SessionCoreService::update_session_meta<F>` 与 `SessionMetaStore::save_session_meta(&SessionMeta)` 的应用层可见性。

   以 `set_trace_title`、event append 内部 head update、connector continuation record、maintenance delete 取代。

3. 删除 RuntimeSession product mutation routes。

   `/sessions/{id}/fork`、`/sessions/{id}/lineage`、`/sessions/{id}/projection/rollback` 保持 internal diagnostics；产品 fork / submit / navigation 只走 AgentRun scoped contracts。现有 retained routes 在 API router 中仍暴露（`crates/agentdash-api/src/routes/sessions.rs:115`, `crates/agentdash-api/src/routes/sessions.rs:118`, `crates/agentdash-api/src/routes/sessions.rs:120`）。

4. 删除 session lineage 中的 product-like relation kinds。

   `Companion`、`SpawnedAgent`、`RollbackBranch` 目前在 enum/DTO 中存在（`crates/agentdash-spi/src/session_persistence.rs:691`, `crates/agentdash-contracts/src/runtime/session.rs:326`）。最小 runtime trace lineage 只保留 `fork`；其它语义迁回 AgentLineage / AgentRunLineage / Lifecycle。

5. 删除 anchor `upsert` 的语义，删除 `latest_updated_anchor_for_agent` 作为 selection API。

   anchor 创建点改为 `create_once`，不同内容重复写入报错。`latest_updated_anchor_for_agent` 调用点必须切换到 current delivery binding，例如 `AgentRunRuntimeSurfaceQuery` 当前还在使用它（`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:133`）。

6. 删除“RuntimeSession 拥有 current surface”的隐式路径。

   需要 current VFS/MCP/backend/capability/admission 的路径必须从 AgentRun runtime surface query 获取；spec 已要求 API/业务路径消费 query/update port 的 DTO，不 import `AgentFrame` 或 delivery trace helper（`.trellis/spec/backend/session/runtime-execution-state.md:96`）。

7. 删除 runtime commands 的用户 command 暗示。

   重命名 `session_runtime_commands`/`SessionRuntimeCommandStore`，并确保 AgentRun command receipt 是唯一用户 command idempotency 源。

8. 删除 `sessions` 泛称。

   内部 code/table/contract 命名应迁移到 `runtime_sessions` / `RuntimeSessionTraceMeta` / `RuntimeSessionEventLog`。当前 `SessionShellDto` 可保留 wire 兼容名的理由不足，因为项目未上线；建议直接改成 runtime-session 语义名。

## 迁移/实施顺序

1. 固化词汇与表名。

   先做破坏式 migration：`sessions -> runtime_sessions`，`session_events -> runtime_session_events`，其它 `session_*` runtime trace 表同名迁移。migration 同时保留 0040 的 envelope-only 形态，不恢复派生列。

2. 拆 port，不改行为。

   在 SPI/application boundary 引入窄 ports，并让 Postgres repository 分别实现。先让 runtime-session services 构造时显式接收 `SessionStoreSet` 的窄 store；删除 `from_persistence(Arc<dyn SessionPersistence>)` 适配入口。

3. 收窄 SessionMeta mutation。

   删除泛型 updater 的外部调用。先替换已知调用：API title patch、`SessionTitleService::set_user_title`、launch auto-title、accepted turn commit meta 保存。写入路径改为 field-specific methods；event append 继续在事务内推进 trace-head cache。

4. anchor create-only。

   把 `RuntimeSessionExecutionAnchorRepository::upsert` 改为 `create`，Postgres 用 PK conflict 检测内容是否一致；不一致返回 conflict。更新 launch/materialization 创建点：ProjectAgent launch（`crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:2032`）、AgentRun runtime materializer（`crates/agentdash-application-lifecycle/src/lifecycle/dispatch/agent_runtime_materializer.rs:73`）、workflow executor launch（`crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:790`）。

5. current delivery selection 改为 AgentRun owned。

   所有 read/update current surface 从 LifecycleAgent current delivery binding 出发，再用 anchor 校验。删除 `latest_updated_anchor_for_agent`。已正确的选择路径可作为模板：`DeliveryRuntimeSelectionService` 先读 `agent.current_delivery`，再校验 anchor（`crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:151`, `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:163`）。

6. lineage 收敛。

   RuntimeSession branch/fork 保留 `session_lineage(relation_kind=fork)`；AgentRun fork materialization 必须写 `agent_run_lineages`，其 schema 已包含 parent/child run+agent refs 与 parent/child runtime session ids（`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:1`, `crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:10`）。删除 session lineage 的 companion/spawned/rollback-branch product 暗示。

7. product API 迁移到 AgentRun scoped surface。

   AgentRun composer/fork/cancel/mailbox/resume 继续使用 mailbox/command receipt；raw session routes 只保留 internal diagnostics，权限必须通过 anchor 回到 Project `Use`。Mailbox spec 已把 AgentRun composer、mailbox、cancel 定义为 HTTP command surface（`.trellis/spec/backend/session/agentrun-mailbox.md:32`）。

8. 测试与静态检查。

   增加测试覆盖：event append envelope-only 与 meta cache、anchor create-once conflict、current delivery 不依赖 latest anchor、RuntimeSession fork 不创建 product facts、AgentRun fork 必写 AgentRunLineage/mailbox/command receipt、terminal effect outbox failure 不回滚 terminal event、raw `/sessions` product mutation 不被前端/业务调用。

## 需要验证的代码事实

1. `SessionPersistence` 的所有生产依赖需要逐一拆成窄 port。

   已知生产依赖包括 lifecycle dispatch runtime session creator（`crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:31`）、lifecycle VFS provider、journey surface、API bootstrap repositories/session/vfs、runtime builder/hub。需要确认每处实际只需要哪些 store，并删除 mega trait 注入。

2. `RuntimeSessionExecutionAnchorRepository::upsert` 的生产调用点需要分类。

   已见 ProjectAgent launch 写 anchor（`crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:2032`）、AgentRun runtime materializer 写 anchor 并绑定 current delivery（`crates/agentdash-application-lifecycle/src/lifecycle/dispatch/agent_runtime_materializer.rs:73`）、workflow executor launcher 写 orchestration anchor（`crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:790`）。需要确认哪些是 create-once，哪些只是测试 fixture，哪些会重复写同一 session。

3. `latest_updated_anchor_for_agent` 的生产读路径需要全部替换。

   已知 `AgentRunRuntimeSurfaceQuery::resource_surface_for_agent_run` 使用它（`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:133`）。还需要验证 lifecycle run view builder、lifecycle agent route tests、workspace module surface/tools 是否把 latest anchor 当 current delivery。

4. `SessionMeta` 写入路径需要区分 title policy、event-head maintenance 与 connector continuation。

   直接 updater 当前主要见 API title patch（`crates/agentdash-api/src/routes/sessions.rs:1040`）、title service（`crates/agentdash-application-runtime-session/src/session/title_service.rs:18`）、launch auto-title（`crates/agentdash-application-runtime-session/src/session/launch/deps.rs:262`）、turn commit save meta（`crates/agentdash-application-runtime-session/src/session/launch/commit.rs:71`）。需要确认没有业务路径通过 `save_session_meta` 改运行状态。

5. session lineage relation kinds 是否有真实生产语义。

   检索显示 `Companion` 主要出现在 memory/postgres repository tests 与 DTO mapping；生产 `SessionBranchingService` 写 lineage 使用 fork path（`crates/agentdash-application-runtime-session/src/session/branching.rs:128`）。需要验证是否可以直接删除 `Companion/SpawnedAgent/RollbackBranch`，或先把它们移动到 AgentLineage / AgentRunLineage 语义。

6. raw `/sessions` diagnostics 是否仍被前端产品流调用。

   API router 暴露 `/sessions/{id}/meta`、`/context/projection`、`/lineage`、`/fork`、`/projection/rollback`（`crates/agentdash-api/src/routes/sessions.rs:99`, `crates/agentdash-api/src/routes/sessions.rs:111`, `crates/agentdash-api/src/routes/sessions.rs:115`, `crates/agentdash-api/src/routes/sessions.rs:118`, `crates/agentdash-api/src/routes/sessions.rs:120`）。需要确认前端 product UI 只使用 AgentRun scoped endpoints，raw session routes 仅在 debug/internal detail 中可达。

7. DB migration 顺序需要验证 FK 与 cascade。

   当前 `runtime_session_execution_anchors.runtime_session_id` FK 到 `sessions(id)` 并 cascade（`crates/agentdash-infrastructure/migrations/0002_runtime_session_anchor_fks.sql:7`）；AgentRun mailbox messages/states 也 FK 到 `sessions(id)` 并 cascade（`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:188`, `crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:247`）。重命名到 `runtime_sessions` 时要一次性更新 FK、repository SQL、contracts 与 generated TS。

8. delete semantics 需要从 RuntimeSession delete 转成 AgentRun delete cleanup。

   当前 `delete_session` 手动删除 events、terminal effects、runtime commands、lineage、projection heads/segments、compactions，再删 sessions（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:264`）。AgentRun delete 已调用 session cleanup（`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:101`）。需要确认最终 product delete 从 AgentRun 出发，RuntimeSession delete 只作为 maintenance cascade/cleanup port。

9. RuntimeSession trace read model 要保留 read-only 聚合价值。

   `SessionRuntimeControlView` 当前组合 runtime_session_ref、session_meta、control_plane、anchor、run、agent、frame_runtime、subject_associations（`crates/agentdash-contracts/src/runtime/workflow.rs:1625`）。这可以保留为 diagnostic/read-model，但不能成为 product mutation 的输入 DTO。

10. contract 命名需要统一。

    contracts 里 `RuntimeSessionRefDto` 已是正确引用（`crates/agentdash-contracts/src/runtime/workflow.rs:851`），`RuntimeSessionExecutionAnchorDto` 已表达 launch evidence（`crates/agentdash-contracts/src/runtime/workflow.rs:872`），但 `SessionShellDto` 仍把 RuntimeSession meta 暴露成 Session shell（`crates/agentdash-contracts/src/runtime/workflow.rs:855`）。未上线阶段建议直接重命名为 `RuntimeSessionTraceShellDto` 或并入 `RuntimeSessionTraceMeta`。
