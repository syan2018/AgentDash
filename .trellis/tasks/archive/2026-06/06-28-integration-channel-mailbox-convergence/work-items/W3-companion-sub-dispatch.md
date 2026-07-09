# W3: Companion Sub Dispatch Into Child Mailbox

Status: implemented

## Goal

将 Companion sub child 创建与 child 首条任务投递拆开。Child AgentRun 创建仍由 lifecycle/gate/lineage 负责，首条 `dispatch_prompt` 进入 child mailbox。

## Dependencies

- W0 source identity model 完成。
- W1 mailbox intake command shape 完成。

## Deliverables

- [x] 将 `CompanionChildDispatchService::dispatch_child` 调整为只负责创建 child AgentRun、AgentFrame、RuntimeSession anchor、LifecycleGate、lineage 和 task binding。
- [x] child 首条 `dispatch_prompt` 通过 child mailbox 投递，source identity 使用 `namespace=companion`、`kind=dispatch`、`source_ref=dispatch_id/gate_id`。
- [x] 保留 `CompanionLaunchSource` 作为 frame construction modifier，不再由 tool 层 direct launch child session。
- [x] wait=true / wait=false 都保留 dispatch correlation，便于后续结果回流。

## Acceptance

- [x] child initial task 在 child workspace mailbox 中可见。
- [x] child idle / running / paused / failed 行为全部由 mailbox scheduler 决定。
- [x] wait=true 的 parent tool 仍通过 gate 等待 child result。
- [x] task assignment 与 selected ProjectAgent binding 仍可追溯。

## Implementation Notes

- Companion sub dispatch 创建 child anchors 后调用 `AgentRunMailboxIntakeCommand` 投递 `dispatch_prompt`。
- Source identity 使用 `namespace=companion`、`kind=dispatch`、`route=sub`、`correlation_ref=dispatch_id`，`source_ref` 优先使用 gate id，wait=false 时使用 dispatch id。
- `CompanionLaunchSource` 不再走 tool 层 direct launch；当前通过 child frame construction 与 mailbox executor config 继续承接执行上下文。

## Suggested Validation

- `cargo test -p agentdash-application companion`
- `cargo test -p agentdash-application-agentrun mailbox`

## Validation Results

- `cargo fmt --check` passed.
- `cargo test -p agentdash-application companion` passed.
- `cargo test -p agentdash-application-agentrun mailbox` passed.
- `cargo check -p agentdash-api` passed.

## Parallel Guidance

W3 可以与 W2 并行。W3 应先于 W4 完成，原因是 W4 的 end-to-end result 验证依赖 child dispatch correlation 和 child mailbox launch 语义稳定。
