# W2 — AgentSession Aggregate 与 Persistence

## Depends On

- W1 Hosted Agent Contract 完成并冻结 public IDs、commands、entities、changes 与 behavior suite。

## Goal

让 AgentSession 成为 Session/Operation/Queue/Turn/Item/Interaction/Context/Compaction 的唯一 durable owner，通过一个 transition kernel与一致的 in-memory/PostgreSQL repository强制跨状态不变量。

## Scope

- AgentSession aggregate 与正交 state types；
- command acceptance/admission、operation、queue、active slot、entity lifecycle、change outbox；
- in-memory repository；
- PostgreSQL normalized repository；
- hard-cut forward migration（预计 `0084_hosted_agent_session_cutover.sql`，以实施时下一个编号为准）；
- repository behavior suite 与真实数据库并发测试；
- 删除 authoritative `agent_runtime_event` persistence。

## Ownership

主要负责：

- `crates/agentdash-agent-runtime/**` 的 aggregate/kernel/repository port
- `crates/agentdash-infrastructure/**` 的 AgentSession PostgreSQL adapter
- `crates/agentdash-infrastructure/migrations/**` 的 hard-cut migration
- `crates/agentdash-agent-runtime-test-support/**` 的 repository behavior implementation

W3 开始前必须明确 effect/binding表的 contract；W3负责 delivery语义，W2负责 schema/transaction骨架。

## Deliverables

- transition kernel；
- in-memory 与 PostgreSQL repository；
- final normalized schema；
- migration与约束测试；
- authoritative Agent snapshot；
- transaction内 Agent Change outbox写入。

## Acceptance Criteria

- [ ] 一个 Session 最多一个 active Turn。
- [ ] 一个 Session 最多一个 nonterminal queued/active compaction。
- [ ] operation idempotency/fingerprint、queue order/dependency、context head CAS 与 change order受强制约束。
- [ ] Turn terminal、active slot、queue promotion决策可以在一个 Agent transaction完成。
- [ ] in-memory 与 PostgreSQL 跑同一 behavior suite。
- [ ] repository read不访问 Journal。
- [ ] migration删除旧 authoritative event/state schema且没有 backfill、双写或兼容 view。
- [ ] embedded PostgreSQL测试不会因共享 data root并发启动而误失败。

## Non-Goals

- 不实现具体 driver dispatch/inspect。
- 不做 AgentRun/API/frontend切换。
- 不把 worker lease状态加入 Agent aggregate。

## Validation

```powershell
pnpm migration:guard
cargo test -p agentdash-agent-runtime
cargo test -p agentdash-infrastructure agent_session
cargo test -p agentdash-infrastructure agent_runtime
```
