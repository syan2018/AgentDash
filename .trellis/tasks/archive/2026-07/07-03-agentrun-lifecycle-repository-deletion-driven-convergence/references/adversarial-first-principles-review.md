# First-Principles Adversarial Review

本文件汇总 6 个只读 subagent 对当前仓库的对抗式审查结果。审查时没有读取 `.trellis/tasks/`，只基于代码、schema、migration、contracts、API 和前端现状。

## Irreducible Facts

- `AgentRun` 是用户可见工作区和单 Agent 会话身份，产品命令必须以 `run_id + agent_id` 进入。
- `LifecycleRun` 是控制面 aggregate，可编排多个 AgentRun；它不应吞掉 RuntimeSession 或 mailbox 的事实语义。
- `LifecycleAgent` 是 run 内 Agent 身份，不应保存 live runtime 指针。
- `AgentFrame` 是能力、上下文、VFS、MCP、executor config 的基准 surface revision；历史 revision 不应被原地追加 runtime visibility。
- `MailboxEnvelope` 是 AgentRun durable 用户意图和队列状态，不属于某个 RuntimeSession。
- `CommandReceipt` 是用户/API 命令的幂等、重放、冲突和 stale guard 事实源。
- `RuntimeSession` 是 internal delivery/trace substrate；它只记录 event stream、turn/tool 边界、context projection、terminal/runtime metadata、executor state。
- `RuntimeSessionExecutionAnchor` 是 immutable launch evidence / reverse index，不是 current selection policy。
- `TurnAccepted` / `TurnTerminal` 才是执行事实；Lifecycle node started/terminal 不能由 runtime allocation 推断。
- Projection、workspace snapshot、session `last_*`、context projection、trace refs 只能是可重建 read model 或 diagnostic evidence。

## P0 Findings

### RuntimeSession 仍是产品写控制面

Raw `/sessions/{id}` 暴露 fork、rollback、delete、tool approval、meta patch、runtime control 等写操作，并且权限只通过 session anchor 回查 project use。AgentRun scoped runtime endpoints 多数只是解析 current `runtime_session_id` 后委托 session handler。

应删除：

- raw Session 产品写 API。
- Session fork/rollback/delete/title patch/tool approval 作为用户操作入口。
- AgentRun runtime route 长期返回 Session-shaped DTO 的结构。

替换边界：

- 所有用户写操作都必须进入 AgentRun command service。
- RuntimeSession namespace 只保留 internal/diagnostic trace。

### Mailbox 被 RuntimeSession 拥有

`agent_run_mailbox_messages` 和 mailbox state 强制 `runtime_session_id NOT NULL`，并通过 FK 级联到 `sessions`。Repository claim 也按 runtime session 过滤。这让 durable 用户意图依附 trace substrate，删除或替换 RuntimeSession 会影响 AgentRun queue。

应删除：

- mailbox message/state 的 runtime session ownership。
- runtime-scoped mailbox claim。
- mailbox 对 `sessions` 的 cascade ownership。

替换边界：

- `Mailbox` 以 `run_id + agent_id + message_id` 为 owner。
- RuntimeSession ref 只能作为 nullable delivery attempt / accepted trace evidence。

### RuntimeSession launch accepted 后 AgentRun commit 是 best-effort

RuntimeSession launch commit 先写 session turn/meta，再调用 accepted launch commit；AgentRun frame/current delivery 写入失败只记诊断，甚至存在 noop accepted commit port。这允许 RuntimeSession 已 accepted，但 AgentRun frame/current surface 丢失或滞后。

应删除：

- RuntimeSession accepted 成功而 AgentRun accepted commit 失败仍继续成功的语义。
- accepted launch commit noop / diagnostic-only 作为生产路径。

替换边界：

- `AgentRunTurnAccepted + FrameCommit` 必须成为同一个 accepted boundary。
- RuntimeSession 只能在 AgentRun accepted commit 成功后对外报告 accepted。

### ProjectAgent start 是跨聚合散落 saga

Start 当前按 receipt claim、lifecycle/runtime materialize、ProjectAgent bind、initial mailbox、receipt accepted/result 多步写入，API 层还后台调度首条消息。没有一个原子的 `RunAdmitted` 事实。

应删除：

- API 层持有首条消息调度职责。
- start accepted 但 initial delivery / mailbox / frame / runtime half-built 的语义。

替换边界：

- `AgentRunAdmission` 用例原子产出 run、agent、frame、anchor、initial mailbox envelope、outer receipt。

### Fork baseline 由三方共同构造

AgentRun fork 先创建 child RuntimeSession/session lineage/fork projection，再 materialize child AgentRun，并复制 parent frame JSON。child 的 model context baseline、runtime surface baseline、lineage 来源分裂。

应删除：

- RuntimeSession fork 作为 product fork 的第一持久事实。
- `fork_initial_projection` 作为 child baseline 权威事实。
- fork receipt `result_json` 承担 lineage/child refs 事实语义。

替换边界：

- `AgentRunForkRecord` 是唯一 product fork 事实。
- Fork 事务以 AgentRun/AgentFrame/baseline/message boundary 为先，RuntimeSession 只是附属 trace。

## P1 Findings

### `LifecycleAgent.current_delivery_*` 是第二事实源

current delivery 从 anchor 和 `sessions.last_delivery_status` backfill，却作为 `LifecycleAgent` 字段和 delivery selection 的入口。它把派生 runtime pointer 持久化到身份聚合里。

应删除：

- `LifecycleAgentCurrentDeliveryBinding` 作为持久身份字段。
- `current_delivery_*` 列和持久化 `DeliveryBindingStatus`。

替换边界：

- current delivery 应由 explicit delivery attachment / read model 表达，或由 anchor + live runtime state + current frame resolver 推导。

### Anchor 名为 evidence，实际可变

`RuntimeSessionExecutionAnchor` 注释是 launch evidence，但 Postgres `upsert` 可改写 run/agent/frame/node 坐标，并暴露 `latest_updated_anchor_for_agent`。

应删除：

- anchor upsert 改写坐标。
- `latest_updated_anchor_for_agent` 作为业务选择入口。

替换边界：

- anchor insert-once/idempotent create。
- current delivery 另设 explicit attachment/projection。

### Lifecycle NodeStarted 早于真实执行

Lifecycle dispatch/materialization 后直接发 `NodeStarted`。此时可能没有 mailbox dispatch、connector accepted 或首条 turn accepted。

应删除：

- Runtime allocation/materialization 等同 NodeStarted。

替换边界：

- materialization 只能产生 `DeliveryPrepared` / `RuntimeAllocated`。
- `NodeStarted` 必须由 `AgentRunTurnAccepted` 推进。

### Command receipt、Mailbox、RuntimeSession command 三套状态机重叠

一个用户命令同时存在于 receipt、mailbox、runtime command、session meta、event stream。状态真相取决于读哪个投影。

应删除：

- mailbox / runtime command 各自定义业务终态。
- receipt 从 mailbox status 反推 command outcome 的长期语义。

替换边界：

- `AgentRunCommand` / `DeliveryAttempt` process manager。
- mailbox 是 queue item；runtime command 是 outbox/attempt。

### AgentFrame revision 被原地修改，并且内部双源

Frame 被定义为 surface revision，但 repository 可原地 append visible resources。Frame 内部还存在 full capability JSON 与 VFS/MCP JSON 覆盖层，读取时后者覆盖前者。

应删除：

- historical frame revision 的原地 append。
- `effective_capability_json + vfs_surface_json + mcp_surface_json` 的覆盖式双源模型。
- `AgentFrameRepository.get_current(agent_id)` 作为 runtime truth。

替换边界：

- append-only `AgentFrameRevision` / typed `RuntimeSurfaceRevision`。
- `DeliverySurfaceBinding(runtime_session_id, launch_frame_id, current_applied_frame_id, accepted_turn_id)`。

### ContextFrame emission 没有唯一事实源

launch preparation、commit、runtime context transition、compaction success 都可构造 ContextFrame。没有 `ContextDeliveryRecord` 表达实际传给模型的 context。

应删除：

- `context_slice_json` / projection 作为 command-side baseline truth。
- ContextFrame 由多个路径各自构造的语义。

替换边界：

- `ContextDeliveryRecord` 作为 connector input 与 ContextFrame emission 的共同来源。

## P2 Findings

- mailbox move/reorder 是用户写操作，但没有 command receipt / stale guard。
- tool approval 可退回 RuntimeSession route，且 response 仍带 session identity。
- AgentRun DTO/contract 暴露 raw `runtime_session_id`、`RuntimeSessionCommandStateDto`、`turn_id`，把 trace 当业务事实。
- AgentRun workspace header 直接展示和复制 RuntimeSession id。
- AgentRunLineage 保存必填 parent/child runtime session id，混合 product lineage 与 runtime trace lineage。
- `AgentRunLineageRef` 名称被用于同 run control tree，lineage 命名边界混乱。
- `lifecycle_runs.view_projection`、`execution_log`、`context.agent_runs.current_frame_id` 等 JSON 字段承担了不可约事实或反向索引。
- `RepositorySet` / `AgentRunRepositorySet` 已变成跨 bounded-context service locator。

## Planning Corrections

当前重构计划必须从“仓储整理”提升为“事实源重建”。优先级应是：

1. 清掉 RuntimeSession 产品写控制面。
2. 纠正 Mailbox owner。
3. 建立 AgentRun admission 原子边界。
4. 建立 AgentRun turn accepted + frame commit 原子边界。
5. 将 Lifecycle node start/terminal 改为由 accepted/terminal turn 推进。
6. 统一 command lifecycle。
7. 重建 fork baseline 与 lineage。
8. 整理 AgentFrame 为 append-only typed surface revision。
9. 处理 projection rebuildability 和 RepositorySet 泄漏。
