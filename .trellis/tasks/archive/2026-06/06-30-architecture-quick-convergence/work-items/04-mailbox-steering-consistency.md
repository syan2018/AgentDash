# Work Item 04: Mailbox steering 语义一致性

## Goal

合并 AgentRun mailbox 中 delegate steering 与 scheduler steering 的重复消费路径，确保同类 steering delivery 具有一致的 receipt/status/error/event 语义。

## Source Issues

- `adversarial-review.md` Issue 9。
- `research/06-agent-runtime-session-surface.md` Issue 3。

## Evidence

- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:297` delegate path 调 `consume_as_delegate_steering`。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:556` scheduler path 调 `consume_as_steering`。
- `scheduler.rs:337` delegate event write failure 标记 `Failed`。
- `scheduler.rs:639` normal steering 可标记 `Steered` 但带 `last_error`。

## Requirements

- active turn validation、expected turn guard、event emission、receipt completion、status write、payload cleanup 由一个 shared executor 拥有。
- delegate path 与 scheduler path 对 event projection failure 的 terminal/error semantics 一致。
- delegate path 只负责把 accepted steering 输出为 agent loop 需要的 `AgentMessage` / equivalent shape。
- 保持 durable mailbox envelope 与 receipt 为唯一 command fact source。

## Suggested Implementation Shape

- 抽 `SteeringDeliveryExecutor` 或 scheduler 内部 helper。
- 输入包含：
  - mailbox message / receipt context。
  - expected turn。
  - active session/turn state。
  - output mode：delegate-return vs live-steer。
- 输出包含：
  - final mailbox status。
  - receipt terminal state。
  - optional agent loop messages。
  - diagnostics/error。
- 两条旧函数改为薄 adapter。

## Tests / Verification

- active turn mismatch 两条路径同语义。
- event projection failure 两条路径同语义。
- receipt completion 一致。
- payload cleanup 一致。

## Out of Scope

- 不重构整个 AgentRunMailboxService。
- 不拆 AgentRuntimeDelegate。
- 不统一 command availability resolver。
