# 实施计划

1. 删除 orchestrator 中 early AgentFrame revision/current_frame 写入。
2. 找到 accepted commit 可放置 frame revision 的现有路径，必要时新增小型 commit helper。
3. 补充 launch failure tests。
4. 修改 runtime registry，支持替换 HookRuntime。
5. 修改 hook dispatch lazy ensure，按 current HookControlTarget 校验缓存。
6. 补充 stale cache 回归测试。
7. 检查 connector InvalidConfig 到 API BadRequest 的链路是否已由 API child 覆盖；若此 child 先落地，先在 application 层保持 error category。

## Validation

- `cargo test -p agentdash-application current_frame -- --nocapture`
- `cargo test -p agentdash-application hook_runtime_target_switch_replaces_stale_cached_runtime -- --nocapture`
- `cargo check -p agentdash-application`
- `cargo clippy -p agentdash-application -- -D warnings`

## Result

- `SessionLaunchOrchestrator` 不再在 connector accepted 前写 AgentFrame revision 或推进 `LifecycleAgent.current_frame_id`。
- `TurnCommitter` 在 accepted commit 后写入新的 AgentFrame revision，并同步 current frame。
- `SessionRuntimeRegistry` 提供 `set_or_replace_hook_runtime`，delivery-session adapter 会按当前 `HookControlTarget` 刷新或替换 stale cache。
- 回归测试覆盖 connector setup failure、planner InvalidConfig、accepted turn frame commit、HookRuntime target switch。
