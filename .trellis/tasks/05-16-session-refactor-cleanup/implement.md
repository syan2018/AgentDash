# Session 重构收尾清理 Implement

## Steps

1. 在 `session/hub_support.rs` 的 `TurnExecution` 增加 stream adapter abort handle 字段，并在构造函数初始化为 `None`。
2. 在 `session/turn_supervisor.rs` 增加：
   - 清理 active turn 时中止 adapter handle 的内部 helper。
   - `register_stream_adapter_handle` 方法。
   - 覆盖登记与清理的单元测试。
3. 修改 `session/prompt_pipeline.rs`：
   - `spawn_stream_adapter` 返回 `JoinHandle<()>`。
   - spawn 后通过 supervisor 登记 abort handle。
4. 运行验证：
   - `cargo test -p agentdash-application turn_supervisor --lib`
   - `cargo test -p agentdash-application connector_setup_failure_does_not_commit_bootstrap_or_pending_commands --lib`
   - `cargo test -p agentdash-application launch_prompt_strict_requires_session_construction_provider --lib`

## Risk Notes

- adapter task 中止必须只发生在 active turn 清理时，不能影响 session 级 hook runtime 或 profile。
- 清理逻辑需要接受 active turn 不存在的幂等情况。

## Review Gate

用户已明确要求“直接开始执行”；本 task 的 planning artifacts 记录上述范围后即可 `task.py start`。
