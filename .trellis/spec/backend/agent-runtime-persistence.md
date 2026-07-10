# Managed Agent Runtime PostgreSQL Persistence

## 1. Scope / Trigger

本规范适用于 `agentdash-agent-runtime` 的持久化 ports、PostgreSQL adapter、managed runtime/context migration，以及消费 runtime durable work 的 worker。修改 `RuntimeCommit`、runtime/context 表、binding/source 引用或 claim/ack/release 语义时必须同步复核本规范。

## 2. Signatures

```rust
pub trait RuntimeUnitOfWork {
    async fn commit(&self, commit: RuntimeCommit) -> Result<(), RuntimeStoreError>;
    async fn quarantine(&self, event: QuarantinedDriverEvent) -> Result<(), RuntimeStoreError>;
}

pub trait RuntimeWorkQueue {
    async fn claim(
        &self,
        request: RuntimeWorkClaimRequest,
    ) -> Result<Vec<RuntimeWorkClaim>, RuntimeStoreError>;
    async fn ack(&self, claim: &RuntimeWorkClaim) -> Result<(), RuntimeStoreError>;
    async fn release(
        &self,
        claim: &RuntimeWorkClaim,
        error: String,
    ) -> Result<(), RuntimeStoreError>;
}
```

`RuntimeCommit` 是 projection、operation、event journal、entity projection、context saga、outbox 与 quarantine 的完整原子写集。`RuntimeWorkKind` 当前包含 `RuntimeOutbox`、`ContextPreparation`、`ContextActivationDispatch`、`ContextActivationRecovery`。

## 3. Contracts

- PostgreSQL adapter 在同一事务中先锁定并校验 Thread projection CAS，再写入 `RuntimeCommit` 的全部 durable facts，最后校验 projection cursor 与实际 operation/event 序列一致。首次创建也必须通过 insert-if-absent 后重读实际 revision 形成真实 CAS。
- operation sequence 与 event sequence 必须从数据库现有 cursor 的下一位开始严格连续。projection 不得在缺少对应 durable fact 时推进 cursor。
- Runtime-owned schema 持有 Thread、Operation、Event、Turn、Item、Interaction、Outbox、Quarantine 及 Context Checkpoint/Preparation/Candidate/Activation/Dispatch/Head。跨实体一致性同时由 domain 校验与 composite foreign key/unique constraint 保护。
- Context Head 只能指向同一 Thread 下的非 `opaque` immutable checkpoint，并完整匹配 checkpoint revision、digest、fidelity、settings revision 与 tool-set revision。
- `agent_runtime_binding` 与 `agent_runtime_source_coordinate` 是 Integration Driver Host 所有的坐标事实。Runtime persistence 仅引用并校验它们，不创建、不推进 generation，也不改写 source coordinate。
- Runtime schema 从新 contract 独立建立；旧 session/connector 表不参与读取、回填或双写。切换与删除旧事实源属于 AgentRun cutover 阶段。
- claim 使用数据库时钟、`FOR UPDATE SKIP LOCKED`、owner、随机 token、到期时间和 attempt。只有仍持有相同 owner/token 且 lease 未过期的 worker 能 ack/release；到期后新 claim 必须生成新 token 并增加 attempt，旧 worker 不得确认新一轮工作。
- queue 只负责 work 的租约和交付确认，业务状态仍留在各自 runtime/context 表。Activation dispatch 仅能 claim `prepared` activation。

## 4. Validation & Error Matrix

| 场景 | 必须得到的结果 |
| --- | --- |
| projection revision 与数据库不一致 | `ProjectionConflict`，事务不产生部分写入 |
| 首次创建 Thread 并发冲突 | 重读实际 revision 后返回 typed conflict |
| operation/event sequence 跳号、重复或 cursor 无事实推进 | 拒绝整个 commit |
| context candidate/activation/head 坐标不一致 | `ContextInvariant` 或数据库 constraint violation，事务回滚 |
| binding/source/generation 不存在或不匹配 | foreign key/typed store error；Runtime 不补造 Host 事实 |
| claim 参数非法 | `InvalidWorkClaim` |
| owner/token 不匹配、lease 已过期或已被接管 | `WorkClaimConflict`，不得 ack/release |
| worker release 有效 claim | 记录错误并释放租约，业务 work 保持可重试 |
| worker ack 有效 claim | durable work 被确认，不能再次 claim |

## 5. Good / Base / Bad Cases

**Good case:** command transaction 以预期 revision 提交连续 operation/event，原子更新 projection 和 outbox；worker 通过有效 lease 执行副作用后 ack。

**Base case:** worker 在 lease 内失败并 release，另一个 worker 随后 claim 同一业务 work，attempt 增加且获得新 token。

**Bad case:** worker 超时后仍用旧 token ack，或 adapter 为通过外键自行创建 binding/source。这两种行为都会破坏 generation fencing 与 Host/Runtime ownership，必须被拒绝。

## 6. Tests Required

- 使用真实 embedded PostgreSQL 覆盖 migration readiness、create/update CAS、并发首次创建、事务回滚、幂等与 sequence/cursor 连续性。
- 覆盖 composite foreign key、Head/checkpoint fidelity 与 revision/digest/settings/tool-set 一致性。
- 对四类 `RuntimeWorkKind` 覆盖 claim 隔离、limit、attempt、ack、release、lease 到期接管和 stale worker fencing。
- 明确验证 Runtime adapter 不写 binding/source；测试 fixture 需要由 Host 角色显式 seed 坐标。
- migration guard 必须确认 managed runtime migration 不引用旧 session runtime/connector 表。

## 7. Wrong vs Correct

```rust
// Wrong: projection cursor 可以脱离 durable facts 单独前进。
commit.projection.next_event_sequence = EventSequence(42);
commit.events.clear();

// Correct: cursor 由同一事务内严格连续的 journal facts 推进并被 adapter 校验。
commit.events = next_contiguous_events;
commit.projection.next_event_sequence = sequence_after(&commit.events);
```

```rust
// Wrong: 仅凭 work identity 删除/确认工作，超时 worker 可以误伤新的 claim。
queue.ack_by_identity(identity).await?;

// Correct: ack/release 携带完整 claim，并校验 owner、token 与数据库时钟下的 lease。
queue.ack(&claim).await?;
```
