# 实施计划

1. 删除 orchestrator 中 early AgentFrame revision/current_frame 写入。
2. 找到 accepted commit 可放置 frame revision 的现有路径，必要时新增小型 commit helper。
3. 补充 launch failure tests。
4. 修改 runtime registry，支持替换 HookRuntime。
5. 修改 hook dispatch lazy ensure，按 current HookControlTarget 校验缓存。
6. 补充 stale cache 回归测试。
7. 检查 connector InvalidConfig 到 API BadRequest 的链路是否已由 API child 覆盖；若此 child 先落地，先在 application 层保持 error category。

## Validation

- `cargo test -p agentdash-application launch_frame`
- `cargo test -p agentdash-application hook_runtime`
- 必要时运行 `pnpm run backend:clippy`
