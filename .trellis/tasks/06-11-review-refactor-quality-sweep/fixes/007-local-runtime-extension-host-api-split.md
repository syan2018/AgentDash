# FIX-007: local-runtime Extension Host API 拆分

## 模块

`local-runtime`

## 来源

- `reviews/004-local-runtime.md`
- `research/local-runtime-executable-plan.md`
- worker: `019eb2b5-5433-7612-828b-cb40aa0a26d7`

## 更新

- 将 Host API 路由拆到 `host_api.rs`。
- 将权限裁决拆到 `permission_guard.rs`。
- 将 workspace / process / http API 分别拆到独立模块。
- `ActiveExtension` 预构造并持有 `ToolExecutor`，workspace/process handler 不再每次重复创建 executor。
- 保持 `runtime.invoke` 与 `extension.channel_invoke` 当前行为，不在本批扩大 contract。

## 涉及文件

- `crates/agentdash-local/src/extensions/host/mod.rs`
- `crates/agentdash-local/src/extensions/host/process.rs`
- `crates/agentdash-local/src/extensions/host/manager.rs`
- `crates/agentdash-local/src/extensions/host/host_api.rs`
- `crates/agentdash-local/src/extensions/host/permission_guard.rs`
- `crates/agentdash-local/src/extensions/host/workspace_api.rs`
- `crates/agentdash-local/src/extensions/host/process_api.rs`
- `crates/agentdash-local/src/extensions/host/http_api.rs`
- `crates/agentdash-local/src/extensions/host/permissions.rs`

## 验证

- `cargo test -p agentdash-local host_api`：10 passed。
- `cargo test -p agentdash-local workspace_host_apis`：3 passed。
- `cargo test -p agentdash-local process_host_apis`：1 passed。
- `cargo check -p agentdash-local`：通过；仅剩既有 `agentdash-executor` unused import warning。
- `cargo fmt --check --package agentdash-local`：通过。
- `git diff --check`：通过。
- 汇合验证：`cargo test -p agentdash-local`：86 passed。

## Commit

`f9f53388 refactor(local-runtime): 收敛本机运行时模块边界`
