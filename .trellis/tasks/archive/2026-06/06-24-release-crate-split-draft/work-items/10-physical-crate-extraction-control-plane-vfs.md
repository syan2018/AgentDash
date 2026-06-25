# Work Item 10: Physical Crate Extraction Control Plane And VFS

## Objective

抽取 `agentdash-application-agentrun`、`agentdash-application-lifecycle`，并在依赖方向满足时抽取 generic `agentdash-application-vfs`。

## Owns

- workspace `Cargo.toml`
- `crates/agentdash-application-agentrun/**`
- `crates/agentdash-application-lifecycle/**`
- optional `crates/agentdash-application-vfs/**`
- umbrella application facade cleanup

## Implementation Strategy

1. Move AgentRun after Session/Lifecycle implementation imports are port-mediated.
2. Move Lifecycle after RuntimeSession creation and AgentRun materialization/update are ports.
3. Keep workflow runtime/reducer with Lifecycle crate.
4. Move VFS core only after owner providers are directional.
5. Use `cargo metadata` after each move and commit after each crate reaches a useful boundary.

## Completion Gates

```powershell
cargo metadata --no-deps --format-version 1
cargo check -p agentdash-application-agentrun
cargo check -p agentdash-application-lifecycle
cargo check --workspace
rg -n "crate::session::|crate::lifecycle::|crate::canvas::" crates/agentdash-application-vfs/src -g '*.rs'
```

## Handoff

Report extracted crates, remaining umbrella facade role, deferred VFS owner providers, and final workspace check status.
