# W5 — Typed Context、Compaction 与 Continuation

## Depends On

- W1 Hosted Agent Contract。
- W2 AgentSession aggregate与repository。
- W3 execution effect、binding、dispatch/inspect settlement。
- W4 authoritative read/change与Agent-owned fork基础。

## Goal

用 compaction tracer bullet证明Hosted Agent的完整状态链：typed context在写入时成立，manual/automatic压缩占用独立Turn，active期间消息durable排队，success/failure/cancel/Lost与continuation都确定收敛。

## Scope

- `ModelContribution` 与 immutable ContextRevision；
- preparation/capability gate；
- Compaction `Preparing → Synchronizing? → terminal`；
- queued manual compaction与atomic promotion；
- active compaction期间mailbox；
- manual success → Idle；
- automatic A/B/C与durable continuation dependency；
- clean Failed、Cancelled、Lost；
- cancellation与late observation；
- 删除presentation replay/context activation二次mapper。

## Ownership

主要负责：

- `crates/agentdash-agent-runtime/**` 的 context/compaction/queue/continuation transitions
- `crates/agentdash-infrastructure/**` 的context/compaction repository与effect integration
- driver adapter中仅typed context capability/apply/inspect路径
- compaction behavior/fault/concurrency tests

W6负责协议/UI投影；W5必须先提交稳定Agent Change语义。

## Deliverables

- typed context pipeline；
- manual/automatic compaction state machine；
- A/B/C独立identity与transaction；
- atomic queue promotion；
- cancellation/failure/Lost；
- cross-adaptercontext capability tests。

## Acceptance Criteria

- [ ] 每个model-visible Item在commit时已有typed contribution；presentation不反向成为context输入。
- [ ] queued compaction不创建Turn/Item/change lifecycle。
- [ ] active Turn terminal与B start/preparation effect在一个Agent transaction。
- [ ] active B期间新message只进mailbox，不steer、不创建普通Turn。
- [ ] manual B success后Idle且无continuation。
- [ ] automatic A、B、C的Turn/Operation identity全部不同。
- [ ] B terminal commit不创建C；独立mailbox promotion才创建C。
- [ ] B clean Failed exactly-once失败continuation且无Turn C；其他无依赖entry可继续。
- [ ] B Lost阻断全部promotion。
- [ ] Preparing可安全取消；Synchronizing必须inspect后terminal。
- [ ] duplicate/reclaim/restart不复制B/C或effect。

## Non-Goals

- 不把manual压缩成功与“自动开始新Turn”耦合。
- 不从Journal sequence确定context cutoff。
- 不为了stateful adapter复制一套平台transcript。

## Validation

```powershell
cargo test -p agentdash-agent-runtime compaction
cargo test -p agentdash-agent-runtime continuation
cargo test -p agentdash-infrastructure compaction
cargo test -p agentdash-infrastructure context_revision
```
