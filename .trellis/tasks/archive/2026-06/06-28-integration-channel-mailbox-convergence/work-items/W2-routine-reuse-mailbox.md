# W2: Routine Reuse Into Mailbox

Status: implemented

## Goal

让 `DispatchStrategy::Reuse` 与 repeated `PerEntity` 的 Routine 触发进入目标 AgentRun mailbox，由 mailbox scheduler 决定 idle/running/paused/failure 行为。

## Dependencies

- W0 source identity model 完成。
- W1 mailbox intake command shape 完成。

## Deliverables

- [x] 在 Routine application 层引入明确 dispatch branch：new target 走 lifecycle creation；existing target 转 mailbox intake。
- [x] `DispatchStrategy::Reuse` 命中已有 target 时调用 mailbox intake，source identity 使用 `namespace=routine`、`kind=trigger`、`source_ref=routine_execution_id`。
- [x] `DispatchStrategy::PerEntity` 首次触发按 Fresh 创建 run；后续解析到 entity target 时按 Reuse 入 mailbox。
- [x] 在 `RoutineExecution` dispatch refs 中记录 mailbox message / command receipt / delivery refs。
- [x] client command id / dedup key 使用 `routine_execution:{execution_id}`。

## Acceptance

- [x] Reuse target idle 时由 mailbox scheduler 启动或恢复目标 AgentRun。
- [x] Reuse target running 时按 mailbox turn boundary 排队。
- [x] target paused / failed 时沿用 mailbox paused/manual resume 行为。
- [x] duplicate RoutineExecution replay 不产生第二条 AgentRun 输入。
- [x] RoutineExecution status 能表达 mailbox accepted / queued / failed 的结果。

## Implementation Notes

- `RoutineExecutor` 在 reuse target 存在时直接构造 `AgentRunMailboxIntakeTargetCommand`，Fresh 与首次 PerEntity 继续使用 lifecycle dispatch 创建 anchor。
- `RoutineDispatchRefs` 记录 runtime refs 与 mailbox refs；mailbox outcome 保存在 `mailbox_refs.outcome`，执行状态仍保持 dispatch/failure 主状态。
- Routine intake 的 source identity 使用 `routine/trigger` 与 `source_ref=routine_execution_id`；`routine_execution:{execution_id}` 保留为显式 fallback dedup key，实际 mailbox stable key 仍优先来源于 source identity。

## Suggested Validation

- `cargo test -p agentdash-application routine`
- `cargo test -p agentdash-application-agentrun mailbox`

## Validation Results

- `cargo fmt --check` passed.
- `cargo test -p agentdash-application routine` passed.
- `cargo test -p agentdash-application-agentrun mailbox` passed.
- `cargo check -p agentdash-api` passed.

## Parallel Guidance

W2 可以在 W1 完成后与 W3 并行。W2 主要触碰 `routine/*` 和 mailbox helper，避免同时修改 Companion gate 文件。
