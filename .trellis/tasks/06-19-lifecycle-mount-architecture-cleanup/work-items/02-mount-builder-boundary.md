# Work Item 02: Mount Builder Boundary

## Status

Completed in `ba0b7fc3`; test-fixture export follow-up in `82996e7c`.

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

- Passed: `cargo test -p agentdash-application lifecycle::surface::mount`
- Passed: `cargo test -p agentdash-application session::assembly_builder`
- Passed: `cargo test -p agentdash-application workflow::orchestration`
- Passed: `cargo test -p agentdash-application session::assembler`

## Outcome

- Removed lifecycle mount wrapper builders.
- Lowered lifecycle mount builder visibility and removed `vfs` public facade exports.
- Kept active workflow lifecycle helper inside crate-local lifecycle surface boundary.
- Restricted test-only lifecycle mount helpers to test builds.
- Updated session/workflow test fixtures to avoid public VFS builder calls.
