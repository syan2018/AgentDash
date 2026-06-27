# Work Item 01: Ports Boundary Expansion

## Objective

扩展 `agentdash-application-ports`，先把跨 crate 共享的 DTO/trait/error 固定下来，让后续 implementation crates 只通过 ports 连接。

## Owns

- `crates/agentdash-application-ports/Cargo.toml`
- `crates/agentdash-application-ports/src/lib.rs`
- `crates/agentdash-application-ports/src/*.rs`
- application implementation 中为满足新 port trait 所需的最小 adapter impl。

## Port Modules

- `agent_run_surface`
- `runtime_session_delivery`
- `runtime_surface_adoption`
- `frame_launch_envelope`
- `lifecycle_surface_projection`
- `lifecycle_materialization`
- `agent_frame_materialization`
- `runtime_gateway_setup`
- `vfs_surface_runtime`

Keep `runtime_gateway_mcp_surface` as the reduced MCP-specific port.

## Implementation Strategy

1. Add modules and re-export them from ports `lib.rs`.
2. Move or mirror pure DTOs before moving concrete implementations.
3. Keep repository/service constructors in application crates; ports expose traits and request/result structs only.
4. Use `cargo check -p agentdash-application-ports` as the first tight loop.
5. Add application adapter impls only when consumers need the trait object immediately.

## Completion Gates

```powershell
cargo check -p agentdash-application-ports
cargo check -p agentdash-application
```

## Handoff

List every new port module, DTO names, trait names, and which later work item owns implementation rewiring.
