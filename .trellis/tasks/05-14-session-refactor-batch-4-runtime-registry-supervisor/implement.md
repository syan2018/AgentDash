# Implementation Plan：Batch 4 Runtime Registry 与 Turn Supervisor

## Ordered Steps

- [x] 新增 `session/runtime_registry.rs`，封装 `SessionRuntime` map 与基本投影。
- [x] 新增 `session/turn_supervisor.rs`，先封装 cancel / stalled scan。
- [x] 将 `SessionHub` 构造改为注入 registry/supervisor，保留同一底层 map 以降低风险。
- [x] 迁移 `has_live_runtime` 调用点，按语义拆为 runtime entry / active turn / live executor session。
- [x] 迁移 prompt pipeline 的 claim/activate/processor_tx/terminal cleanup。
- [x] 迁移 cancel 与 stall detector。
- [x] 迁移 hook injection sink / tool builder 的 active turn 访问。
- [x] 迁移 runtime context transition 的 active turn 访问。
- [x] 删除生产代码中直接锁 `hub.sessions` 的调用。

## Verification

```powershell
cargo fmt --check
cargo check -p agentdash-application
cargo test -p agentdash-application session::hub
cargo test -p agentdash-application session::hub::tests::cancel_marks_running_turn_interrupted
cargo test -p agentdash-application session::hub::tests::schedule_hook_auto_resume_routes_through_augmenter
cargo test -p agentdash-application session::hub::runtime_context_transition
rg -n "sessions\\.lock\\(|\\.sessions" crates/agentdash-application/src/session
```

## Exit Criteria

- `SessionHub` 不再是 runtime map 的直接操作点。
- 并发 prompt / cancel / stall / hook auto-resume 行为保持现有测试语义。
- 运行态命名不再把 active turn 与 live executor session 混为一谈。
