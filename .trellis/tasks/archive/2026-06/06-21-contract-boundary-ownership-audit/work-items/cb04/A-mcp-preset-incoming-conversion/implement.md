# CB04-A Implementation Plan

## Steps

- Locate all reverse conversions in MCP preset contracts and their route/application callers.
- Move incoming conversion into API/application mapper with names that express command parsing.
- Keep outbound DTO projection in contracts.
- Update tests around create/update/probe mapping and run focused contract checks.

## Validation

```powershell
cargo test -p agentdash-contracts mcp_preset
cargo test -p agentdash-api mcp_preset --lib
pnpm run contracts:check
```

## Dispatch Notes

- Good first-wave task.
- Avoid parallel edits with CB04-E only if shared generated contract exports are touched.
