# CB04-F Implementation Plan

## Steps

- Audit backend access/status/mode conversion call sites.
- Classify each conversion as outbound projection or incoming command parsing.
- Move incoming command parsing to route/application boundary if found.
- Record any keep decisions in owner-map or relevant spec.

## Validation

```powershell
cargo test -p agentdash-contracts backend
cargo test -p agentdash-api backend --lib
pnpm run contracts:check
```

## Dispatch Notes

- Not first-wave unless a research worker is available.
- Keep write scope away from MCP preset and Routine/LLM/Settings conversion.
