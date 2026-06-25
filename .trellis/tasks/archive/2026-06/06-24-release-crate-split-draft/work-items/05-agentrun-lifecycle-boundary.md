# Work Item 05: AgentRun Lifecycle Boundary

## Objective

打断 AgentRun 与 Lifecycle 之间的 implementation import cycle：AgentRun 消费 Lifecycle projection/materialization ports；Lifecycle 消费 RuntimeSession creation 和 AgentRun frame materialization/update ports。

## Owns

- `crates/agentdash-application/src/lifecycle/**`
- `crates/agentdash-application/src/workflow/orchestration/**`
- AgentRun/Lifecycle shared DTO moves to ports
- materialization adapters

## Implementation Strategy

1. Move `AgentRunRuntimeAddress` and lifecycle projection input/output DTOs to ports.
2. Expose `LifecycleSurfaceProjectionPort`; implement it with current projector.
3. Move `RuntimeSessionCreator` contract to `runtime_session_delivery`.
4. Replace direct `AgentFrameBuilder` handoff with AgentRun frame materialization/update port.
5. Keep workflow runtime/reducer with Lifecycle implementation for this split.

## Completion Gates

```powershell
cargo check -p agentdash-application
rg -n "AgentFrameBuilder" crates/agentdash-application/src/lifecycle crates/agentdash-application/src/workflow/orchestration -g '*.rs'
rg -n "crate::lifecycle::.*AgentRunRuntimeAddress|crate::lifecycle::surface::surface_projector|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-application/src/agent_run crates/agentdash-application/src/session -g '*.rs'
```

## Handoff

Report which AgentRun/Lifecycle links are now ports, which reducer/materialization paths still need concrete implementation, and any DTO naming conflicts.
