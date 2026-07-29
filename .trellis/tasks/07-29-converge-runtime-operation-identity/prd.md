# 精简 Agent 执行与 Compaction 持久化模型

## Goal

从第一性原理删除没有独立 owner、恢复职责或生产消费者的身份和持久化副本，只保留：

- Agent service 的 public effect，作为跨层提交、幂等、inspect 与回执的唯一执行句柄；
- Dash command，作为内部队列、依赖和恢复身份；
- compaction history，作为上下文变换与故障恢复事实；
- 正式 repository schema migration，确保现有会话可被最终严格结构读取。

目标不是给每层补齐 `operation_id`，而是消除导致同一执行出现多条路径的模型。

## Confirmed Problems

- Product facade 同时生成 `product-command:v2:*` Runtime operation 和
  `product-effect:v2:*` Agent effect，但只有后者有持久化 owner 与 inspect 语义。
- Agent execution snapshots、Runtime `operations` 集合、Native compaction item 的
  `operation_id` 没有生产消费者；Runtime operations 集合始终为空。
- Dash 为 automatic compaction B/C 创建 child effect 并维护 lifecycle-local effect ledger，
  但 B/C 的真实恢复关系已经由 durable command、dependency、history 和 active lease 表达。
- `CompactionCheckpoint` 大部分字段只在写入和测试中出现；生产只消费
  `context_revision`、`summary_frame` 和 `retained_from`，其余为可重算或重复证据。
- `DashAgentStore.changes` 完整复制 history；额外的 `ActiveTurnChanged` 在 Native adapter 中被
  丢弃，形成双份持久化、双份 digest 和漂移风险。
- Product `Rebind` command 返回无 effect 的成功回执；真实 rebind 属于 Host/Product convergence。
- 历史 schema 曾新增必填 `operation_id`、side-effect state，并把平铺 Applied 改为 checkpoint，
  却没有数据 migration，导致 repository 严格解码时整个会话读取失败。

## Requirements

### R1. 单一跨层执行句柄

- Product command receipt 直接使用 Complete Agent public `effect_id` 的同值
  `RuntimeOperationId`。
- 保留 `AgentRuntimeOperationReceipt.operation_id`，因为 Routine、Channel、Gate 与 saga 会持久化
  并消费该回执。
- 删除 Agent execution snapshot、blocking evidence、terminal outcome、Runtime view operations
  和 Native compaction item 中重复的 operation identity。
- queued compaction snapshot 只保留展示所需的状态/时间，不发布无人消费的内部 command ID。
- 不新增 Agent operation aggregate、operation repository、root/child effect relation或字符串映射。

### R2. Dash 内部只建模命令与事实

- automatic A/B/C 使用 durable command ID 与 dependency 表达内部执行链。
- 删除 Dash lifecycle-local effect ledger、effect settlement 与 B/C child effect ID。
- 删除无人消费的 `DashEffectInspection.execution` 派生块；public effect inspection 只保留
  command/effect identity、receipt state 与 retryability。
- 保留 Complete Agent source repository 的 public effect records；它承担 duplicate inspection、
  wait/inspect、terminalize 和 worker recovery，具有真实 owner 职责。
- `command_id` 表示内部 intent/queue；`effect_id` 表示外部可 inspect effect；
  `idempotency_key` 只参与 retry-equivalent request fingerprint。

### R3. 最小 compaction history

- `CompactionQueued` / `CompactionStarted` 不保存 operation/effect ID。
- `CompactionStarted` 保留 `source_head` 与 `source_digest`，用于锁定被压缩的精确历史快照。
- 删除 `CompactionCheckpoint`，将 `CompactionApplied` 收束为生产真正消费的平铺事实：
  `compaction_id`、`context_revision`、`summary_frame`、`retained_from`。
- `context_revision` 由 compaction ID、Started 的 source digest、summary frame 内容和
  retained boundary 确定性计算并在 fold 中校验，不在 Applied 重复保存 source digest。
- Applied 是副作用已完成的更强终态证据；历史 fold 不要求终态前必须存在
  `CompactionSideEffectStarted`。该事件只服务 active crash recovery，不为旧历史合成伪造事件。

### R4. 单一 change source

- 删除 `DashAgentStore.changes`、`DashAgentChangePayload::ActiveTurnChanged` 和 ordinal cursor。
- change API 按 history revision 直接投影 history entry；active state 由 snapshot/history fold
  得出，不另建 change 事实。
- change cursor 只表达 history revision，不复制 history payload 和 digest 到 repository。

### R5. Persistence migration

- 新增明确的 repository schema version 和启动期 typed migration。
- migration 将所有旧 repository 归一化到最终结构：移除 execution operation 字段、内部 effect
  ledger、changes 镜像和 checkpoint 冗余字段，平铺 Applied 并重算受序列化影响的 history digest。
- migration 定点更新 Product owner 已持久化的旧 Runtime receipt evidence：
  `product-command:v2:*` → 同 suffix 的 `product-effect:v2:*`。
- 不添加 `serde(default)`、旧 variant 双读、运行时 fallback 或兼容别名。
- 无法证明 active side effect 是否发生的旧记录必须显式迁移失败；不猜测 terminal。

### R6. 误导性内容收束

- 删除无真实 effect 的 Product `Rebind` command/API route；保留真实 Runtime lifecycle Rebind。
- 更新 Trellis specs、领域词汇、合同、生成类型、fixture 与注释，只描述最终模型及其原因。
- 清除把内部 effect 称为 operation、把派生镜像当 source of truth 的 helper 和断言。

## Acceptance Criteria

- [x] 一次 Product Agent command 只有一个跨层可寻址执行句柄：Complete Agent public effect。
- [x] `AgentRuntimeOperationReceipt.operation_id` 与 submitted public effect 同值，duplicate/reload
      后不变化。
- [x] Agent execution snapshots、Runtime view、canonical context compaction item 不再携带
      `operation_id`；Runtime view 不再包含恒空的 `operations` 集合。
- [x] Dash automatic A/B/C 只通过 command/dependency/history/lease 恢复，不存在 B/C child effect
      或 lifecycle-local effect ledger。
- [x] `CompactionCheckpoint` 被删除，Applied 只保存最小 canonical fact，并可校验
      `context_revision` 与 Started source。
- [x] repository 不再持久化 history 的 changes 镜像；change API 直接从 history 投影。
- [x] migration 能升级已知旧 shape，升级后 production strict decoder 可读取，会话不再因缺少
      `operation_id` 或 `checkpoint` 挂掉。
- [x] migration 不合成未知 identity、claim 时间或 side-effect 结果；不可证明的 active 数据
      fail-fast 并报告 source coordinate。
- [x] Product runtime command contract 不再包含无 effect 的 `Rebind`。
- [x] 相关 Rust、migration、contract、frontend 类型和回归测试通过。

## Out of Scope

- 重写 Complete Agent public effect record 与外层 `dash_complete_effect` 的全部持久化架构；二者当前
  分别承担 source-local recovery 与 facade/saga handoff，需另行验证能否安全合并。
- 修改 AgentRun 之外的 Workflow effect 模型。
- 建立全局 operation database 或通用 tracing 系统。
- 猜测无法由现有 durable facts 确定的历史执行结果。
