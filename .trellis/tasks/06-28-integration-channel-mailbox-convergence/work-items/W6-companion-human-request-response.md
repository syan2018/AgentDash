# W6: Companion Human Request And Response

Status: implemented

## Goal

Human request 继续作为 UI-facing 事件暴露给用户；human response resolve gate 后进入 requesting AgentRun mailbox，避免用户回应只作为 runtime notification 注入。

## Dependencies

- W0 source identity model 完成。
- W1 mailbox intake command shape 完成。
- W4 的 gate delivery adapter pattern 已稳定。

## Deliverables

- [x] `target=human` 创建 gate 后继续产生 human-visible UI notification / projection。
- [x] human respond resolve gate 后创建 requesting AgentRun mailbox message，source identity 使用 `namespace=companion`、`kind=human_response`、`source_ref=gate_id`。
- [x] wait=true 轮询 gate 的工具返回仍读取 gate payload；AgentRun 后续处理读取 mailbox message。
- [x] wait=false 用户回应后不再仅作为 runtime notification 注入。

## Acceptance

- [x] human request 在 UI 中可见并可回应。
- [x] human response 在 requesting AgentRun mailbox 中可见并可恢复投递。
- [x] requesting AgentRun running / paused / failed 行为全部复用 mailbox policy。
- [x] duplicate human response 不创建重复 AgentRun input。

## Implementation Notes

- `CompanionGateControlService::respond` resolve human gate 后通过 `CompanionHumanResponseMailboxDelivery` 投递 requesting AgentRun mailbox。
- API companion gate respond route 注入窄的 AgentRun mailbox runtime bridge，不依赖 runtime tool service 聚合。
- Gate payload 记录 `human_mailbox_delivery`；delivery `failed/blocked` 写入 failed payload 并返回错误。
- 旧 human response notification helper 仅保留给旧 runtime meta event 调用点；新的 AgentRun-facing continuation 以 mailbox message 为事实源。

## Suggested Validation

- `cargo test -p agentdash-application companion`
- companion gate API tests
- `cargo test -p agentdash-application-agentrun mailbox`

## Validation Results

- `cargo fmt --check` passed.
- `cargo test -p agentdash-application companion` passed.
- `cargo test -p agentdash-application-agentrun mailbox` passed.
- `cargo check -p agentdash-api` passed.

## Parallel Guidance

W6 可以与 W5 并行做设计审阅，但落代码建议顺序执行。若必须并行，W6 只负责 API/gate respond human path，W5 只负责 parent-owned gate path，并由主会话统一合并 `companion/tools.rs`。
