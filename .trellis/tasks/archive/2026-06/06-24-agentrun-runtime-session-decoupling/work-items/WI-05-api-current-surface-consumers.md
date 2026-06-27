# WI-05 API, VFS And Terminal Current Surface Consumers

Status: done

Assigned Worker: Codex WI-05

## Tracking

- Files changed:
  - `crates/agentdash-api/src/agent_run_runtime_surface.rs`
  - `crates/agentdash-api/src/lib.rs`
  - `crates/agentdash-api/src/routes/canvases.rs`
  - `crates/agentdash-api/src/routes/extension_runtime.rs`
  - `crates/agentdash-api/src/routes/terminals.rs`
  - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs`
  - `crates/agentdash-application/src/agent_run/mod.rs`
  - `crates/agentdash-application/src/agent_run/runtime_surface.rs`
- Tests run:
  - `cargo fmt -p agentdash-application -p agentdash-api`
  - `cargo test -p agentdash-application agent_run::runtime_surface::tests::terminal_target`
  - `cargo check -p agentdash-api`
- Blockers: 无。
- Handoff summary: API current/resource surface adapter 已改为 AgentRun runtime surface 命名。VFS `SessionRuntime` / `AgentRun` source resolution 消费 WI-02 引入的 AgentRun resource surface facade。Terminal launch target 推导已从 API route 迁入 AgentRun runtime surface facade；API route 只保留权限、DTO、backend 在线校验和 relay command dispatch。

## Purpose

Migrate API current-surface, VFS and Terminal consumers to AgentRun facades so routes stop assembling current/resource surface from session construction helpers or route-local anchor selection.

## Dependencies

- `WI-02`

## Scope

- Rename or move `agentdash-api/src/session_construction.rs` to an AgentRun runtime surface adapter.
- Terminal launch target derivation consumes application runtime placement facade.
- VFS `SessionRuntime` / `AgentRun` sources consume AgentRun resource surface facade.

## Out Of Scope

- `routes/sessions.rs` and `routes/lifecycle_views.rs` presentation read-model cleanup belongs to `WI-08`.
- Canvas/Extension Project/session binding guards belong to `WI-11`.

## Deliverables

- Route code performs auth/DTO/error mapping only.
- Terminal/VFS current surface tests updated.

## Acceptance

- `cargo check -p agentdash-api` passes.
- VFS AgentRun latest-anchor selection is no longer route-owned.
