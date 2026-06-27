# Work Item 04: RuntimeSession Substrate Boundary

## Objective

把 `session` 收束为 RuntimeSession delivery/trace substrate，并通过 ports 倒置 launch、live adoption、mailbox、effective capability 等 AgentRun/Lifecycle 依赖。

## Owns

- `crates/agentdash-application/src/session/**`
- RuntimeSession implementations of delivery/adoption ports
- session-facing adapter construction in API bootstrap

## Implementation Strategy

1. Split public `session::types` and keep public facade limited to delivery/trace/turn/event/projection use cases.
2. Change launch deps to consume `frame_launch_envelope` and accepted launch commit/bootstrap status ports.
3. Change live adoption implementation to implement `runtime_surface_adoption` port so RuntimeSession stays a live adapter for AgentRun-owned surface updates.
4. Replace direct AgentFrame/Lifecycle/Permission/mailbox repository fields with injected ports where semantics cross business boundaries.
5. Keep connector live tool refresh and active turn cache in RuntimeSession implementation.

## Completion Gates

```powershell
cargo check -p agentdash-application
rg -n "use crate::agent_run" crates/agentdash-application/src/session -g '*.rs'
rg -n "use crate::lifecycle" crates/agentdash-application/src/session -g '*.rs'
```

## Handoff

Report which session imports became ports, which live hub fields remain, and which compile errors belong to AgentRun/Lifecycle owners.
