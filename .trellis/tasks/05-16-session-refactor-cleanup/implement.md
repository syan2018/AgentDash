# Session 重构彻底收尾 Implement

## Completed

- [x] `TurnExecution` 记录 stream adapter abort handle。
- [x] `TurnSupervisor` 登记并在 active turn 清理时中止 adapter task。
- [x] `prompt_pipeline` spawn adapter 后通过 supervisor 登记。
- [x] 相关 `turn_supervisor`、bootstrap/provider 回归测试通过。
- [x] `SessionEffectsService` 改为持有 terminal effect deps，dispatcher 不再反向依赖 `SessionRuntimeInner`。
- [x] terminal effect hook trigger / auto-resume 改为 port，callback/registry/store 显式注入。
- [x] `SessionLaunchPlanner` 删除 hook resolve 失败时的 turn 清理，`SessionLaunchExecutor` 统一释放 claim/hook。
- [x] API `ServiceSet.session_hub` 删除，构造期改用 `SessionRuntimeBuilder`。
- [x] 本机 relay 改持有 `SessionRuntimeServices`，不再向 command handler / ws config 传内部 runtime 装配对象。
- [x] 删除 `SessionRuntimeInner` facade 中无人使用的历史代理方法，不用 `allow(dead_code)` 掩盖残壳。
- [x] 删除 `SessionHub` 代码符号，内部残余装配对象改为 crate-private `SessionRuntimeInner`。
- [x] runtime command record/status/API 从 pending 命名收敛为 requested，并新增 PostgreSQL migration。

## Next Steps

1. 执行最终 git diff 审核。
2. 提交本轮重构收尾。

## Validation

- `cargo test -p agentdash-application terminal_effect --lib`
- `cargo test -p agentdash-application turn_supervisor --lib`
- `cargo test -p agentdash-application connector_setup_failure_does_not_commit_bootstrap_or_requested_commands --lib`
- `cargo test -p agentdash-application launch_prompt_strict_requires_session_construction_provider --lib`
- `cargo test -p agentdash-application session::hub --lib`
- `cargo test -p agentdash-application runtime_command --lib`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-local`
