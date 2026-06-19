# Work Item 01: Lifecycle Surface Facts

## Status

Planned.

## Goal

把 AgentRun lifecycle surface 的业务入口收束到 `AgentRunLifecycleSurfaceProjector` 的场景化入口，让 workspace query、VFS surface resolver、owner bootstrap、session assembler 只负责收集上下文事实。

## Scope

- 定义 typed projection facts，区分 session evidence、node runtime、message stream、SkillAsset projection。
- 增加 projector 场景化入口，替代调用方手写 `AgentRunLifecycleSurfaceInput`。
- 移除 `node_projection.is_some()` 推导 mode 的路径。
- VFS surface resolver 使用只读 node evidence ref，不能再填空 `lifecycle_key` / 空 writable ports 来复用可写 node runtime projection。

## Affected Areas

- `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs`
- `crates/agentdash-application/src/agent_run/workspace/query.rs`
- `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs`
- `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs`
- `crates/agentdash-application/src/session/assembler.rs`

## Dependencies

无前置 work item；这是后续 builder 边界和 metadata refresh 的事实基础。

## Validation

- `cargo test -p agentdash-application lifecycle::surface`
- `cargo test -p agentdash-application agent_run::workspace`
- `cargo check -p agentdash-api`
