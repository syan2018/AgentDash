# Work Item 07: VFS Resource Surface Boundary

## Objective

把 AgentRun resource surface 与 generic VFS core 分清：resource browser/current runtime surface 从 AgentRun facade 进入，generic VFS core 只负责 provider/path/summary/materialization/mutation mechanics。

## Owns

- `crates/agentdash-application/src/vfs/**`
- VFS summary/runtime projection ports and adapters
- VFS route helper changes that belong to VFS preview/resource facade wiring

## Implementation Strategy

1. Move `VfsSurfaceRuntimeProjection` or equivalent to `vfs_surface_runtime` port.
2. Keep generic summary builder independent of API/AppState concrete state.
3. Move Project/Story/Task preview VFS construction behind an application VFS preview facade.
4. Keep owner-specific providers with owner modules until dependencies are directional.
5. Prepare generic VFS core file set for later physical move.

## Completion Gates

```powershell
cargo check -p agentdash-application
cargo check -p agentdash-api
rg -n "crate::session::|crate::lifecycle::|crate::canvas::" crates/agentdash-application/src/vfs -g '*.rs'
```

## Handoff

Report generic VFS files ready for extraction, owner-specific providers left in place, and API route callers that now consume preview/resource facades.
