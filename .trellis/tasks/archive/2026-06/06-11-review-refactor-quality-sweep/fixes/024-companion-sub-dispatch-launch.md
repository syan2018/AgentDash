# Fix 024: companion sub dispatch launch chain

## 范围

- `crates/agentdash-application/src/companion/tools.rs`
- `crates/agentdash-application/src/session/assembler.rs`
- `crates/agentdash-application/src/session/assembly_builder.rs`

## 更新

- `target=sub` 的 `companion_request` 保留 `LifecycleDispatchService` 负责创建 run / agent / frame / gate，并捕获 `delivery_runtime_ref` 作为 child delivery runtime session。
- dispatch plan 现在通过 `build_companion_dispatch_prompt(&plan, prompt)` 生成真实 child prompt，并随 `CompanionLaunchSource` 进入 session construction。
- 解析出的 companion executor config 进入 `CompanionLaunchSource.companion_executor_config`，子 session 不再沿用未消费的 `_companion_executor_config` 链路。
- control-plane child 创建完成后调用 `session_services.launch.launch_command_with_outcome(child_session_id, LaunchCommand::companion_dispatch_input(...))` 启动 child runtime turn。
- `AgentToolResult.details` 在 wait 和 async 路径都返回 `delivery_runtime_session_id` / `child_session_id` / `child_turn_id` / `context_sources`。
- 删除未使用的 `CompanionAgentRef`。
- companion parent facts 缺失改为 construction 阶段显式错误；`Full` / `Compact` slice 缺 parent VFS 不再转为空 VFS，`WorkflowOnly` / `ConstraintsOnly` 继续保留业务语义上的空 VFS。

## 验证

- `cargo check -p agentdash-application`：通过。
- `cargo test -p agentdash-application companion`：通过，39 passed。
- `cargo test -p agentdash-application session::launch`：通过，7 passed。
- `cargo test -p agentdash-application workflow::frame_construction`：通过，0 个匹配测试。

## 备注

- 未做 Batch 3 request/respond 文件拆分。
- 未接入 platform grant。
- 测试输出存在既有 `session::construction` dead_code warnings，本轮未触及。
