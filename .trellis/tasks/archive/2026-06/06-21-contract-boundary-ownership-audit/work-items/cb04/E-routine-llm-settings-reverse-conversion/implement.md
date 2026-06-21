# CB04-E Implementation Plan

## Steps

- Identify reverse conversion impls for Routine, LLM provider and Settings.
- Move each incoming conversion to the owning route/application command mapper.
- Keep or rename outbound conversions so direction is explicit.
- Add focused tests around request parsing and generated contract check.

## Validation

```powershell
cargo test -p agentdash-contracts routine llm_provider settings
cargo test -p agentdash-api routine llm settings --lib
pnpm run contracts:check
```

## Dispatch Notes

- Good first-wave task.
- Avoid touching MCP preset and backend access conversion files.
