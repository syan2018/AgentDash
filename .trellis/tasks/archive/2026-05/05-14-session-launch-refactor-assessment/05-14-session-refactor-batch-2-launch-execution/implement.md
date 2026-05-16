# Implementation Plan：Batch 2 LaunchExecution

## Steps

1. 阅读 `prompt_pipeline.rs` 中 `start_prompt_with_follow_up`，标注纯解析与副作用边界。
2. 新增 `session/launch.rs`，定义 `LaunchExecution`、`LaunchSummary`、builder input。
3. 先把 connector projection 与 lifecycle summary 搬进 builder。
4. 修改 `start_prompt_with_follow_up` 使用 `LaunchExecution` 生成 `ExecutionContext`。
5. 保留旧 request 参数和 hub facade，不迁移入口 adapter。
6. 补测试：launch builder 单测 + 既有 hub characterization 回归。
7. 运行 focused tests 与 `cargo fmt --check`。

## Candidate Commands

```powershell
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application pending_capability_state_transition_applies_on_next_prompt_and_clears_meta
cargo test -p agentdash-application start_prompt_records_failed_terminal_when_connector_setup_fails
cargo test -p agentdash-application session::prompt_pipeline
cargo fmt --check
```

## Commit Plan

```text
feat(session): 引入 LaunchExecution

- 新增 launch execution plan 与 summary。
- 覆盖 lifecycle、pending transition、connector projection 的 plan tests。
```

```text
refactor(session): 让 prompt pipeline 执行 launch plan

- 将 start_prompt_with_follow_up 的执行前解析迁入 LaunchExecution builder。
- 保持 connector failure 与 pending apply-once 语义不变。
```

## Exit Criteria

- `start_prompt_with_follow_up` 的 connector context 构造经由 LaunchExecution。
- connector prompt 前有可测试 launch summary。
- 入口 adapter 与 `PromptSessionRequest` 删除留到 Batch 3。

## Verification

```powershell
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application pending_capability_state_transition_applies_on_next_prompt_and_clears_meta
cargo test -p agentdash-application start_prompt_records_failed_terminal_when_connector_setup_fails
cargo test -p agentdash-application session::prompt_pipeline
cargo fmt --check
```

以上命令均通过。
