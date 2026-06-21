# CB04-B Implementation Plan

## Steps

- Wait for `DeliveryRuntimeSelectionService` and current delivery binding.
- Introduce application read model for AgentRun workspace and conversation snapshot facts.
- Move contract DTO construction into API adapter mapping.
- Update command policy to consume core read model / resolver facts.

## Validation

```powershell
cargo test -p agentdash-application agent_run
cargo test -p agentdash-api lifecycle_agents --lib
pnpm run contracts:check
pnpm run frontend:check
```

## Dispatch Notes

- Not first-wave.
- Dispatch only after Runtime Coordinate current delivery binding is implemented.
