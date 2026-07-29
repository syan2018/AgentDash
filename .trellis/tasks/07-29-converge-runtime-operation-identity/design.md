# Design: 精简 Agent 执行与 Compaction 持久化模型

## 1. Core Decision

执行身份只在有 owner 的边界存在：

- Complete Agent public effect 拥有接纳、幂等、inspect、terminal 与 recovery，因而是事实身份。
- Runtime operation 只是该 effect 穿过调用接口时的 opaque receipt handle，不是独立 aggregate。
- Dash command 拥有内部队列与依赖生命周期，但不是另一条 public effect。
- execution snapshot 与 presentation 是状态投影，不拥有身份，不重复发布 operation handle。

因此删除 root/child effect 设计。automatic compaction B/C 是内部 durable commands，不是需要
单独 inspect/settle 的 effects。

## 2. Final Vocabulary

- **Command**：Dash 内部可排队、依赖、恢复并终结的 durable intent，由 `command_id` 标识。
- **Public Effect**：Complete Agent 对调用方接纳的副作用，由 `effect_id` 标识，并可
  inspect/terminalize。
- **Runtime Operation Receipt**：public effect 通过 Runtime 接口返回时的同值 opaque handle。
- **Idempotency Key**：retry-equivalent request fingerprint 的输入，不是执行句柄。
- **History Fact**：已经发生的 conversation/compaction 领域事件，是 projection 与 recovery 的
  source of truth。

## 3. Identity Flow

```text
Product request
  ├─ AgentCommandId       -> intent / request fingerprint
  ├─ AgentEffectIdentity  -> Complete Agent public effect owner
  └─ IdempotencyKey       -> retry equivalence

AgentEffectIdentity --same opaque value--> RuntimeOperationId receipt
```

Product 不再生成 `stable_product_command_operation_id`。Routine、Channel、Gate 继续保存 receipt，
但保存的是 public effect 的 handle。

Agent snapshot 不再携带 effect/operation ID。需要等待或 inspect 的调用方使用提交时获得的 receipt，
而不是从不断变化的 execution presentation 反向寻找身份。

## 4. Dash Execution Model

### 4.1 Internal chain

automatic overflow 保留 A/B/C command chain：

```text
A SubmitInput command
  -> B RequestCompaction command
  -> C ContinueAfterCompaction command
```

command dependency、status、active command、history 与 lease 已足以恢复和阻断执行。删除：

- `DashLifecycle.effects`
- `EffectOutcome` / `EffectSettlement`
- `DashExecutionInspection` 与 `DashEffectInspection.execution` 整体派生块
- B/C child effect ID 生成与 settlement

Complete Agent source-level `DashEffectRecord` 保留。它是 external request 的 owner，覆盖整个内部
command chain，并在最终 terminal 时收敛。

### 4.2 Compaction history

最终事件形状：

```rust
CompactionQueued {
    compaction_id,
    mode,
    queued_at_ms,
}

CompactionStarted {
    compaction_id,
    mode,
    source_head,
    source_digest,
    started_at_ms,
}

CompactionApplied {
    compaction_id,
    context_revision,
    summary_frame,
    retained_from,
}
```

`CompactionSideEffectStarted` 仍标识 active compaction 已越过副作用边界，用于 crash 后判定 lost。
`CompactionApplied` 已经证明副作用完成，因此 fold 不把 SideEffectStarted 当作 Applied 的必需前置。

删除 `CompactionCheckpoint` 及其重复/无消费者字段。`summary_frame` 是 canonical summary；
`context_revision` 由 Started source 与 Applied canonical content 确定性生成并校验。tool pairing、
retained/compacted entry list、usage 和 timestamp 若未来出现真实需求，应由专门 owner/telemetry
重新建模，而不是塞入 checkpoint。

## 5. Projection and Change Feed

### 5.1 Runtime and canonical items

删除：

- `AgentActiveTurnSnapshot.operation_id`
- `AgentQueuedCompactionSnapshot.operation_id`
- `AgentCompactionOutcomeSnapshot.operation_id`
- `AgentControlAvailabilityEvidence.blocking_operation_id`
- `AgentRuntimeOperation` 与 `AgentRuntimeView.operations`
- Native `ContextCompaction.operation_id`
- queued compaction snapshot 中无人消费的内部 `command_id`

这些字段没有生产决策者，且把 public effect、internal compaction 和 presentation 混成一条路径。
Receipt contract 保留，不影响真正的 inspect/wait/handoff。

`AgentRuntimeOperationStatus` 与 `AgentRuntimeOperationEvidence` 仍由 receipt 使用，不能随
`AgentRuntimeOperation` view record 一起删除；删除的是恒空集合及其 record，不是 receipt 的状态和
evidence 合同。

### 5.2 Changes

`AgentHistory` 是唯一 change source。change API 从指定 revision 后的 history entries 投影：

- cursor = history revision；
- payload = 当前 history entry 的 adapter projection；
- head/source revision 按当前 projection contract 计算，不持久化副本。

删除 persisted `changes`、ordinal 与 `ActiveTurnChanged`。active turn 是 history fold 的状态，不是
第二类 durable event。

## 6. Product Rebind

删除 Product Agent runtime command 的 `Rebind` variant、route mapping 和无 effect 成功回执。真实
surface rebind 继续由 Host/Product convergence workflow 负责；Runtime lifecycle 中的 Rebind
evidence 不受影响。

## 7. Repository Migration

### 7.1 Version and execution

- SQL schema 为 `dash_complete_source` 增加 `repository_schema_version`。
- 新 source 直接写最终版本；旧 row 由 startup typed migration 在任何 production decode 前处理。
- migration 使用数据库 migration ledger/事务与必要锁保证单次生效。
- production loader 只接受最终版本并使用严格 decoder。

### 7.2 Normalization

Agent-owned migration transformer：

1. 读取已知旧 repository shape；
2. 从 queued/started 删除 `operation_id`；
3. 将旧 flat Applied 或 nested checkpoint 归一化为最小 Applied；
4. 保留真实 SideEffectStarted；不为 terminal history 合成中间事件；
5. 删除 lifecycle-local effects 与 persisted changes；
6. 重新序列化 history，并重算后续 Started `source_digest`、context revision 与 repository 相关 digest；
7. 通过最终 strict decoder、history fold 与 owner invariant 后原子写回并提升版本。

旧 active compaction 若缺少判断副作用边界所需的事实，migration 报错并定位 source，不假定 safe
retry 或 terminal。

### 7.3 Product receipt evidence

定点转换以下 typed fields 中的旧 `product-command:v2:*` operation handle 为同 suffix 的
`product-effect:v2:*`：

- Routine dispatch input handoff
- Channel AgentInput materialized delivery ref
- Lifecycle Gate accepted operation

Agent command IDs 继续使用 command namespace，不做全局字符串替换。

## 8. Why This Is Smaller and Safer

- 不建立新的 relation，只删除没有 owner 的 relation。
- 恢复仍由 public effect record、command lifecycle、history、lease 和 source fence承担。
- presentation 不再反向承担 inspect 身份职责。
- history 不再以 changes/checkpoint 形式重复持久化。
- migration 只生成可由既有事实确定的最终数据，不制造历史证据。

## 9. Explicitly Retained

- `AgentRuntimeOperationReceipt.operation_id`
- Complete Agent public effect records 与 request fingerprint
- Dash command IDs、dependency、status、queue/active state
- compaction ID、source head/digest、side-effect boundary 与 minimal Applied fact
- strict migration/version gate

外层 `dash_complete_effect` 与 source-local public effect record 暂不合并：两者目前分别有
facade/saga handoff 与 source recovery 消费者。若要继续精简，应另做 owner 合并设计，而不是顺手删除。
