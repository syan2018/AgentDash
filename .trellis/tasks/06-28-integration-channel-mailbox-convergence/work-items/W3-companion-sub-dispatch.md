# W3: Companion Sub Dispatch Into Child Mailbox

Status: planned

## Goal

将 Companion sub child 创建与 child 首条任务投递拆开。Child AgentRun 创建仍由 lifecycle/gate/lineage 负责，首条 `dispatch_prompt` 进入 child mailbox。

## Dependencies

- W0 source identity model 完成。
- W1 mailbox intake command shape 完成。

## Deliverables

- [ ] 将 `CompanionChildDispatchService::dispatch_child` 调整为只负责创建 child AgentRun、AgentFrame、RuntimeSession anchor、LifecycleGate、lineage 和 task binding。
- [ ] child 首条 `dispatch_prompt` 通过 child mailbox 投递，source identity 使用 `namespace=companion`、`kind=dispatch`、`source_ref=dispatch_id/gate_id`。
- [ ] 保留 `CompanionLaunchSource` 作为 frame construction modifier，不再由 tool 层 direct launch child session。
- [ ] wait=true / wait=false 都保留 dispatch correlation，便于后续结果回流。

## Acceptance

- [ ] child initial task 在 child workspace mailbox 中可见。
- [ ] child idle / running / paused / failed 行为全部由 mailbox scheduler 决定。
- [ ] wait=true 的 parent tool 仍通过 gate 等待 child result。
- [ ] task assignment 与 selected ProjectAgent binding 仍可追溯。

## Suggested Validation

- `cargo test -p agentdash-application companion`
- `cargo test -p agentdash-application-agentrun mailbox`

## Parallel Guidance

W3 可以与 W2 并行。W3 应先于 W4 完成，原因是 W4 的 end-to-end result 验证依赖 child dispatch correlation 和 child mailbox launch 语义稳定。
