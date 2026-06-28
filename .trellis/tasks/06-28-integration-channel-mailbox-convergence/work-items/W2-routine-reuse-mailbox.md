# W2: Routine Reuse Into Mailbox

Status: planned

## Goal

让 `DispatchStrategy::Reuse` 与 repeated `PerEntity` 的 Routine 触发进入目标 AgentRun mailbox，由 mailbox scheduler 决定 idle/running/paused/failure 行为。

## Dependencies

- W0 source/schema baseline 完成。
- W1 mailbox intake command shape 完成。

## Deliverables

- [ ] 在 Routine application 层引入明确 dispatch branch：new target 走 lifecycle creation；existing target 转 mailbox intake。
- [ ] `DispatchStrategy::Reuse` 命中已有 target 时调用 mailbox intake，source 使用 `routine_executor`。
- [ ] `DispatchStrategy::PerEntity` 首次触发按 Fresh 创建 run；后续解析到 entity target 时按 Reuse 入 mailbox。
- [ ] 在 `RoutineExecution` dispatch refs 中记录 mailbox message / command receipt / delivery refs。
- [ ] client command id / dedup key 使用 `routine_execution:{execution_id}`。

## Acceptance

- [ ] Reuse target idle 时由 mailbox scheduler 启动或恢复目标 AgentRun。
- [ ] Reuse target running 时按 mailbox turn boundary 排队。
- [ ] target paused / failed 时沿用 mailbox paused/manual resume 行为。
- [ ] duplicate RoutineExecution replay 不产生第二条 AgentRun 输入。
- [ ] RoutineExecution status 能表达 mailbox accepted / queued / failed 的结果。

## Suggested Validation

- `cargo test -p agentdash-application routine`
- `cargo test -p agentdash-application-agentrun mailbox`

## Parallel Guidance

W2 可以在 W1 完成后与 W3 并行。W2 主要触碰 `routine/*` 和 mailbox helper，避免同时修改 Companion gate 文件。

