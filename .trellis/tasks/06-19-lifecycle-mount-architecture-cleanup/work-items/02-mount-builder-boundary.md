# Work Item 02: Mount Builder Boundary

## Status

Planned.

## Goal

让低层 lifecycle mount builder 只从最终 mount spec 生成 `Mount`，业务层不能绕过 projector 直接重建 AgentRun lifecycle mount。

## Scope

- 删除 `build_lifecycle_mount` 与 `build_lifecycle_mount_with_ports` wrapper。
- 降低 session evidence builder 和 node runtime builder 的可见性。
- 收束 `LifecycleMountSurface`、`append_active_workflow_lifecycle_mount`、`project_active_workflow_lifecycle_vfs`。
- 更新仍直接调用低层 builder 的测试夹具。

## Affected Areas

- `crates/agentdash-application/src/vfs/mount_lifecycle.rs`
- `crates/agentdash-application/src/vfs/mod.rs`
- `crates/agentdash-application/src/lifecycle/surface/mount.rs`
- `crates/agentdash-application/src/task/context_builder.rs`
- `crates/agentdash-application/src/session/assembly_builder.rs`
- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs`

## Dependencies

依赖 Work Item 01 的 projector facts 和场景化入口，否则调用方没有统一替代路径。

## Validation

- `cargo test -p agentdash-application lifecycle::surface::mount`
- `cargo test -p agentdash-application session::assembly_builder`
- `cargo test -p agentdash-application workflow::orchestration`
