# CB04-B Implementation Plan

## Steps

- [x] Wait for `DeliveryRuntimeSelectionService` and current delivery binding.
- [x] Introduce application read model for AgentRun workspace and conversation snapshot facts.
- [x] Move contract DTO construction into API adapter mapping.
- [x] Update command policy to consume core read model / resolver facts.

## Completion Notes

- `agent_run::workspace` and `agent_run::conversation_snapshot` now return application read models instead of browser-facing contract DTOs.
- `agentdash-api/src/routes/lifecycle_agents.rs` maps AgentRun workspace/conversation read models into generated workflow contract DTOs.
- Command precondition parsing is API-owned: request DTO command kind/stale guard are mapped into application command precondition models before command policy evaluation.
- `resource_surface_coordinate` remains an application model and is mapped at the lifecycle API adapter boundary, preserving RC08 `surface_frame_ref` and `source_anchor` semantics.

## Validation

```powershell
cargo test -p agentdash-application agent_run
cargo test -p agentdash-api lifecycle_agents --lib
pnpm run contracts:check
pnpm run frontend:check
```

Focused validation completed in this slice:

- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo test -p agentdash-application workspace --lib`
- `cargo test -p agentdash-application snapshot_preserves_resource_surface_coordinate --lib`
- `cargo test -p agentdash-api lifecycle_agents --lib`
- `cargo test -p agentdash-api project_agent --lib`
- `pnpm run contracts:check`
- `pnpm run frontend:check`

## Dispatch Notes

- Completed after Runtime Coordinate current delivery binding and resource surface coordinate stabilized.
