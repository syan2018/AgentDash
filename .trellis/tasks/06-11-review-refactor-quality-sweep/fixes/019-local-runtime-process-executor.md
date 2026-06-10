# FIX-019: local-runtime ProcessExecutor 与 env overlay 权限策略

## 模块

`local-runtime`

## 来源

- `research/local-runtime-followup-executable-plan.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/backend/permission/policy-engine.md`

## 更新

- 新增 `ProcessExecutor`，集中处理 workspace cwd resolve、shell command wrapping、argv exec、timeout、stdout/stderr UTF-8 decode 与退出码投影。
- `ToolExecutor::shell_exec()` 和 `shell_exec_streaming()` 保留原调用面，内部委托 `ProcessExecutor`；relay shell timeout 仍返回 `ToolError::Timeout`。
- `process.shell` 与 `process.exec` 统一通过 `ProcessExecutor` 执行，Host API 层只负责解析 shell string 或 argv 输入。
- `process.exec` 的 `args` 改为 fail-closed typed parsing，非字符串参数返回 host API 参数错误。
- `options.env` 对 shell/exec 行为一致，非对象或非字符串 env value 返回 host API 参数错误。
- 显式 `options.env` overlay 除 `process.execute` 外，还逐个变量要求当前 action/channel method 声明 `env.read` 或 `env.read:<KEY>`，复用既有 permission family。
- `process.exec` / `process.shell` timeout 继续向 extension host 返回 `{ timed_out: true }`，输出截断仍只发生在 host API response 投影阶段。

## 涉及文件

- `crates/agentdash-local/src/process_executor.rs`
- `crates/agentdash-local/src/lib.rs`
- `crates/agentdash-local/src/tool_executor.rs`
- `crates/agentdash-local/src/extensions/host/process_api.rs`
- `crates/agentdash-local/src/extensions/host/host_api.rs`

## 验证

- `cargo fmt -p agentdash-local`：通过。
- `cargo test -p agentdash-local process_host_apis`：1 passed。
- `cargo test -p agentdash-local shell_exec`：3 passed。
- `cargo test -p agentdash-local built_in_host_apis_use_action_permissions_and_workspace_boundary`：1 passed。
- `cargo check -p agentdash-local`：通过。
- Rust 命令输出存在既有 `agentdash-executor` unused import warning，与本次改动无关。

## Commit

未提交；本轮按要求只完成代码与记录更新。
