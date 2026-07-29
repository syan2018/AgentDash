# Agent Execution Simplification Research

## Conclusion

问题不是 `operation_id` 缺边界，而是同一执行被建模了多次。删除测试表明，应删除大多数
operation 投影、Dash internal effect ledger、rich checkpoint 和 persisted changes；唯一应保留的
operation ID 是跨层 receipt handle，其值直接锚定 Complete Agent public effect。

## Production Consumer Audit

### 1. Receipt operation 有真实消费者

`AgentRuntimeOperationReceipt.operation_id` 被以下 durable handoff 使用：

- Routine execution 的 `runtime_operation_id`
- Channel `MaterializedDeliveryRef::AgentInput.operation_id`
- Lifecycle Gate accepted operation
- saga receipt consistency checks

因此保留 receipt，但 Product 不再另造 identity；它返回 submitted public effect 的同值 handle。

### 2. Execution operation 投影没有消费者

- Agent active turn、queued compaction、compaction outcome、blocking evidence 中的 operation 字段只在
  Agent→Runtime→generated types/tests 之间搬运。
- 前端 production 不消费这些字段。
- `AgentRuntimeView.operations` 在 Agent projector 中固定为 `Vec::new()`。
- Native `ContextCompaction.operation_id` 只被生成合同与测试引用。

这些字段没有 owner，也不驱动任何 production decision，可以整体删除。

### 3. Dash 有两套 effect 记录，只有一套是 public owner

- Complete Agent source repository 的 public effect records 被 submit duplicate inspection、
  wait/inspect、queued lookup、terminalize 与 worker recovery 使用，应保留。
- `DashLifecycle.effects` 只被 `DashExecutionInspection.effect_outcome` 读取，后者没有外部生产
  消费者；B/C child effects 也只被 settlement 与测试观察。
- command status、dependency、active state、history 和 lease 已完整表达内部 A/B/C 执行链。

因此删除 lifecycle-local effect ledger 和 B/C child effect，不建立 root/child relation。

`DashEffectInspection.execution` 中的 command status、effect outcome、history head 与 consistency
也只有测试读取；Complete Agent reconciliation 只读取 public receipt `state`。因此该派生块可整体
删除，而不是仅删除 `effect_outcome`。

### 4. Rich CompactionCheckpoint 基本是 write-only

生产读取仅涉及：

- `context_revision`
- `summary_frame`
- `retained_from`
- transition validation 中重复的 `source_digest` 与 `operation_id`

`base_history_revision`、`applied_history_revision`、`summary`、compacted/retained entry IDs、
tool pairs、checkpoint digest、usage 和 created time 没有生产消费者。`summary_frame` 已是 canonical
summary；Started 已拥有 source fence。因此 checkpoint 可删除，Applied 平铺保存最小事实。

`source_digest` 有独立价值：provider compaction 的 `context_revision` 会把它与 summary、retained
boundary 一起 hash，且 Started 用它锁定精确历史快照。但它只需保存在 Started，不需在 Applied
重复。

### 5. Persisted changes 是 history 镜像

- 每个 history entry 都复制为 `DashAgentChangePayload::HistoryEntry`。
- `ActiveTurnChanged` 额外 change 在 Native adapter 中直接映射为空。
- change API 本来就遍历完整 history 并做 adapter projection。
- 镜像使 schema migration、digest 与一致性验证都翻倍，并允许两份内容漂移。

因此删除 persisted changes，按 history revision 即时投影 change feed。

### 6. Product Rebind 没有 effect

Product `Rebind` route 直接返回 succeeded receipt，没有 effect owner。真实 rebind 位于
Host/Product convergence workflow。该 command route 应删除，Runtime lifecycle Rebind evidence 保留。

## Minimal Invariants

1. 只有拥有接纳、inspect、terminal 或 recovery 的模块才能定义 effect identity。
2. 透明 facade 返回下游 owner 的 handle，不创造平行 operation。
3. presentation/snapshot 不承担 effect lookup 职责。
4. 内部顺序与恢复优先使用 command dependency、history 和 lease；只有独立可 inspect 副作用才建
   effect。
5. canonical history 只持久化一次；projection 不成为第二份 source of truth。
6. Applied 终态比 SideEffectStarted 中间事件更强；历史重放不要求为终态补造中间证据。
7. migration 只转换可确定事实，不猜测 identity、时间或副作用结果。

## Historical Failure Evidence

- `9e3b585ef` 为 `CompactionStarted` 新增必填 `operation_id`，无 migration。
- `8a22b594d` 引入 queued/side-effect/lease recovery，migration 只处理 active lease。
- `09691fed0` 将平铺 Applied 改为 nested `CompactionCheckpoint`，无 migration。
- persistence adapter 严格反序列化整个 repository，任一旧 payload 不匹配都会让会话整体读取失败。

只补缺失字段会依次撞上下一层旧 shape，并继续保留双重模型，不能解决根因。

## Migration Boundary

- 增加 repository schema version，在 production decode 前执行 typed migration。
- 同时识别已知旧平铺 Applied 与 nested checkpoint，输出唯一最终 shape。
- 删除 operation 字段、lifecycle-local effects、changes 镜像与 checkpoint 冗余字段。
- 由于 history serialized bytes 参与后续 source digest，按最终 serialization 重放并重算 fence /
  revision / repository digest。
- terminal Applied 无需合成缺失 SideEffectStarted；active 且 side-effect boundary 不可证明时明确失败。
- 定点修正 Routine/Channel/Gate 中旧 Product receipt handle，绝不全局替换 command ID。

## Deferred Question

外层 `dash_complete_effect` 与 source-local public effect record 仍有部分重复，但当前分别承担
pre-source/facade handoff 与 source worker recovery。若要继续合并，需要先选定唯一 owner 并改造
Create/Fork 及 source-open 边界；不纳入本次清理，避免把已证实的删除与未证实的重构混在一起。
