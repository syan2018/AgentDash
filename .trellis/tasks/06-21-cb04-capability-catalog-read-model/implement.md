# CB04-D Implementation Plan

## Steps

- Introduce application read model types for capability catalog.
- Change catalog service to return application read model.
- Add adapter mapping in API route/module to contract DTO.
- Update tests and generated contract check.

## Validation

```powershell
cargo test -p agentdash-application capability
cargo test -p agentdash-api capability --lib
pnpm run contracts:check
```

## Dispatch Notes

- Second-wave task unless no worker is editing capability/exposure modules.
