# Work Item 03: AgentRun Surface Facade

## Objective

把 AgentRun current/resource surface、effective capability/admission、terminal/runtime placement 固化为可跨 crate 消费的 facade/port，确保 current surface 使用 current frame，launch frame 只作为 evidence/provenance。

## Owns

- `crates/agentdash-application/src/agent_run/runtime_surface.rs`
- `crates/agentdash-application/src/agent_run/runtime_surface_update.rs`
- `crates/agentdash-application/src/agent_run/effective_capability.rs`
- `crates/agentdash-application/src/agent_run/workspace/**`
- AgentRun adapters implementing ports from Work Item 01

## Implementation Strategy

1. Move shared DTOs/traits to `agent_run_surface` where needed.
2. Add `AgentRunResourceSurfaceQueryPort` and terminal/runtime placement facade.
3. Fix resource/workspace address DTOs so `frame_id` means current surface frame; keep launch frame in evidence fields.
4. Keep RuntimeGateway reduced MCP port separate from full AgentRun surface.
5. Expose effective capability/admission through AgentRun-owned ports for RuntimeSession/tool consumers.

## Completion Gates

```powershell
cargo test -p agentdash-application agent_run::runtime_surface
cargo test -p agentdash-application agent_run::runtime_surface_update
cargo test -p agentdash-application agent_run::permission_runtime_surface_update
rg -n "selection\\.launch_frame_id" crates/agentdash-application/src/agent_run/workspace/query.rs crates/agentdash-api/src -g '*.rs'
```

## Handoff

Report DTO moves, trait impls, current/launch frame handling, and consumers still importing AgentRun implementation internals.
