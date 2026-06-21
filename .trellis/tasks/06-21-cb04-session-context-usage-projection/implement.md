# CB04-C Implementation Plan

## Steps

- Move context usage helper logic from contracts into application session projection.
- Replace application call sites to use the new application helper/read model.
- Keep response DTO construction at API/stream boundary.
- Remove now-invalid contracts dependency on SPI context analysis.

## Validation

```powershell
cargo test -p agentdash-application session::eventing
cargo test -p agentdash-contracts runtime::session
pnpm run contracts:check
```

## Dispatch Notes

- Good first-wave task.
- Low conflict with MCP preset and Routine/LLM/Settings conversion cleanup.
