# Managed Agent Runtime 持久内核与 Context Compaction

## Goal

实现 RuntimeThread/Turn/Item/Interaction/Operation 的唯一业务状态机与事实源，并将 context construction、checkpoint、restore/fork、compaction、recovery 从 application/session/core 中收敛到 Managed Runtime。

## Depends On

- `01-runtime-contract`

## Parent Design

- `../../design.md` 第 5、12、15 节
- `../../implement.md` 第 4 节

## Requirements

- 实现 `execute/snapshot/events` gateway、per-thread operation sequence、idempotency 与 expected revision admission。
- 实现 Turn/Item/Interaction exactly-one terminal、Lost 与 protocol violation state transition。
- authoritative journal、projection、outbox 与 cursor 同事务；transient delta 与 durable event 分离。
- 实现 ContextRecipe、MaterializedContext、immutable Checkpoint、ActiveHead、fidelity 与 settings/tool revisions。
- Manual/automatic compaction 共用 candidate durable -> driver activate -> head CAS -> terminal saga。
- 实现 driver generation fencing、late event quarantine 与 crash recovery。
- 实现 HookPlanRevision、runtime lifecycle hook orchestration、durable actionful HookTrace 与 hook effect recovery；具体执行点由 bound HookProfile 选择。
- Hook产生的ContextFrame、Interaction、mailbox effect与domain effect进入同一journal/outbox；进程内notice/pending queue不再是事实源。
- required Hook按显式FailClosed/FailOpenWithDiagnostic/RetryDurableEffect/ObserveOnly policy收敛，不由调用路径偶然决定。
- 提供 PostgreSQL repository/transaction ports，并提交目标 schema migration。
- 删除被新内核替代的旧 checkpoint/head/event 写路径，不保留 dual write。

## Acceptance Criteria

- [ ] mutation durable accept 后才触发 driver side effect。
- [ ] event append、projection/head 与 terminal persistence 不再分叉。
- [ ] database failure 不会产生假 Completed。
- [ ] candidate persist、driver apply、head CAS 三个 crash point均可恢复。
- [ ] active context head 使用 expected revision CAS，并发不能回退。
- [ ] `thread/read` 与 `thread/context/read` 的 fidelity 明确不同。
- [ ] Actionful HookRun/effect在crash/replay后幂等；silent observer不推进durable cursor。
- [ ] 空库与代表性预研数据 migration tests 通过，旧 active state不冒充可恢复。
