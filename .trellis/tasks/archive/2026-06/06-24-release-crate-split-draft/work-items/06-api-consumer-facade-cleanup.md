# Work Item 06: API Consumer Facade Cleanup

## Objective

让 API route/helper 层消费 application facades，原因是 route 只需要 auth、DTO、path parsing 和错误映射，current/resource surface closure 属于 application use case。

## Owns

- `crates/agentdash-api/src/agent_run_runtime_surface.rs`
- `crates/agentdash-api/src/app_state.rs`
- `crates/agentdash-api/src/bootstrap/**`
- selected routes: canvases, extension_runtime, terminals, vfs_surfaces, sessions, lifecycle_views

## Implementation Strategy

1. Add AppState-owned `Arc<dyn AgentRunRuntimeSurfaceQueryPort>` and resource/terminal facade handles.
2. Update `agent_run_runtime_surface.rs` to consume shared handles.
3. Move terminal launch target derivation behind application facade.
4. Move extension workspace/runtime placement selection behind application facade.
5. Keep presentation/debug frame views separate from RuntimeGateway/current-surface DTOs.

## Completion Gates

```powershell
cargo check -p agentdash-api
cargo test -p agentdash-api agent_run_runtime_surface
rg -n "AgentRunRuntimeSurfaceQuery::new|AgentRunRuntimeSurfaceQueryDeps|runtime_surface_query\\(" crates/agentdash-api/src -g '*.rs'
rg -n "agentdash_application::session::(construction|plan|types|hub)|agentdash_application::agent_run::frame|agentdash_application::vfs::ResolvedVfsSurfaceSource|agentdash_application::vfs::build_surface_summary" crates/agentdash-api/src -g '*.rs'
```

## Handoff

Report route/helper imports removed, route imports that remain by design, and any AppState constructor signature changes.
