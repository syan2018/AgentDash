# W0A: AgentRun Mailbox Directory Split

Status: completed

## Goal

把 `crates/agentdash-application-agentrun/src/agent_run/mailbox.rs` 收束为 `agent_run/mailbox/` 目录模块，先建立清晰文件边界，再推进 W1 intake command shape 和后续 Routine / Companion delivery 改造。

## Dependencies

- W0 source identity / schema baseline 完成并提交。

## Deliverables

- [x] 将 `agent_run/mailbox.rs` 拆为 `agent_run/mailbox/mod.rs` 与按职责分组的子模块。
- [x] 推荐边界至少覆盖 command DTO/result、delivery policy、scheduler、message creation、runtime delivery adapter glue、receipt replay、payload conversion/test helpers。
- [x] 对外 public API 保持由 `agent_run::mailbox` module 暴露，调用方不需要知道内部文件布局。
- [x] 不改变 mailbox 调度语义，不引入 Routine / Companion 新投递路径。
- [x] 更新相关 imports、tests 和 module declarations。

## Acceptance

- [x] `mailbox/mod.rs` 只保留 module exports、service struct 装配和必要 orchestration glue。
- [x] scheduler claim/consume、command accept、control commands、payload helpers 不再全部平铺在同一个文件。
- [x] W1 可以在目录化结构上新增 intake helper，而不继续扩大单文件。

## Suggested Validation

- `cargo check -p agentdash-application-agentrun`
- `cargo check -p agentdash-api`
- `cargo test -p agentdash-application-agentrun mailbox`

## Parallel Guidance

W0A 必须独占执行。它会移动大量 `agent_run/mailbox` 代码，不能与 W1-W6 并行落代码。W0A 完成后，W2 Routine 与 W3 Companion 才适合按文件所有权并行派发。
