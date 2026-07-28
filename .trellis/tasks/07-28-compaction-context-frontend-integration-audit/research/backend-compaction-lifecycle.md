# Research: Compaction 后端与 Runtime 生命周期审计

- Query: 从手动/自动触发、command admission、运行状态、成功/失败/取消、durable persistence、snapshot/read、live/replay/reconnect 逐层审计 Compaction，区分确认缺陷、设计空洞与已有保障，并评估竞态、owner invariant 和验证矩阵。
- Scope: internal
- Date: 2026-07-28

## 结论

当前 Native Dash compaction 已经有真实的 Agent-owned history transform，也能在成功后把同一份
summary 文本用于 provider 与 canonical `ContextFrameChanged`；但它还没有成为完整、可恢复、可由
前端控制的会话活动协议。

最严重的问题有四类：

1. **实际上下文可能丢失工具事实**：compactor 的摘要输入只包含用户/助手文本，不包含
   `ToolCall` / `ToolResult`；一旦这些工具事实又落在 retained tail 之前，它们既未被摘要看到，也不再
   进入后续 provider context。
2. **当前上下文投影与模型真实输入不一致**：模型使用 “summary ContextFrame + retained suffix”，
   但 `/runtime/context/projection` 把 compaction completed 之前的所有消息全部排除，完全不读取
   `retained_from`，所以用户只看到 summary 和压缩后的新消息，看不到仍在模型上下文里的 retained
   suffix。
3. **compaction activity 没进入 snapshot authority**：Native history 有
   `active_compaction`，canonical live 也有 `ContextCompaction item/started`，但
   `AgentSnapshot` / `AgentObservation` / `ManagedRuntimeSnapshot.command_availability` 只认识 active
   Turn。压缩期间前端仍可能看到 Submit/Compact/Fork 可用，实际请求却被 Agent owner 拒绝。
4. **失败与重启恢复不闭合**：Native `CompactionFailed` 被 canonical projector 映射成普通
   `ItemCompleted(ContextCompaction)`，上下文投影会把失败误当成成功边界；进程若在
   `CompactionStarted` 后、terminal 前退出，durable source 会永久保留 active compaction，但没有
   recovery worker 重启 compactor。

因此，用户提出的三个现象都成立：

- “压缩中”没有形成 snapshot/control-plane 状态；
- 压缩完成后，当前上下文视图不是模型真实 context recipe；
- CompactionSummary frame 只暴露文本，没有完整 checkpoint/provenance/retained membership。

但“把 retained message 全文复制进 ContextFrame”不是推荐修复。Conversation message 仍应由
canonical history 拥有；Compaction ContextFrame 应完整携带 checkpoint identity、summary、
cut/retain 坐标和 usage evidence，上下文投影再按这些坐标组合 summary frame 与 retained canonical
segments。这样既能完整暴露，又不会制造第二份 message 内容。

## Files Found

### 规范与既有研究

- `.trellis/spec/backend/session/architecture.md` — conversation owner、snapshot/live/reconnect 总合同。
- `.trellis/spec/backend/agent-runtime-context.md` — Dash/Codex compaction owner、history transform、
  presentation 和 recovery 合同。
- `.trellis/spec/backend/agent-runtime-native-adapter.md` — Dash source document、provider input、
  durable live、自动压缩期间 Steer 合同。
- `.trellis/spec/backend/agent-runtime-persistence.md` — Product binding 与 concrete Agent source 的
  持久化 owner 边界。
- `.trellis/spec/backend/agent-runtime-kernel.md` — Agent snapshot normalize 与 command availability。
- `.trellis/spec/cross-layer/backbone-protocol.md` — canonical record、ContextFrame、live/reconnect 合同。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` — Runtime HTTP、snapshot/live、context
  projection 跨层合同。
- `.trellis/tasks/07-17-agent-runtime-compaction-state-protocol-review/research/current-compaction-state-and-codex-reference.md`
  — 旧 Runtime compaction saga 的问题清单；owner 已切换到 concrete Agent，不能直接套用旧实现结论。
- `.trellis/tasks/07-23-contextframe-input-authority-restoration/design.md` — ContextFrame 作为已接纳
  platform context 可读投递权威的目标设计。

### 当前实现

- `crates/agentdash-agent/src/dash/history.rs` — Dash native history、folded active compaction、
  CompactionState 和 summary ContextFrame。
- `crates/agentdash-agent/src/dash/lifecycle.rs` — command queue/dependency/effect settlement。
- `crates/agentdash-agent/src/dash/store.rs` — compaction start/complete/fail 原子 commit。
- `crates/agentdash-agent/src/dash/service.rs` — manual compaction、automatic overflow A/B/C、
  context materialization、provider round refresh。
- `crates/agentdash-integration-native-agent/src/bridge_execution.rs` — production compactor、
  effective conversation 与 summary provider request。
- `crates/agentdash-integration-native-agent/src/canonical_projection.rs` — Dash history 到
  canonical Backbone presentation 的映射。
- `crates/agentdash-integration-native-agent/src/service.rs` — Complete Agent execute/read/changes/live/
  inspect、source observation 与 outer effect reconciliation。
- `crates/agentdash-infrastructure/src/persistence/postgres/dash_complete_agent_store.rs` — PostgreSQL
  Dash source/effect persistence 与 CAS。
- `crates/agentdash-agent-service-api/src/snapshot.rs` — Complete Agent snapshot/observation 公共合同。
- `crates/agentdash-agent-runtime/src/agent_snapshot_projection.rs` — Runtime snapshot 与 command
  availability 内存投影。
- `crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs` — Product target 到
  concrete Agent command handoff。
- `crates/agentdash-application-agentrun/src/agent_run/product_projection_gateway.rs` — Product binding
  到 Agent read/live 的查询边界。
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs` — 当前上下文
  popup 的 stateless projection。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` — snapshot/live/command/context projection HTTP。
- `crates/agentdash-integration-codex/src/complete_agent.rs` — Codex native compact command 与
  in-process effect ledger。
- `crates/agentdash-integration-codex/src/canonical_projection.rs` — Codex notification/thread-read 到
  canonical history。
- `crates/agentdash-agent-protocol/src/backbone/context_frame.rs` — ContextFrame 与 structured
  CompactionSummary section。

## Related Specs

以下规范合同与当前实现直接冲突：

- concrete Agent 独占 compaction authority，Product 只保存 association：
  `.trellis/spec/backend/agent-runtime-context.md:8-17`、
  `.trellis/spec/backend/agent-runtime-persistence.md:74-86`。
- Dash transform 应是 `summary + retained suffix + provenance`：
  `.trellis/spec/backend/agent-runtime-context.md:47-65`。
- manual compaction 在 normal Turn active 时应 durable queued，compaction active 时新输入应
  deferred：`.trellis/spec/backend/agent-runtime-context.md:67-79`。
- typed projection 应暴露 started/completed/failed/lost，前端不得固定 completed：
  `.trellis/spec/backend/agent-runtime-context.md:81-92`。
- 自动压缩期间 Steer 应 durable 保留并由 continuation 消费：
  `.trellis/spec/backend/agent-runtime-native-adapter.md:168-176`、
  `.trellis/spec/backend/agent-runtime-native-adapter.md:250-258`。
- UI command availability 必须来自 authoritative Agent snapshot：
  `.trellis/spec/cross-layer/frontend-backend-contracts.md:86-100`。
- disconnect/Lagged 后重读 authoritative snapshot：
  `.trellis/spec/backend/session/architecture.md:38-49`、
  `.trellis/spec/cross-layer/backbone-protocol.md:51-65`。

## End-to-End Lifecycle

### 1. 手动触发

HTTP `request_compaction` 经 Product facade 映射为 `AgentCommand::RequestCompaction`：

- API 映射：`crates/agentdash-api/src/routes/lifecycle_agents.rs:705-757`
- Product facade：`crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs:156-190`
- command mapping：
  `crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs:291-318`
- Native adapter 映射为 `DashPublicCommand::RequestCompaction { Manual }`：
  `crates/agentdash-integration-native-agent/src/service.rs:1464-1492`

Dash service 的真实顺序是：

```text
CompactionStarted durable commit
  -> synchronous compactor LLM call
  -> CompactionApplied + CompactionCompleted durable commit
     或 CompactionFailed durable commit
  -> command/effect terminal receipt
```

证据：

- `crates/agentdash-agent/src/dash/service.rs:1836-1924`
- `crates/agentdash-agent/src/dash/store.rs:125-217`
- `crates/agentdash-agent/src/dash/store.rs:220-257`

这不是 background admission。`execute_admitted` 只把普通 Submit 转为 background；其它命令仍调用
同步 `execute`：`crates/agentdash-agent/src/dash/service.rs:923-951`。因此 compactor 执行多久，
Product command HTTP 就等待多久。虽然 started history 会先 live 发布，但命令请求本身没有及时返回
`Accepted`，也没有可轮询的 Product/Runtime operation。

### 2. 自动 overflow

普通 Turn A 返回 `ContextOverflow` 后，Dash service 在同一 owner 内构造：

- A：失败的原 Turn；
- B：`AutomaticOverflow` compaction command；
- C：依赖 B 的 continuation command。

证据：

- overflow 分支：`crates/agentdash-agent/src/dash/service.rs:1256-1259`
- A/B/C identity 与 dependency：
  `crates/agentdash-agent/src/dash/service.rs:1349-1379`
- A terminal + B/C enqueue + B started：
  `crates/agentdash-agent/src/dash/service.rs:1380-1425`
- B success 后显式 promote C：
  `crates/agentdash-agent/src/dash/service.rs:1483-1521`
- B failure terminalize C / Lost block：
  `crates/agentdash-agent/src/dash/service.rs:1437-1480`、
  `crates/agentdash-agent/src/dash/lifecycle.rs:224-288`

这是已有保障：B clean failure 会让依赖的 C 进入 Failed，B Lost 会让 C Blocked 并把 consistency
置 Lost；测试覆盖 `crates/agentdash-agent/tests/dash_service.rs:1043-1099`。

但自动 compaction 活动期间：

- history 的 `active_turn` 已因 A failed 清空；
- `active_compaction` 为 B；
- repository root 的 `active` 仍保留原请求，直到 C 或整个请求 terminal。

因此 read snapshot 会呈现“没有 active Turn”，而真正 owner 仍拒绝新的 Submit。这是 control-plane
和 admission 的明确分叉。

## Confirmed Findings

### P0 — 摘要输入丢弃 ToolCall / ToolResult，可能造成真实模型上下文不可逆丢失

production compactor 先调用 `effective_conversation`，但该函数只把以下 history payload 转为摘要
消息：

- `InputAccepted`
- `AgentOutput`

其它 payload 全部忽略，包括 `ToolCall`、`ToolResult`、interaction 和 structured tool outcome：
`crates/agentdash-integration-native-agent/src/bridge_execution.rs:297-362`。

随后 compactor 只把这组消息的 prefix 发送给总结模型：
`crates/agentdash-integration-native-agent/src/bridge_execution.rs:171-237`。

代码中的 prompt 明确要求 “Preserve ... tool outcomes”，但 provider request 根本没有收到工具
调用或结果：`crates/agentdash-integration-native-agent/src/bridge_execution.rs:206-218`。

成功后，后续 provider context 只恢复：

```text
CompactionSummary ContextFrame
+ 从 retained_from 开始的 history suffix
```

证据：`crates/agentdash-agent/src/dash/service.rs:2070-2191`。

所以只要某个重要 ToolResult 位于 `retained_from` 之前，它会同时满足：

- 没进入 summary provider request；
- 没进入 retained suffix；
- 后续模型不可见。

这不是 UI 可观测性问题，而是 compaction correctness 问题。必须先修复，再谈前端完整展示。

### P1 — Context projection 完全忽略 retained suffix，用户视图与 provider input 不一致

`CompactionApplied` durable fact 已保存 `retained_from: Option<HistoryEntryId>`：
`crates/agentdash-agent/src/dash/history.rs:191-198`。provider materializer 会定位该 entry，并把
`entries[history_start..]` 重新变为 messages：
`crates/agentdash-agent/src/dash/service.rs:2081-2116`。

但 `project_managed_runtime_context` 只寻找最后一个
`ItemCompleted(ContextCompaction)`，然后把它之前的全部 message records 排除：
`crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:52-92`。

该 projector 从未读取 `retained_from`，因为 `ManagedRuntimeSnapshot` 的 canonical records 里也没有
这个 checkpoint coordinate。结果：

- provider 实际仍看到 retained user/assistant/tool tail；
- popup 的 `segments`、message breakdown、top tools 和 token estimate 全部不包含这段 tail；
- 用户看到的“当前上下文”必然偏小；
- retained tool activity 对用户完全隐藏。

当前单测反而把错误语义固化为“compaction 之前全部消息排除”：
`crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:500-541`。

### P1 — 失败 compaction 被投影成成功 completed，并错误切断 context projection

Native canonical projector 把下面两种 history terminal 合并到同一个事件：

```text
CompactionCompleted
CompactionFailed
  -> ItemCompleted(ContextCompaction)
```

证据：`crates/agentdash-integration-native-agent/src/canonical_projection.rs:197-207`。

`ContextCompaction` ThreadItem 本身没有 status/error 字段。于是 snapshot/read/reconnect 后无法区分：

- 成功；
- clean failure；
- Lost。

更严重的是 context projector 只要看到 `ItemCompleted(ContextCompaction)` 就建立新边界：
`crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:52-68`。因此失败压缩会
让 popup 排除全部既有消息，尽管 Agent model context 根本没有应用新 checkpoint。

内部 owner 其实保存了正确 failure/lost：

- `CompactionFailed { error, lost }`：
  `crates/agentdash-agent/src/dash/history.rs:202-206`
- folded status：
  `crates/agentdash-agent/src/dash/history.rs:960-975`
- command/effect terminal：
  `crates/agentdash-agent/src/dash/store.rs:220-257`

问题发生在 owner state 到 canonical presentation/snapshot 的信息丢失。

### P1 — Active compaction 没进入 AgentSnapshot / AgentObservation / command availability

Dash folded state明确保存 `active_turn` 与 `active_compaction`：
`crates/agentdash-agent/src/dash/history.rs:531-548`。`ensure_idle` 也同时检查二者，保证 source 内不会
同时开始 normal Turn 和 compaction：
`crates/agentdash-agent/src/dash/history.rs:1019-1046`。

但公共 `AgentSnapshot` 只有 lifecycle、interactions、surface、initial context 和 canonical
conversation history，没有 active activity/compaction：
`crates/agentdash-agent-service-api/src/snapshot.rs:97-120`。

Native `AgentObservation` 只投影 `state.active_turn`：
`crates/agentdash-integration-native-agent/src/service.rs:143-197`。

Runtime availability 再只调用 `CanonicalConversationView.active_turn()`：
`crates/agentdash-agent-runtime/src/agent_snapshot_projection.rs:119-132`。而
`CanonicalConversationView.active_turn()` 只识别 `TurnStarted/TurnCompleted`，不识别 active
ContextCompaction item：
`crates/agentdash-agent-protocol/src/presentation.rs:80-94`。

因此 active compaction 期间 mapper 会把：

- SubmitInput
- RequestCompaction
- Fork

全部标记 available：
`crates/agentdash-agent-runtime/src/agent_snapshot_projection.rs:202-249`。

实际 owner 仍会通过 history `ensure_idle` 或 repository `active` 拒绝这些请求，所以不会并发破坏
history，但前端 authority 是错误的，用户会遇到按钮可点、请求 conflict。

### P1 — 自动压缩期间 Steer/deferred input 的规范合同没有实现

规范要求自动压缩期间的 Steer durable 保留，并由 continuation turn 继承消费：
`.trellis/spec/backend/agent-runtime-native-adapter.md:168-176`、
`.trellis/spec/backend/agent-runtime-native-adapter.md:250-258`。

当前 Product facade 根据 snapshot active Turn 决定 Submit 是普通 Submit 还是 Steer：
`crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs:209-233`、
`crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs:291-318`。

自动 compaction B 期间 snapshot 没有 active Turn，所以新输入被映射为 `SubmitInput`。Dash
`execute_submit` 看到 repository root 仍有 active execution，直接返回 conflict：
`crates/agentdash-agent/src/dash/service.rs:1015-1021`。

底层 queue 确实有 command dependency 和 deferred promotion 能力，但 public service 没有把压缩期间
输入接到这条 queue。`manual_compaction_defers_new_input...` 只测试直接操作 store：
`crates/agentdash-agent/tests/dash_history.rs:417-465`，不能证明 Product/API 行为。

### P1 — `CompactionStarted` 后进程退出会永久卡住 source

Native manual compaction 的 compactor future 是当前 HTTP/service future，不是 durable worker。
`CompactionStarted` 先通过 source document CAS 持久化，然后才 await compactor：
`crates/agentdash-agent/src/dash/service.rs:1841-1884`。

若进程在 terminal commit 前退出：

- source document 保留 active lifecycle command；
- folded state 保留 `active_compaction`；
- inner effect receipt 仍是 Accepted；
- 没有 cancellation token、work item 或 recovery worker 可重新运行 compactor。

重启后相同 effect 重试时，`DashAgentService::execute` 会直接返回已保存的 Accepted receipt：
`crates/agentdash-agent/src/dash/service.rs:880-898`。Complete Agent 随后只会启动
`wait_for_effect_terminal` settlement：
`crates/agentdash-integration-native-agent/src/service.rs:774-856`，而 inner effect 已无人推进，
因此永久等待。

这违反：

- process restart 从 owner `read/inspect` 重建的合同：
  `.trellis/spec/backend/agent-runtime-persistence.md:85-88`
- source-owned worker 不得留下永久 active 的合同：
  `.trellis/spec/backend/agent-runtime-native-adapter.md:257-258`。

### P1 — Compaction effect 有双 durable record 与非原子 gap

Dash source JSON document 内已经保存：

- lifecycle command/effect；
- history；
- active compaction；
- compaction terminal。

证据：`crates/agentdash-agent/src/dash/service.rs:98-103`、
`crates/agentdash-agent/src/dash/store.rs:71-76`。

但 Complete Agent 又把同一 execute effect 保存到独立 `dash_complete_effect`：
`crates/agentdash-integration-native-agent/src/service.rs:823-856`。

这两个事实不是一次数据库 transaction：

1. `service.execute_admitted()` 先通过 source repository CAS 更新 `dash_complete_source`；
2. 返回后 Complete Agent 再独立 commit outer effect。

PostgreSQL outer commit 虽能原子更新它收到的 effect/source mutations：
`crates/agentdash-infrastructure/src/persistence/postgres/dash_complete_agent_store.rs:214-326`，
但 execute path 的 source mutation早已通过另一 CAS 完成。

已有 `reconcile_accepted_command_record` 可以把 outer Accepted 收敛到 inner terminal：
`crates/agentdash-integration-native-agent/src/service.rs:498-558`，这保障“inner 已 terminal、outer 仍
Accepted”的恢复；它不能恢复“inner active compaction 的执行 future 已丢失”。

### P1 — Manual compaction 没有真正 queue、cancel 或 interrupt 语义

规范声明：

- normal Turn active 时 manual compaction durable queued；
- compaction active 时新输入 deferred。

当前 `begin_compaction` 会 enqueue 后立即要求自己被 promote；若前面已有 active command则返回
`CommandNotPromoted`：
`crates/agentdash-agent/src/dash/store.rs:125-163`。因为 mutation closure 整体失败，queue 不会落地。

Native compactor trait没有 CancellationToken，manual `execute_compaction` 也不注册 cancellation
handle。Interrupt 只接受 `repository.active.turn_id` 对应的 normal Turn：
`crates/agentdash-agent/src/dash/service.rs:1739-1833`、
`crates/agentdash-agent/src/dash/service.rs:2289-2312`。

Close 在 active compaction 时又会被 history `ensure_idle` 拒绝：
`crates/agentdash-agent/src/dash/history.rs:1011-1024`。

所以 Native manual compaction 的真实语义是：

- active Turn 时直接拒绝，不 queue；
- compaction 开始后不可 cancel；
- 新输入不 deferred；
- Close 也不可执行。

这是可以接受的产品选择之一，但必须显式建模并由 snapshot availability 暴露；当前既与 spec
不一致，也没有 typed compaction-specific unavailable reason。

### P1 — Summary ContextFrame 只有文本，没有 structured checkpoint provenance

协议已经定义 `ContextFrameSection::CompactionSummary`，可携带：

- `tokens_before`
- `messages_compacted`
- `compaction_id`
- `projection_version`
- `strategy/trigger/phase`
- source range
- first kept coordinate
- compacted-until ref

证据：`crates/agentdash-agent-protocol/src/backbone/context_frame.rs:530-558`。

但当前 `accepted_compaction_summary_frame` 实际创建的是普通 `SystemNotice`：
`crates/agentdash-agent/src/dash/history.rs:224-268`。

它只用：

- frame id；
- cache key = compaction id；
- cache revision = context revision；
- `<compacted_context>summary</compacted_context>` rendered text。

没有 `retained_from`、source head/digest、message/tool membership、tokens before/after 或 terminal
evidence。因此：

- provider 与用户看到的 summary 文本是同一份，这是已有保障；
- 但 ContextFrame 无法证明完整 context recipe；
- Product projection 也无法从 canonical records 无损恢复 retained suffix。

### P2 — `active_compaction_id` 实际是“最后一个 completed item id”

`SessionProjectionViewResponse.active_compaction_id` 由最后一个
`ItemCompleted(ContextCompaction)` 得到：
`crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:52-68`、
`crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs:214-221`。

这不是 active：

- running compaction只有 ItemStarted，字段为 null 或仍指向上一次；
- success 后字段长期保留；
- failure 因错误映射也会成为该字段。

字段名和语义都应收敛为明确的：

- `active_compaction`：仅非终态；
- `context_checkpoint` / `applied_compaction_id`：当前已应用 checkpoint。

### P2 — 压缩完成后 provider usage 不会立即更新，缺少“已过期/待确认”语义

provider token usage 只在正常 provider round 的 `ProviderUsageConfirmed` 中提交：
`crates/agentdash-agent/src/dash/history.rs:811-833`。manual compaction 使用独立 summarizer provider，
不会产生主 Agent 的新 `TokenUsageUpdated`。

Compaction result也只有：

- revision；
- summary；
- retained_from。

证据：`crates/agentdash-integration-native-agent/src/bridge_execution.rs:227-237`。

因此压缩完成后：

- last provider-confirmed context usage 仍是压缩前值；
- popup 的 projection estimate 会尝试刷新，但当前又漏算 retained tail；
- 后端没有 `usage_revision`、`stale_until_next_provider_round` 或估算后的 current context size。

不能用字符估算冒充 provider confirmed usage，但应显式返回“最后确认值属于旧 context revision”，并
单独提供新 checkpoint 的 projection estimate。

### P2 — Compaction terminal 不是 authoritative snapshot reload boundary

Native durable live 有正确的基本保障：

- source history commit 后才 publish：
  `crates/agentdash-agent/src/dash/service.rs:2372-2430`
- live 与 snapshot 共用 canonical projector：
  `crates/agentdash-integration-native-agent/src/service.rs:1981-2009`
- lag 返回 typed retryable error并要求 reload：
  `crates/agentdash-integration-native-agent/src/service.rs:2144-2164`

但 HTTP live endpoint没有 baseline revision/cursor/after 参数，只订阅 process-local broadcast：
`crates/agentdash-api/src/routes/lifecycle_agents.rs:658-702`、
`crates/agentdash-application-agentrun/src/agent_run/product_projection_gateway.rs:284-299`。

当前 source live sequence 又只在进程内递增：
`crates/agentdash-integration-native-agent/src/service.rs:1868-1930`。

客户端先开 transport、再读 snapshot并缓冲 live，能缩小初次 race；但 snapshot read 与服务器完成
broadcast subscribe 之间仍没有原子 cursor handoff。重连时重读 snapshot可以恢复 durable record，
这是已有保障；正在运行的 compaction state仍无法恢复，因为 snapshot没有 active compaction。

另一个问题是前端只有 `TurnCompleted` 才触发 authoritative snapshot reload，ContextCompaction
ItemCompleted 不触发。live reducer只追加 conversation record，不重算
`command_availability/revision`。后端若继续把 command availability放在 snapshot中，就需要把
compaction terminal纳入 authoritative reload/invalidation 边界。

### P2 — Codex compaction 的 operation 与完整 context evidence 仍是 opaque

Codex adapter 正确调用 native `thread/compact/start`：
`crates/agentdash-integration-codex/src/complete_agent.rs:1019-1074`，并把返回 receipt记为
Accepted：`crates/agentdash-integration-codex/src/complete_agent.rs:1140-1170`。

但该 in-process effect ledger没有用 compaction item/turn terminal回写具体 command effect；`inspect`
只返回当前 ledger：
`crates/agentdash-integration-codex/src/complete_agent.rs:1285-1320`。进程重启后未知 effect明确返回
Unknown，这是 adapter fidelity 限制，不是 durable recovery。

Codex canonical mapper同时接纳：

- v2 `ItemStarted/ItemCompleted(ContextCompaction)`；
- deprecated `thread/compacted` telemetry。

证据：`crates/agentdash-integration-codex/src/canonical_projection.rs:21-41`、
`crates/agentdash-integration-codex/src/canonical_projection.rs:109-112`。

`thread/read` snapshot 能恢复 compaction item/turn lifecycle，但没有 AgentDash
CompactionSummary ContextFrame、retained recipe 或 source-native summary 文本：
`crates/agentdash-integration-codex/src/canonical_projection.rs:195-309`。

因此跨 Agent 的“完整上下文视图”必须尊重 fidelity：

- Dash 可提供 Exact recipe；
- Codex 若 native API不暴露 replacement context，只能明确标记 Observed/Opaque；
- 不能仅凭一个 ContextCompaction item伪造“此前全部消息均被替换”的 exact projection。

## Existing Guarantees

以下部分已经正确，不应在修复时推倒：

1. **唯一 source identity**
   - Product 先校验 `run_id + agent_id` binding，再 resolve Complete Agent：
     `crates/agentdash-application-agentrun/src/agent_run/product_projection_gateway.rs:156-190`。
   - Product command facade 同样校验 binding target 与返回 source：
     `crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs:156-190`。
   - PostgreSQL decode/commit 校验 source coordinate 与 history session id一致：
     `crates/agentdash-infrastructure/src/persistence/postgres/dash_complete_agent_store.rs:536-548`。

2. **history provenance fencing**
   - `CompactionStarted` 固定 source head/digest；
   - replay 在 commit 前校验 digest；
   - `CompactionApplied` 再校验同一 source digest与未应用状态。
   - 证据：`crates/agentdash-agent/src/dash/history.rs:583-614`、
     `crates/agentdash-agent/src/dash/history.rs:895-945`。

3. **source 内单一活动**
   - `ensure_idle` 同时阻止 active Turn 与 active compaction重叠：
     `crates/agentdash-agent/src/dash/history.rs:1019-1024`。
   - 即使 snapshot availability错误，history CAS仍防止真实并发污染。

4. **成功/失败 owner commit**
   - success 的 Applied + Completed + command/effect terminal在一个 staged store commit中：
     `crates/agentdash-agent/src/dash/store.rs:167-217`。
   - failure/lost 的 state + command/effect terminal同样一次 commit：
     `crates/agentdash-agent/src/dash/store.rs:220-257`。

5. **summary 文本单一权威**
   - `CompactionApplied` 保存最终 ContextFrame；
   - provider round通过该 frame `rendered_text`重建 prompt；
   - canonical projector发布同一个 frame。
   - 证据：`crates/agentdash-agent/src/dash/service.rs:2079-2219`、
     `crates/agentdash-agent/src/dash/service.rs:2505-2565`、
     `crates/agentdash-integration-native-agent/src/canonical_projection.rs:184-196`。

6. **durable read/changes/live 同源**
   - read从 source document重建完整 canonical history：
     `crates/agentdash-integration-native-agent/src/service.rs:860-908`。
   - changes从持久 Dash changes投影：
     `crates/agentdash-integration-native-agent/src/service.rs:920-984`。
   - committed suffix live也调用相同 entry projector：
     `crates/agentdash-integration-native-agent/src/service.rs:1981-2009`。

## Race And Recovery Matrix

| 时点 / 竞态 | 当前结果 | 判断 |
| --- | --- | --- |
| active normal Turn 时请求 manual compaction | `begin_compaction`不能 promote，整个 mutation失败；没有 durable queue | 确认缺陷，违反 spec |
| manual compaction started 后提交新输入 | snapshot显示可提交；history `ensure_idle`最终拒绝 | 数据安全已有保障，control-plane错误 |
| automatic B 期间提交新输入 | snapshot无active Turn，映射普通Submit；repository root active导致conflict | 确认缺陷，未实现deferred/Steer |
| 同时请求两个 compaction | history `ensure_idle` / lifecycle active阻止第二个 | 已有保障 |
| source CAS发生并发变化 | compare-and-swap冲突，无部分 staged history | 已有保障 |
| crash发生在 CompactionStarted 之前 | 没有 compaction fact，可用同identity重试 | 已有保障 |
| crash发生在 Started 之后、compactor返回之前 | active compaction与Accepted inner effect永久保留，无worker恢复 | Critical recovery gap |
| crash发生在 inner terminal 后、outer effect commit前 | 同identity重试可读inner terminal，再写outer receipt | 部分已有保障 |
| history commit成功、live publish失败 | read/changes仍可恢复，commit不回滚 | 已有保障 |
| live lag超过1024 | stream返回retryable unavailable并断开 | 已有保障，但依赖客户端read |
| snapshot read完成、live subscribe尚未真正建立时发生compaction | endpoint无cursor handoff，started/terminal可能漏live；重连read恢复records | 终态可恢复，进行中authority仍缺失 |
| compaction clean failure后重连 | read显示ItemCompleted，context projection错误切边界 | 确认缺陷 |
| compaction Lost后重连 | inner state Lost，但canonical与Failed/Success不可区分 | 确认缺陷 |
| automatic B failure | C Failed；B Lost时C Blocked且consistency Lost | 已有保障 |

## Owner Invariant Assessment

### 正确边界

```text
AgentRun target
  -> LifecycleAgent.runtime_binding
  -> Complete Agent service + source coordinate
  -> Dash/Codex owner read/execute/inspect
```

这条 owner route整体正确。Runtime snapshot/context projection都是 request-scoped projection，没有
另建 Runtime conversation/context 数据库。

### 需要收敛的 owner drift

Native execute effect实际存在两份 durable state：

```text
dash_complete_source.repository.effects[effect_id]
dash_complete_effect[effect_id]
```

前者决定真实 Dash command/compaction lifecycle，后者决定 Complete Agent `inspect`。当前靠异步
reconciliation 收敛，而不是同一 owner transaction。对于普通短命令尚可恢复；对于 active
compaction，这个 gap会暴露永久 Accepted。

推荐让 Complete Agent `inspect(effect_id)` 直接以 source-owned effect为权威，或让 source mutation与
Complete receipt在同一 `DashCompleteAtomicCommit` 中提交。项目未上线，不应继续保留双账本兼容。

## Recommended Target Contract

### 1. 将 Compaction 建模为 Agent-owned 正式 Turn

不要把执行态塞进`AgentLifecycleStatus`，也不要建立平行activity owner。Compaction复用正式Turn：

```text
ActiveTurnSnapshot {
  turn_id
  kind: normal | context_compaction
  phase
  interaction?
  compaction? {
    compaction_id
    mode
    base_context_revision
    applied_context_revision?
  }
  cancellable
  source_revision
}
```

`AgentSnapshot`、`AgentObservation`、changes/live invalidation和Runtime availability都消费该同一
Turn state。Canonical `ContextCompaction` item属于该Turn并负责feed presentation，不单独充当
command authority。

### 2. 明确 command matrix

若采用当前 spec 的 deferred语义：

| Active activity | Submit | Steer | Interrupt | Compact | Fork | Close |
| --- | --- | --- | --- | --- | --- | --- |
| Idle | start Turn | unavailable | unavailable | start/queue compaction | available | available |
| Normal Turn | durable Steer | durable Steer | available | durable queue | unavailable | policy-defined |
| Compaction queued | durable deferred input | unavailable | cancel queue | duplicate/conflict | unavailable | policy-defined |
| Compaction running pre-side-effect | durable deferred input | unavailable | cancel compaction | unavailable | unavailable | unavailable |
| Compaction applying/post-side-effect | durable deferred input | unavailable | unavailable | unavailable | unavailable | unavailable |
| Lost | unavailable | unavailable | unavailable | unavailable | unavailable | recovery only |

若产品决定不接受 deferred input，也应在 snapshot明确 unavailable，不能显示可用后由 history conflict。

### 3. 建立 exact CompactionCheckpoint

Dash owner需要一个可投影的 typed checkpoint：

```text
CompactionCheckpoint {
  compaction_id
  mode
  terminal
  source_head / source_digest / source_revision
  applied_context_revision
  summary_frame
  compacted_range
  retained_from
  retained_record_ids
  included_tool_pair_ids
  usage_before
  usage_after_estimate
  usage_confirmation_status
}
```

ContextFrame使用 structured `CompactionSummary` section表达 checkpoint provenance和可读summary；
retained payload仍由canonical history拥有。`SessionProjectionViewResponse`按 checkpoint membership
选择 retained segments，而不是按 item completed的位置切一刀。

### 4. summary 输入复用原 Session 的精确 context materialization

Compaction在原Session上启动正式Turn。它复用正常Turn的精确provider context prefix，并只在末尾
追加一次synthetic compaction instruction：

```text
active ContextFrames
+ previous summary frame
+ exact current conversation prefix
+ complete tool call/result pairs
+ synthetic compaction instruction
```

本轮不开启新工具调用；历史tool pair仍属于prefix。provider output只作为checkpoint candidate，
synthetic instruction与candidate不写成普通conversation records。成功安装checkpoint后，同一
materializer继续驱动：

- checkpoint digest；
- post-compaction provider request；
- canonical context projection；
- frontend context details。

这样可以用纵向测试证明Compaction request没有第二套有损transcript，并证明“被summary覆盖或被
retained保留”，不会出现第三类隐藏丢失。

### 5. terminal presentation 必须 typed

至少需要：

```text
ContextCompactionStarted
ContextCompactionSucceeded
ContextCompactionFailed { code, message, retryable }
ContextCompactionLost { reason }
ContextCompactionCancelled
```

可以表现为 AgentDash typed item extension，或 ContextCompaction item lifecycle + typed
Platform terminal evidence；不能再把 Failed/Lost统一映射为 `ItemCompleted`。

### 6. recovery

compaction必须满足二选一：

- durable work item + claim/lease + source revision fence，重启可重新执行；
- source open/inspect发现 active compaction后，基于稳定 compaction identity重启幂等 compactor。

进入真正 provider side effect前可 Cancelled；side effect outcome unknown时必须 Lost并阻止 continuation，
不能清成 idle。

## Suggested Verification Matrix

### A. Native manual

1. Idle触发：
   - HTTP立即返回 Accepted；
   - snapshot activity = Compaction/running；
   - live先出现 started；
   - Submit/Compact/Fork/Close availability符合策略。
2. active Turn触发：
   - durable queued，或typed unavailable；不能短暂 enqueue后回滚成通用 conflict。
3. success：
   - exact summary frame；
   - checkpoint retained refs；
   - canonical succeeded terminal；
   - snapshot activity回到Idle；
   - context projection与下一次provider request逐项一致。
4. clean failure：
   - 旧context checkpoint保持active；
   - canonical failed；
   - context projection不移动边界；
   - command activity回到Idle。
5. Lost：
   - source consistency Lost；
   - 不promote deferred input；
   - read/inspect/reconnect仍显示Lost。
6. cancel：
   - queued/pre-effect可取消；
   - post-effect不伪造Cancelled。

### B. Automatic overflow A/B/C

1. A overflow -> B started -> B success -> C started顺序唯一。
2. B running期间新输入：
   - durable accepted；
   - 不创建并行Turn；
   - C在安全边界消费。
3. B clean failure：
   - C exactly-once Failed；
   - 原请求terminal；
   - 旧context继续有效。
4. B Lost：
   - C Blocked；
   - 后续Submit拒绝；
   - restart后仍一致。

### C. Context fidelity

构造至少包含：

- user message；
- assistant text；
- command tool call/result；
- MCP tool call/result；
- tool error；
- reasoning；
- interaction resolution；
- attachment/structured input；
- 多轮历史与前一轮summary。

断言每个事实恰好属于：

- summary input；
- retained suffix；
- explicit excluded/audit-only。

不允许“既未summary、也未retain”。

### D. Projection / ContextFrame

1. structured CompactionSummary section包含checkpoint坐标。
2. popup segments = 实际provider message recipe。
3. retained tool pairs完整且顺序合法。
4. failed/cancelled/lost不改变applied checkpoint。
5. `active_compaction`与`applied_compaction_id`语义分离。
6. token estimate包含summary + retained messages/tools + stable ContextFrames。
7. provider-confirmed usage带context revision；compaction后旧usage标记stale，下一provider round确认新值。

### E. Crash injection

在以下位置逐点杀进程并用全新service instance恢复：

1. started commit前；
2. started commit后、summary provider call前；
3. provider request发出后、response前；
4. summary返回后、Applied commit前；
5. inner terminal后、outer receipt前；
6. outer terminal后、live publish前。

每个点断言：

- effect inspect；
- activity；
- command availability；
- provider side effect调用次数；
- context checkpoint；
- deferred input状态；
- read/changes/live/reconnect。

### F. Live / replay / reconnect

1. snapshot-before-subscribe race；
2. subscribe-before-snapshot buffering；
3. 1024以上事件触发Lagged；
4. compaction started时断线；
5. applied/terminal时断线；
6. server process restart导致live sequence重置；
7. durable changes cursor回放；
8. reconnect snapshot与live重复presentation id去重。

### G. Codex / external Agent

1. native compact command receipt与item/turn terminal对应。
2. failed compact不会成为成功context boundary。
3. adapter restart后effect明确Unknown/Observed，不伪造Applied。
4. `thread/compacted`只作deprecated telemetry，不覆盖v2 item lifecycle。
5. native API不暴露summary/retained recipe时，context projection标记Opaque，而不是Exact空summary。

### H. Owner / binding

1. Product target绑定错误source必须拒绝。
2. stale binding generation必须拒绝command。
3. source coordinate与Dash history session id必须一致。
4. 删除任一双effect账本后composition test仍能完成 create/execute/inspect/restart。
5. Complete Agent source切换或Host rebind不能复活旧generation的active compaction。

## Severity Summary

| Severity | Finding |
| --- | --- |
| P0 | compactor summary输入忽略ToolCall/ToolResult，且被cut掉的工具事实会真实丢失 |
| P1 | context projection不读取retained suffix，与provider input不一致 |
| P1 | Failed/Lost映射为ItemCompleted并错误建立成功边界 |
| P1 | active compaction不进入snapshot/activity/availability |
| P1 | 自动压缩期间Steer/deferred input合同未实现 |
| P1 | Started后进程退出无法恢复，source永久active |
| P1 | inner/outer effect双durable state与非原子gap |
| P1 | manual compaction无queue/cancel/deferred语义 |
| P1 | Compaction ContextFrame缺少structured checkpoint/provenance |
| P2 | `active_compaction_id`实际表示最后completed item |
| P2 | 压缩后usage没有revision/stale语义，且projection estimate漏算tail |
| P2 | compaction terminal不触发authoritative snapshot reload |
| P2 | Codex compact effect与context recipe只能Observed/Opaque，当前投影未表达fidelity |

## Caveats / Not Found

- 本研究为源码静态审计，没有启动 `pnpm dev`，也没有运行真实 LLM compaction 或浏览器 E2E。
- 没有执行 PostgreSQL crash-injection；重启卡死结论来自 durable transition与恢复入口的完整代码路径。
- Codex App Server 是否能在未来版本提供完整 post-compaction replacement context，需要以当前 pinned
  schema/`thread/read`实际响应再做一次协议 fixture验证；当前代码没有消费这类 evidence。
- 旧 `agentdash-agent/src/agent.rs` 中仍有另一套 `ContextCompactionStarted/Noop/Failed` event，
  production Native Complete Agent 当前走 `dash/service.rs`，本报告没有把旧路径当成当前权威。
- 07-20 persistence authority 与 07-23 ContextFrame authority 任务仍在进行中；实现计划需要与它们
  协调 shared hotspot，但不应因此保留本报告识别的兼容双账本。
