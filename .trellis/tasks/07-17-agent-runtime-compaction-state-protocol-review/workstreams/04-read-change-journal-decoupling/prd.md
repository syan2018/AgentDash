# W4 — Authoritative Read、Change 与 Journal 解耦

## Depends On

- W1 Hosted Agent Contract。
- W2 AgentSession aggregate、repository 与 change outbox transaction。

## Goal

让 Session current state、reconnect和fork直接读取 AgentSession；让 Journal、App Server notification、audit/search/analytics只消费 after-commit Agent Change，并通过删除测试证明 Journal不再是隐藏事实源。

## Scope

- authoritative Session snapshot/query；
- Agent revision + ordered change tail；
- cursor gap/retention与snapshot reread；
- stable revision/Turn/Item cutoff fork；
- Journal/projector consumer；
- product-level非 Agent event feed分离；
- 删除 `journal_records_after` 驱动的 Session/context/fork/terminal/admission；
- Journal no-op/delete architecture test。

## Ownership

主要负责：

- `crates/agentdash-agent-runtime/**` 的 read/change/fork gateway
- `crates/agentdash-infrastructure/**` 的 outbox delivery/projection repository
- `crates/agentdash-application-agentrun/**` 中仅 Session read/fork/change接入部分
- Journal/projector边界

与 W6 协调：W4提供稳定 read/change contract；W6负责完整 AgentRun/API/UI消费切换。

## Deliverables

- Session snapshot at revision；
- ordered Agent Change subscription；
- typed gap contract；
- Agent-owned fork；
- Journal consumer/no-op adapter；
- architecture deletion test。

## Acceptance Criteria

- [ ] read/resume/fork从 AgentSession repository获得事实。
- [ ] reconnect使用 snapshot R + changes after R，gap时重新snapshot。
- [ ] fork不拼接、截断或重新编号presentation records。
- [ ] Journal consumer延迟、停止或删除不会影响Agent command/context/recovery。
- [ ] protocol projector可由snapshot+change恢复current view。
- [ ] public Application API不能绕过Agent追加Session presentation。
- [ ] Session业务路径不调用 `journal_records_after`。

## Non-Goals

- 不实现完整frontend reducer。
- 不扩展audit/search/analytics产品功能。
- 不把outbox变成永久event-sourced aggregate store。

## Validation

```powershell
cargo test -p agentdash-agent-runtime snapshot
cargo test -p agentdash-application-agentrun session_read
cargo test -p agentdash-application-agentrun fork
cargo test -p agentdash-infrastructure agent_change_outbox
```
