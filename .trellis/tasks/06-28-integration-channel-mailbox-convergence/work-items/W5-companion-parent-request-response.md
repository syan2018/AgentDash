# W5: Companion Parent Request And Response

Status: planned

## Goal

Child 向 parent 提审和 parent 回应 child 都进入 mailbox。LifecycleGate 继续记录 review/wait/correlation，AgentRun 后续处理由 mailbox delivery 表达。

## Dependencies

- W0 source identity model 完成。
- W1 mailbox intake command shape 完成。
- W4 的 gate delivery adapter pattern 已稳定。

## Deliverables

- [ ] `open_parent_request` 创建 parent-owned gate 后创建 parent mailbox message，source identity 使用 `namespace=companion`、`kind=parent_request`、`source_ref=gate_id`。
- [ ] `resolve_parent_request` resolve parent-owned gate 后创建 child mailbox message，source identity 使用 `namespace=companion`、`kind=parent_response`、`source_ref=gate_id`。
- [ ] 保留 hook pending action 的 review/adoption 决策能力，但 AgentRun 继续处理的输入归 mailbox。
- [ ] parent request / response dedup key 使用 `companion_parent_request:{gate_id}` 与 `companion_parent_response:{gate_id}`。

## Acceptance

- [ ] child 向 parent 提审时，parent workspace 能看到 mailbox message。
- [ ] parent 回应后，child workspace 能看到 mailbox message。
- [ ] wait=true / wait=false 只影响 tool 等待方式，不影响 durable delivery fact。
- [ ] duplicate parent respond 不创建重复 child input。

## Suggested Validation

- `cargo test -p agentdash-application companion`
- `cargo test -p agentdash-application-agentrun mailbox`

## Parallel Guidance

W5 可以与 W6 并行做设计审阅，但不建议并行落代码，因为二者大概率都改 `companion/gate_control.rs`、`companion/tools.rs` 和 mailbox helper call sites。
